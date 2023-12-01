// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::Form;
use chrono::Duration;
use chrono::TimeZone;
use chrono::Utc;
use constant_time_eq::constant_time_eq;
use entities::mail_verification_tokens;
use lettre::message::Mailbox;
use lettre::Message;
use lettre::SmtpTransport;
use lettre::Transport;
use rand::distributions::Alphanumeric;
use rand::distributions::DistString;
use reqwest::Url;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue;
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::ModelTrait;
use sea_orm::QueryFilter;
use sea_orm::TransactionTrait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tower_sessions::Session;

use super::get_specific_login_host;
use super::url_path_alerts;
use super::url_path_overview;
use super::Result;
use entities::instance_mail;

/// Admin login form
#[derive(Deserialize, Debug)]
pub struct AddEmailForm {
    mail: String,
    instance: i32,
}
//let verifier_entry = mail_verification_tokens::Entity::find().filter(mail_verification_tokens::Column::KnownPart.eq(input.))
pub async fn add_mail(
    State(ref config): State<Arc<crate::Config>>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    session: Session,
    Form(form): Form<AddEmailForm>,
) -> Result<axum::response::Response> {
    let back_url: String = back_url_host_alerts(form.instance);

    let transaction = db.begin().await?;

    let host = get_specific_login_host(form.instance, &session, &transaction).await?;

    let mail = host
        .find_related(instance_mail::Entity)
        .one(&transaction)
        .await?;

    if mail.is_some() {
        transaction.rollback().await?;
        return super::render_error_page(
            template,
            "Invalid operation",
            "Can't add another email, please remove the current one.",
            &back_url,
        );
    }

    // invalidate old tokens
    mail_verification_tokens::Entity::delete_by_id(host.id)
        .exec(&transaction)
        .await?;
    // insert new validation token
    let (secret, new_token_model) =
        generate_mail_token(&form.mail, host.id, config.mail_token_ttl_s);
    let token_model = new_token_model.insert(&transaction).await?;
    tracing::trace!(secret = secret, hashed = token_model.secret_part);

    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    let mut url = Url::parse(&config.site_url).expect("invalid site url");
    url.set_path(&format!(
        "/admin/mail/activate/{}/{}",
        token_model.public_part, secret
    ));
    context.insert("ACTIVATION_LINK", url.as_str());

    let mail_body = template.render("mail_activation.j2", &context)?;

    let address: Mailbox = match form.mail.parse() {
        Ok(v) => v,
        Err(e) => {
            transaction.rollback().await?;
            tracing::info!(error=?e, address=form.mail,"Failed to parse email address");
            return super::render_error_page(
                template,
                "Invalid email address",
                "Your email address seems to be invalid.",
                &back_url,
            );
        }
    };

    let email = Message::builder()
        // Addresses can be specified by the tuple (email, alias)
        .to(address)
        // ... or by an address only
        .from(config.mail_from.parse()?)
        .header(lettre::message::header::ContentType::TEXT_PLAIN)
        .subject(format!("Mail Activation for {}", config.site_url))
        .body(mail_body)?;

    let smtp_credentials = lettre::transport::smtp::authentication::Credentials::new(
        config.mail_smtp_user.clone(),
        config.mail_smtp_password.clone(),
    );

    // Open a local connection on port 25
    let mailer = SmtpTransport::relay(&config.mail_smtp_host)
        .unwrap()
        .credentials(smtp_credentials)
        .build();
    // Send the email
    match mailer.send(&email) {
        Ok(_) => (),
        Err(e) => {
            tracing::info!(error=?e, address=form.mail,"Failed to send validation mail");
            return super::render_error_page(
                template,
                "Failed to send email",
                "Couldn't send email.",
                &back_url,
            );
        }
    }

    transaction.commit().await?;

    let eol_formatted = Utc
        .timestamp_opt(token_model.eol_date, 0)
        .unwrap()
        .format("%d.%m.%Y %H:%M")
        .to_string();

    let mut context = tera::Context::new();
    context.insert("EMAIL", &host.domain);
    context.insert("MAIL_VALID_UNTIl", &eol_formatted);
    context.insert("URL_BACK", &back_url);

    let res = Html(template.render("mail_send.html.j2", &context)?).into_response();
    Ok(res)
}

/// Admin login form
#[derive(Deserialize, Debug)]
pub struct ActivateEmailPath {
    public: String,
    secret: String,
}
/// Confirmation view for mail activation links
pub async fn activate_mail_view(
    State(ref config): State<Arc<crate::Config>>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<ActivateEmailPath>,
) -> Result<axum::response::Response> {
    let verification_token = mail_verification_tokens::Entity::find()
        .filter(mail_verification_tokens::Column::PublicPart.eq(&instance.public))
        .one(db)
        .await?;

    let verification_token = match verification_token {
        Some(v) => v,
        None => {
            return super::render_error_page(
                template,
                "Invalid Activation Token",
                "Activation link outdated or invalid.",
                url_path_overview(),
            )
        }
    };

    let mut context = tera::Context::new();
    context.insert("SITE_URL", &config.site_url);
    context.insert("MAIL_PUBLIC_TOKEN", &instance.public);
    context.insert("MAIL_SECRET_TOKEN", &instance.secret);
    context.insert("MAIL", &verification_token.mail);

    let res = Html(template.render("mail_activate_confirm.html.j2", &context)?).into_response();
    Ok(res)
}

/// Activate email via form post
pub async fn activate_mail(
    State(ref config): State<Arc<crate::Config>>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Form(form): Form<ActivateEmailPath>,
) -> Result<axum::response::Response> {
    let transaction = db.begin().await?;

    let verification_token = mail_verification_tokens::Entity::find()
        .filter(mail_verification_tokens::Column::PublicPart.eq(&form.public))
        .one(&transaction)
        .await?;

    let verification_token = match verification_token {
        None => {
            return super::render_error_page(
                template,
                "Invalid Activation Token",
                "Activation link outdated or invalid.",
                url_path_overview(),
            )
        }
        Some(v) => v,
    };

    if verification_token.is_outdated() {
        return super::render_error_page(
            template,
            "Expired Activation Token",
            "Activation link expired.",
            url_path_overview(),
        );
    }

    if !verify_token(&form.secret, &verification_token.secret_part) {
        return super::render_error_page(
            template,
            "Invalid secret token",
            "Secret part is invalid.",
            url_path_overview(),
        );
    }

    // set mail for host
    // this could error if we get a glitch where an activation link is somehow valid while an email is bound
    instance_mail::Entity::insert(instance_mail::ActiveModel {
        host: ActiveValue::Set(verification_token.host),
        mail: ActiveValue::Set(verification_token.mail),
    })
    .exec(&transaction)
    .await?;

    // remove activation token
    mail_verification_tokens::Entity::delete_by_id(verification_token.host)
        .exec(&transaction)
        .await?;

    transaction.commit().await?;

    let mut context = tera::Context::new();
    context.insert("EMAIL", &config.site_url);
    context.insert("URL_ALERTS", &url_path_alerts(verification_token.host));

    let res = Html(template.render("mail_activate_success.html.j2", &context)?).into_response();
    Ok(res)
}

fn generate_mail_token(
    mail: &str,
    host: i32,
    lifetime_s: i64,
) -> (String, mail_verification_tokens::ActiveModel) {
    let public = Alphanumeric.sample_string(&mut rand::thread_rng(), 16);
    let secret = Alphanumeric.sample_string(&mut rand::thread_rng(), 20);

    let secret_hashed_encoded: String = hash_and_encode(secret.as_bytes());

    let eol = Utc::now() + Duration::seconds(lifetime_s);

    (
        secret,
        mail_verification_tokens::ActiveModel {
            host: ActiveValue::Set(host),
            mail: ActiveValue::Set(mail.to_owned()),
            public_part: ActiveValue::Set(public),
            secret_part: ActiveValue::Set(secret_hashed_encoded),
            eol_date: ActiveValue::Set(eol.timestamp()),
        },
    )
}

fn hash_and_encode(data: &[u8]) -> String {
    let mut hasher: Sha256 = Sha256::new();
    hasher.update(&data);
    let data_hashed = hasher.finalize();
    base16ct::upper::encode_string(&data_hashed)
}

/// Verify activation token to hashed secret
fn verify_token(activation_token: &str, hashed_secret: &str) -> bool {
    tracing::trace!(secret = activation_token);
    let hex_hashes_secret = hash_and_encode(activation_token.as_bytes());
    tracing::debug!(expected=?hashed_secret,input=?hex_hashes_secret);
    constant_time_eq(hashed_secret.as_bytes(), hex_hashes_secret.as_bytes())
}

fn back_url_host_alerts(instance: i32) -> String {
    format!("/admin/alerts/{instance}")
}
