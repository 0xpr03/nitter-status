// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::Form;
use constant_time_eq::constant_time_eq;
use entities::host;
use hyper::header::REFERER;
use hyper::HeaderMap;
use hyper::StatusCode;
use reqwest::Client;
use reqwest::Url;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tower_sessions::Session;
use trust_dns_resolver::config::ResolverConfig;
use trust_dns_resolver::config::ResolverOpts;
use trust_dns_resolver::AsyncResolver;

use super::get_session_login;
use crate::Config;
use crate::Result;
use crate::ADMIN_OVERVIEW_URL;
use crate::LOGIN_URL;

/// Stored session login information
#[derive(Serialize, Deserialize, Default)]
pub struct ActiveLogin {
    /// Hosts this session has access to.
    pub hosts: HashSet<i32>,
    pub admin: bool,
}
pub const LOGIN_KEY: &'static str = "LOGIN";

/// Error shown to user, details aren't part of the error message, as they're displayed separately.
#[derive(Error, Debug)]
pub enum LoginError {
    #[error("Couldn't get login file at '{0}'")]
    HttpFailure(Url, reqwest::Error),
    #[error("Invalid instance URL '{0}'")]
    InstanceUrl(String),
    #[error("Invalid response at '{0}'")]
    InvalidResponse(Url, String),
    #[error("Key mismatch")]
    KeyMismatch,
    #[error("No public instance host found with domain '{0}'")]
    HostNotFound(String),
    #[error("Host is not in the public instances listed right now")]
    DisabledHost(String),
    #[error("Server responded with status code '{0}'")]
    ServerResponse(u16, String),
    #[error("Invalid hash, found:")]
    InvalidHash(String),
    #[error("Failed to fetch DNS TXT records")]
    DNSError(#[from] trust_dns_resolver::error::ResolveError),
    #[error("No valid DNS TXT entry found for your key, found:")]
    DNSNoValidEntry(String),
}
type LoginResult<T> = std::result::Result<T, LoginError>;

pub async fn logout(session: Session) -> Result<axum::response::Response> {
    session.delete();
    Ok(Redirect::temporary(LOGIN_URL).into_response())
}

pub async fn login(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref config): State<Arc<crate::Config>>,
    State(ref login_client): State<Client>,
    State(ref db): State<DatabaseConnection>,
    session: Session,
    Form(input): Form<LoginInput>,
) -> Result<axum::response::Response> {
    tracing::debug!(login=?input);
    let domain = input.domain.trim();
    let host = host::Entity::find()
        .filter(host::Column::Domain.eq(domain))
        .one(db)
        .await?;

    if host
        .as_ref()
        .map(|host| {
            get_session_login(&session)
                .map(|login| login.hosts.contains(&host.id))
                .is_ok()
        })
        .unwrap_or_default()
    {
        // already logged in
        return Ok(Redirect::temporary(&input.referrer).into_response());
    }

    match login_inner(config, login_client, &input, host).await {
        Ok(host) => {
            let session_value = match get_session_login(&session) {
                Ok(mut session) => {
                    session.hosts.insert(host.id);
                    session
                }
                Err(_) => {
                    let mut ids = HashSet::with_capacity(1);
                    ids.insert(host.id);
                    ActiveLogin {
                        hosts: ids,
                        admin: config.admin_domains.iter().any(|e| e == domain),
                    }
                }
            };
            session.insert(LOGIN_KEY, session_value)?;
            let referrer = input.referrer.trim();
            let location = match referrer.trim().is_empty() && referrer != LOGIN_URL {
                true => ADMIN_OVERVIEW_URL,
                false => input.referrer.trim(),
            };
            let mut res = Redirect::temporary(location).into_response();
            *res.status_mut() = StatusCode::FOUND;
            Ok(res)
        }
        Err(e) => {
            tracing::debug!(login_error=?e);
            let mut context = tera::Context::new();
            context.insert("REFERRER", &input.referrer);
            context.insert("ERROR", &e.to_string());
            context.insert("LOGIN_METHOD", &input.verification_method);
            context.insert("DOMAIN", &input.domain);
            context.insert("VERIFY_TOKEN_NAME", &config.login_token_name);
            match e {
                LoginError::InvalidResponse(_, val)
                | LoginError::ServerResponse(_, val)
                | LoginError::DNSNoValidEntry(val)
                | LoginError::InvalidHash(val) => context.insert("QUOTE", &val),
                _ => (),
            }
            let mut res = Html(template.render("login.html.j2", &context)?).into_response();
            *res.status_mut() = StatusCode::FORBIDDEN;
            Ok(res.into_response())
        }
    }
}

async fn login_inner(
    config: &crate::Config,
    login_client: &Client,
    input: &LoginInput,
    host: Option<host::Model>,
) -> LoginResult<host::Model> {
    let host = host.ok_or_else(|| LoginError::HostNotFound(input.domain.clone()))?;

    if !host.enabled {
        return Err(LoginError::DisabledHost(input.domain.clone()));
    }

    match input.verification_method {
        VerificationMethod::DNS => {
            let entries = fetch_host_dns(&host.domain, &config).await?;
            for entry in &entries {
                if let Ok(_) = verify_key(entry, &input.key) {
                    return Ok(host);
                }
            }
            Err(LoginError::DNSNoValidEntry(entries.join(",")))
        }
        VerificationMethod::HTTP => {
            let fetched_key = fetch_host_txt(&host.url, login_client, &config).await?;
            verify_key(fetched_key.trim(), &input.key).map(|_| host)
        }
    }
}

/// Admin login form
#[derive(Deserialize, Debug)]
pub struct LoginInput {
    domain: String,
    key: String,
    referrer: String,
    verification_method: VerificationMethod,
}

/// Part of the admin login form
#[derive(Serialize, Deserialize, Debug)]
enum VerificationMethod {
    DNS,
    HTTP,
}

async fn fetch_host_txt(
    instance_url: &str,
    client: &Client,
    config: &Config,
) -> LoginResult<String> {
    let mut request_url =
        Url::parse(&instance_url).map_err(|_| LoginError::InstanceUrl(instance_url.to_string()))?;
    request_url.set_path(&[".well-known/", &config.login_token_name].concat());
    request_url.set_query(None);
    let result = client
        .get(request_url.clone())
        .send()
        .await
        .map_err(|e| LoginError::HttpFailure(request_url.clone(), e))?;
    match result.status().is_success() {
        true => Ok(result.text().await.map_err(|e: reqwest::Error| {
            LoginError::InvalidResponse(request_url, e.to_string())
        })?),
        false => Err(LoginError::ServerResponse(
            result.status().as_u16(),
            result.text().await.unwrap_or_default(),
        )),
    }
}
async fn fetch_host_dns(instance_domain: &str, config: &Config) -> LoginResult<Vec<String>> {
    // TODO: cache resolver ?
    let resolver = AsyncResolver::tokio(ResolverConfig::cloudflare_tls(), ResolverOpts::default());
    let hashed_key = resolver
        .txt_lookup(format!("{}.{}.", &config.login_token_name, instance_domain))
        .await?;

    let mut entries = Vec::with_capacity(2);
    for record in hashed_key.iter() {
        for data in record.iter() {
            match std::str::from_utf8(data) {
                Ok(val) => entries.push(val.to_owned()),
                Err(_) => (),
            }
        }
    }
    Ok(entries)
}
/// Verify a key with its public available hash
/// Key is in base16 (hex) and has to match the hash passed in after SHA2 encoding it.
fn verify_key(hash: &str, key: &str) -> LoginResult<()> {
    let mut hasher: Sha256 = Sha256::new();
    hasher.update(key);
    let res = hasher.finalize();
    let mut decoded_hash = [0; 32];
    base16ct::mixed::decode(hash, &mut decoded_hash).map_err(|e| {
        tracing::trace!(hash_error=?e);
        LoginError::InvalidHash(hash.to_owned())
    })?;
    match constant_time_eq(&res, &decoded_hash) {
        false => Err(LoginError::KeyMismatch),
        true => Ok(()),
    }
}

pub async fn login_view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref config): State<Arc<crate::Config>>,
    headers: HeaderMap,
) -> Result<axum::response::Response> {
    tracing::debug!(headers=?headers);
    let referrer = headers.get(REFERER).and_then(|v| v.to_str().ok());
    let mut context = tera::Context::new();
    context.insert("REFERRER", &referrer); // FIXME: won't work, handle this in the error part to extract the current situation
    context.insert("VERIFY_TOKEN_NAME", &config.login_token_name);
    let res = Html(template.render("login.html.j2", &context)?).into_response();
    Ok(res)
}