// SPDX-License-Identifier: AGPL-3.0-only
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use about_parser::AboutParser;
use chrono::Utc;
use entities::{
    host,
    prelude::*,
    state::{scanner::ScannerConfig, Cache},
};
use instance_parser::InstanceParser;
use profile_parser::ProfileParser;
use regex::{Regex, RegexBuilder};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client, Url,
};
use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait,
    DatabaseConnection, DbBackend, EntityTrait, FromQueryResult, QueryFilter, Statement,
    TransactionTrait,
};
use thiserror::Error;
use tokio::{task::JoinSet, time::sleep};
use tracing::instrument;

type Result<T> = std::result::Result<T, ScannerError>;

mod about_parser;
mod cache_update;
mod instance_check;
mod instance_parser;
mod profile_parser;
mod version_check;

const CAPTCHA_TEXT: &'static str = "Enable JavaScript and cookies to continue";
const CAPTCHA_CODE: u16 = 403;

static ACCEPT: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";
static LANGUAGE: &str = "de,en-US;q=0.7,en;q=0.3";
static HEADERS: [[&'static str; 2]; 7] = [
    ["Accept", ACCEPT],
    ["Accept-Language", LANGUAGE],
    ["Sec-Fetch-Dest", "document"],
    ["Sec-Fetch-Mode", "navigate"],
    ["Sec-Fetch-Site", "none"],
    ["Sec-Fetch-User", "?1"],
    ["TE", "trailers"],
];

#[derive(Error, Debug)]
pub enum ScannerError {
    #[error("Database Error {0:?}")]
    DB(#[from] sea_orm::DbErr),
    #[error("Failed parsing instance list {0}")]
    InstanceParse(#[from] instance_parser::InstanceListError),
    #[error("Failed to fetch URL: {0}")]
    FetchError(#[from] FetchError),
    #[error("Failed fetching git: {0}")]
    GitFetch(#[from] git2::Error),
    #[error("Couldn't find git branch")]
    GitBranch,
}

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("Http fetch error {0:?}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Url fetching failed, host responded with status {0} '{1}'. Response body: '{2}'")]
    HttpResponseStatus(u16, String, String),
    #[error("Url fetching failed, host responded with status {0} '{1}'")]
    KnownHttpResponseStatus(u16, String),
    #[error("Reading response body failed for Host {0}: {1}")]
    RetrievingBody(String, reqwest::Error),
    #[error("Host responded with captcha")]
    Captcha,
}

impl FetchError {
    /// Returns the http status code, if applicable
    fn http_status_code(&self) -> Option<u16> {
        match self {
            FetchError::Reqwest(e) => e.status().map(|v| v.as_u16()),
            FetchError::HttpResponseStatus(code, _, _) => Some(*code),
            FetchError::KnownHttpResponseStatus(code, _) => Some(*code),
            FetchError::Captcha | FetchError::RetrievingBody(_, _) => None,
        }
    }
}

pub fn run_scanner(
    db: DatabaseConnection,
    config: ScannerConfig,
    cache: Cache,
    disable_startup_scan: bool,
) -> Result<()> {
    let scanner = Scanner::new(db, config, cache);

    tokio::spawn(async move {
        scanner.run(disable_startup_scan).await.unwrap();
    });

    Ok(())
}

#[derive(Clone)]
struct Scanner {
    inner: Arc<InnerScanner>,
}

struct InnerScanner {
    db: DatabaseConnection,
    cache: Cache,
    config: ScannerConfig,
    client: reqwest::Client,
    instance_parser: InstanceParser,
    about_parser: AboutParser,
    profile_parser: ProfileParser,
    last_list_fetch: Mutex<Instant>,
    last_uptime_check: Mutex<Instant>,
    rss_check_regex: Regex,
}

#[derive(Debug, FromQueryResult, Default)]
pub struct LatestCheck {
    pub host: i32,
    pub healthy: bool,
    pub domain: String,
}

impl Scanner {
    fn new(db: DatabaseConnection, config: ScannerConfig, cache: Cache) -> Self {
        let mut headers = HeaderMap::with_capacity(HEADERS.len());
        for header in HEADERS {
            headers.insert(header[0], HeaderValue::from_static(header[1]));
        }
        let user_agent = format!("nitter-status (+{}/about)",config.website_url);
        let http_client = Client::builder()
            .cookie_store(true)
            .brotli(true)
            .deflate(true)
            .gzip(true)
            .user_agent(user_agent)
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(10))
            .default_headers(headers);
        let mut builder_regex_rss = RegexBuilder::new(&config.rss_content);
        builder_regex_rss.case_insensitive(true);
        Self {
            inner: Arc::new(InnerScanner {
                db,
                cache,
                config,
                client: http_client.build().unwrap(),
                instance_parser: InstanceParser::new(),
                about_parser: AboutParser::new(),
                profile_parser: ProfileParser::new(),
                last_list_fetch: Mutex::new(Instant::now()),
                last_uptime_check: Mutex::new(Instant::now()),
                rss_check_regex: builder_regex_rss
                    .build()
                    .expect("Invalid RSS Content regex!"),
            }),
        }
    }

    pub async fn run(mut self, disable_startup_scan: bool) -> Result<()> {
        let mut first_run = !disable_startup_scan;

        self.update_cache().await?;
        loop {
            if first_run || self.is_instance_list_outdated() {
                if let Err(e) = self.update_instacelist().await {
                    tracing::error!(error=?e,"Failed updating instance list");
                }
            }
            if first_run || self.is_instance_check_outdated() {
                if let Err(e) = self.check_uptime().await {
                    tracing::error!(error=?e,"Failed checking instance");
                }
            }
            if let Err(e) = self.update_cache().await {
                tracing::error!(error=?e,"Failed updating cache!");
            }
            self.sleep_till_deadline().await;
            first_run = false;
        }
    }

    async fn sleep_till_deadline(&self) {
        let delay_instance_check =
            self.last_uptime_check() + self.inner.config.instance_check_interval;

        let delay_list_update = self.last_list_fetch() + self.inner.config.list_fetch_interval;

        let delay = delay_instance_check.min(delay_list_update);
        tracing::trace!(duration=?delay-Instant::now(),"scanner sleeping");
        sleep(delay.duration_since(Instant::now())).await;
    }

    fn last_uptime_check(&self) -> Instant {
        *self.inner.last_uptime_check.lock().unwrap()
    }

    fn last_list_fetch(&self) -> Instant {
        *self.inner.last_list_fetch.lock().unwrap()
    }

    fn is_instance_check_outdated(&self) -> bool {
        let val = self.last_uptime_check();
        Instant::now().saturating_duration_since(val) >= self.inner.config.instance_check_interval
    }

    fn is_instance_list_outdated(&self) -> bool {
        let val = self.last_list_fetch();
        Instant::now().saturating_duration_since(val) >= self.inner.config.list_fetch_interval
    }

    #[instrument]
    async fn update_instacelist(&mut self) -> Result<()> {
        let start = Instant::now();
        let html: String = self.fetch_instancelist().await?;
        let parsed_instances = self.inner.instance_parser.parse_instancelist(
            &html,
            &self.inner.config.additional_hosts,
            &self.inner.config.additional_host_country,
            false,
        )?;

        let transaction = self.inner.db.begin().await?;

        // find all currently enabled instances
        let enabled_hosts = Host::find()
            .filter(host::Column::Enabled.eq(true))
            .all(&transaction)
            .await?;
        // make a diff and remove the ones not found while parsing
        let time: chrono::DateTime<Utc> = Utc::now();
        let mut removed = 0;
        for host in enabled_hosts.iter() {
            if !parsed_instances.contains_key(&host.domain) {
                host::ActiveModel {
                    id: ActiveValue::Set(host.id),
                    enabled: ActiveValue::Set(false),
                    updated: ActiveValue::Set(time.timestamp()),
                    ..Default::default()
                }
                .update(&transaction)
                .await?;
                removed += 1;
            }
        }
        // now update/insert the existing ones
        let found_instances: usize = parsed_instances.len();
        // find last update checks to detect spam
        let last_status = self.query_latest_check(&transaction).await?;
        let mut join_set = JoinSet::new();
        for (_, instance) in parsed_instances {
            // TODO: parallelize this!
            let scanner_c = self.clone();
            // detect already offline host and prevent log spam
            let muted_host = match self.inner.config.auto_mute {
                false => false,
                true => last_status
                    .iter()
                    .find(|v| v.domain == instance.domain)
                    .map_or(false, |check| !check.healthy),
            };
            // tracing::trace!(muted_host,instance=?instance,last_status=?last_status);
            join_set.spawn(async move {
                let (rss, version, version_url) = match Url::parse(&instance.url) {
                    Err(_) => {
                        if !muted_host {
                            tracing::info!(url = instance.url, "Instance URL invalid");
                        }
                        (false, None, None)
                    }
                    Ok(mut url) => {
                        let rss = scanner_c.has_rss(&mut url, muted_host).await;
                        match scanner_c.nitter_version(&mut url, muted_host).await {
                            Some(version) => (rss, Some(version.version_name), Some(version.url)),
                            None => (rss, None, None),
                        }
                    }
                };

                host::ActiveModel {
                    id: ActiveValue::NotSet,
                    domain: ActiveValue::Set(instance.domain),
                    country: ActiveValue::Set(instance.country),
                    url: ActiveValue::Set(instance.url),
                    enabled: ActiveValue::Set(true),
                    version: ActiveValue::Set(version),
                    version_url: ActiveValue::Set(version_url),
                    rss: ActiveValue::Set(rss),
                    updated: ActiveValue::Set(time.timestamp()),
                }
            });
        }
        while let Some(update_model) = join_set.join_next().await.map(|v| v.unwrap()) {
            Host::insert(update_model)
                .on_conflict(
                    OnConflict::column(host::Column::Domain)
                        .update_columns([
                            host::Column::Enabled,
                            host::Column::Updated,
                            host::Column::Url,
                            host::Column::Rss,
                            host::Column::Version,
                            host::Column::VersionUrl,
                            host::Column::Country,
                        ])
                        .to_owned(),
                )
                .exec(&transaction)
                .await?;
        }

        transaction.commit().await?;
        let end = Instant::now();
        let took_ms = end.saturating_duration_since(start).as_millis();
        {
            *self.inner.last_list_fetch.lock().unwrap() = end;
        }
        tracing::debug!(
            removed = removed,
            found = found_instances,
            took_ms = took_ms
        );
        Ok(())
    }

    async fn fetch_instancelist(&self) -> Result<String> {
        let (_, body) = self.fetch_url(&self.inner.config.instance_list_url).await?;
        Ok(body)
    }

    async fn fetch_url(&self, url: &str) -> std::result::Result<(u16, String), FetchError> {
        let fetch_res = self.inner.client.get(url).send().await?;
        let code = fetch_res.status().as_u16();
        if !fetch_res.status().is_success() {
            let message = fetch_res
                .status()
                .canonical_reason()
                .unwrap_or_default()
                .to_owned();
            let body_text = match fetch_res.text().await {
                Err(e) => format!("Additionally failed reading response body: {:?}", e),
                Ok(v) => v,
            };
            if code == CAPTCHA_CODE && body_text.contains(CAPTCHA_TEXT) {
                return Err(FetchError::Captcha);
            }
            if code == 403 && body_text.contains("You have been blocked") {
                // cloudflare block
                return Err(FetchError::KnownHttpResponseStatus(code, message));
            }
            if code == 429 && body_text.contains("Instance has been rate limited") {
                // out of non-limited accounts
                return Err(FetchError::KnownHttpResponseStatus(code, message));
            }
            if code == 404 {
                // don't spam the body on 404s
                return Err(FetchError::KnownHttpResponseStatus(code, message));
            }
            if code >= 502 && code <= 504 {
                // don't spam the body on Bad Gateway/Service Unavailable/Gateway Timeout
                return Err(FetchError::KnownHttpResponseStatus(code, message));
            }
            if code >= 520 && code <= 527 {
                // don't spam the body on Cloudflare errors
                // https://en.wikipedia.org/wiki/List_of_HTTP_status_codes
                return Err(FetchError::KnownHttpResponseStatus(code, message));
            }
            return Err(FetchError::HttpResponseStatus(code, message, body_text));
        }
        let body = fetch_res
            .text()
            .await
            .map_err(|e| FetchError::RetrievingBody(url.to_owned(), e))?;

        Ok((code, body))
    }

    pub async fn query_latest_check<T: ConnectionTrait>(
        &self,
        connection: &T,
    ) -> Result<Vec<LatestCheck>> {
        let health_checks = LatestCheck::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            WITH latest AS(
                SELECT u.host,MAX(u.time) as time FROM health_check u
                GROUP BY u.host
            )
            SELECT u.host,healthy,h.domain FROM health_check u
            JOIN host h ON h.id = u.host
            JOIN latest l ON l.host = u.host AND l.time = u.time
            WHERE h.enabled = true
            "#,
            [],
        ))
        .all(connection)
        .await?;
        Ok(health_checks)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use entities::state::scanner::Config;
    use sea_orm::{ConnectOptions, Database};
    use tokio::{fs::File, io::AsyncWriteExt};

    async fn db_init() -> DatabaseConnection {
        Database::connect(ConnectOptions::new(
            "sqlite:./test_db.db?mode=rwc".to_owned(),
        ))
        .await
        .unwrap()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn update_instacelist() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new());
        let res = scanner.fetch_instancelist().await.unwrap();
        let mut file = File::create("test_data/instancelist.html").await.unwrap();
        file.write_all(&res.as_bytes()).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn fetch_test() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new());
        let (_, res) = scanner.fetch_url("example.com/jack").await.unwrap();
        let mut file = File::create("test_data/blocked.html").await.unwrap();
        file.write_all(&res.as_bytes()).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn stats_test() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new());
        dbg!(scanner.generate_cache_data().await.unwrap());
    }
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner").finish()
    }
}
