// SPDX-License-Identifier: AGPL-3.0-only
use std::{
    net::IpAddr,
    sync::{Arc, Mutex},
};

use about_parser::AboutParser;
use chrono::{DateTime, Duration, TimeZone, Utc};
use entities::state::{error_cache::HostError, scanner::ScannerConfig, AppState};
use instance_parser::InstanceParser;
use miette::{Context, IntoDiagnostic};
use profile_parser::ProfileParser;
use regex::{Regex, RegexBuilder};
use reqwest::{
    Client, ClientBuilder, header::{HeaderMap, HeaderValue}, retry
};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, FromQueryResult, Statement};
use thiserror::Error;
use tokio::time::sleep;

use crate::version_check::VersionCheck;

type Result<T> = std::result::Result<T, ScannerError>;

mod about_parser;
mod cache_update;
mod cleanup;
mod instance_check;
mod instance_parser;
mod list_update;
mod profile_parser;
mod update_stats;
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
    /// Configured git branch can't be found
    #[error("Couldn't find configured git branch")]
    GitBranchNotFound,
    /// Failed to parse .health endpoint
    #[error("Failed to parse .health endpoint. {0} Body: {1}")]
    StatsParsing(serde_json::Error, String),
    /// Generic error for parsing an instance URL to [reqwest::Url]
    #[error("Can't parse instance URL")]
    InstanceUrlParse,
    /// Failed to join a tokio thread in a JoinSet
    #[error("Failed to join worker task {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("Http fetch error {0:?}")]
    Reqwest(#[from] reqwest::Error),
    /// http code, code name, body
    #[error("Url fetching failed, host responded with status {0} '{1}'. Response body: '{2}'")]
    HttpResponseStatus(u16, String, String),
    /// http code, code name, body
    #[error("Url fetching failed, host responded with status {0} '{1}'")]
    KnownHttpResponseStatus(u16, String, String),
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
            FetchError::KnownHttpResponseStatus(code, _, _body) => Some(*code),
            FetchError::Captcha | FetchError::RetrievingBody(_, _) => None,
        }
    }

    fn to_host_error(self) -> HostError {
        match self {
            FetchError::Reqwest(e) => HostError::new_message(e.to_string()),
            FetchError::HttpResponseStatus(http_status, _code_msg, http_body) => {
                HostError::new(format!("failed to fetch"), http_body, http_status)
            }
            FetchError::KnownHttpResponseStatus(http_status, code_msg, body) => {
                let msg = format!("Known bad response on status {code_msg}");
                HostError::new(msg, body, http_status)
            }
            FetchError::RetrievingBody(_url, reqwest_error) => {
                HostError::new_message(reqwest_error.to_string())
            }
            FetchError::Captcha => HostError::new_message(format!("Captcha detected")),
        }
    }
}

#[derive(Debug, FromQueryResult, Default)]
pub(crate) struct LatestCheck {
    pub host: i32,
    pub healthy: bool,
    pub domain: String,
}

pub async fn run_scanner(
    db: DatabaseConnection,
    config: ScannerConfig,
    app_state: AppState,
    disable_health_checks: bool,
) -> miette::Result<()> {
    let scanner = Scanner::new(db, config, app_state)
        .await
        .wrap_err("Initializing scanner!")?;
    scanner.schedule_cleanup().unwrap();

    if disable_health_checks {
        tracing::error!("Health checks disabled!");
        return Ok(());
    }
    tokio::spawn(async move {
        scanner.run().await.expect("Failed to run scanner daemon!");
    });

    Ok(())
}

#[derive(Clone)]
struct Scanner {
    inner: Arc<InnerScanner>,
}

struct InnerScanner {
    db: DatabaseConnection,
    app_state: AppState,
    config: ScannerConfig,
    client: reqwest::Client,
    instance_parser: InstanceParser,
    about_parser: AboutParser,
    profile_parser: ProfileParser,
    last_list_fetch: Mutex<DateTime<Utc>>,
    last_stats_fetch: Mutex<DateTime<Utc>>,
    last_uptime_check: Mutex<DateTime<Utc>>,
    rss_check_regex: Regex,
    client_ipv4: Client,
    client_ipv6: Client,
    version_checker: Mutex<VersionCheck>,
}

impl Scanner {
    pub(crate) fn client_builder(config: &ScannerConfig) -> ClientBuilder {
        let mut headers = HeaderMap::with_capacity(HEADERS.len());
        for header in HEADERS {
            headers.insert(header[0], HeaderValue::from_static(header[1]));
        }
        let user_agent = format!("nitter-status (+{}/about)", config.website_url);
        let http_client = Client::builder()
            .cookie_store(true)
            .brotli(true)
            .deflate(true)
            .tls_backend_native()
            .gzip(true)
            .user_agent(user_agent)
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .default_headers(headers);
        http_client
    }

    async fn new(
        db: DatabaseConnection,
        config: ScannerConfig,
        app_state: AppState,
    ) -> miette::Result<Self> {
        let mut builder_regex_rss = RegexBuilder::new(&config.rss_content);
        builder_regex_rss.case_insensitive(true);

        let version_checker = Mutex::new(VersionCheck::new(config.clone()).into_diagnostic()?);

        let http_client = Self::client_builder(&config);
        let client_ipv4 = Scanner::client_builder(&config)
            .local_address("0.0.0.0".parse::<IpAddr>().unwrap())
            .build()
            .into_diagnostic()?;
        let client_ipv6 = Scanner::client_builder(&config)
            .local_address("::".parse::<IpAddr>().unwrap())
            .build()
            .into_diagnostic()?;

        let last_uptime_check = Self::query_last_entry_for_table(&db, "health_check")
            .await
            .into_diagnostic()
            .wrap_err("Fetching last uptime check failed!")?;

        let last_stats_check = Self::query_last_entry_for_table(&db, "instance_stats")
            .await
            .into_diagnostic()
            .wrap_err("Fetching last stats check failed!")?;
        tracing::info!(?last_uptime_check, ?last_stats_check); // TODO: wrong timestamp
        let scanner = Self {
            inner: Arc::new(InnerScanner {
                db,
                app_state,
                version_checker,
                client: http_client.build().unwrap(),
                config,
                client_ipv4,
                client_ipv6,
                instance_parser: InstanceParser::new(),
                about_parser: AboutParser::new(),
                profile_parser: ProfileParser::new(),
                last_list_fetch: Mutex::new(last_uptime_check),
                last_uptime_check: Mutex::new(last_uptime_check),
                last_stats_fetch: Mutex::new(last_stats_check),
                rss_check_regex: builder_regex_rss
                    .build()
                    .into_diagnostic()
                    .wrap_err("Invalid RSS Content regex!")?,
            }),
        };
        scanner
            .update_cache()
            .await
            .into_diagnostic()
            .wrap_err("Initial cache update failed!")?;
        Ok(scanner)
    }

    /// Retrieves the last stats fetching that happened
    pub async fn query_last_entry_for_table(
        db: &DatabaseConnection,
        table: &str,
    ) -> Result<DateTime<Utc>> {
        #[derive(Debug, FromQueryResult, Default)]
        pub(crate) struct TimeMax {
            pub max: Option<i64>,
        }
        // can't use find().column_as(health_check::Column::Time.max(), health_check::Column::Time)
        // will crash on an empty DB
        let time_max = TimeMax::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            format!("SELECT MAX(time) as max FROM {table}"),
            [],
        ))
        .one(db)
        .await?
        .map(|model| model.max)
        .flatten()
        .unwrap_or_default();
        Ok(Utc.timestamp_opt(time_max, 0).unwrap())
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

    pub async fn run(mut self) -> Result<()> {
        loop {
            if self.is_instance_list_outdated() {
                if let Err(e) = self.update_instacelist().await {
                    tracing::error!(error=?e,"Failed updating instance list");
                }
            }
            if self.is_instance_check_outdated() {
                if let Err(e) = self.check_uptime().await {
                    tracing::error!(error=?e,"Failed checking instances for health");
                }
                // schedule instance stats always post health checks
                // and wait enough time
                if self.is_instance_stats_outdated() {
                    sleep(std::time::Duration::from_secs(1)).await;
                    tracing::info!("Checking instance stats");
                    if let Err(e) = self.check_health().await {
                        tracing::error!(error=?e,"Failed checking instances for stats");
                    }
                }
            }
            if let Err(e) = self.update_cache().await {
                tracing::error!(error=?e,"Failed to update cache!");
            }
            self.sleep_till_deadline().await;
        }
    }

    async fn sleep_till_deadline(&self) {
        let delay_instance_check =
            self.last_uptime_check() + self.inner.config.instance_check_interval;
        let delay_list_update = self.last_list_fetch() + self.inner.config.list_fetch_interval;
        let delay_stats_update =
            self.last_stats_fetch() + self.inner.config.instance_stats_interval;
        tracing::debug!(
            ?delay_list_update,
            ?delay_instance_check,
            ?delay_stats_update
        );

        let next_deadline = delay_instance_check
            .min(delay_list_update)
            .min(delay_stats_update);
        let now = Utc::now();
        let sleep_time = next_deadline.signed_duration_since(now);
        if sleep_time <= Duration::zero() {
            // schedule right now, also std duration can't be negative
            return;
        }
        let sleep_time = sleep_time.to_std().unwrap();
        tracing::trace!(duration=?sleep_time,"scanner sleeping");
        sleep(sleep_time).await;
    }

    fn last_uptime_check(&self) -> DateTime<Utc> {
        *self.inner.last_uptime_check.lock().unwrap()
    }

    fn last_list_fetch(&self) -> DateTime<Utc> {
        *self.inner.last_list_fetch.lock().unwrap()
    }

    fn last_stats_fetch(&self) -> DateTime<Utc> {
        *self.inner.last_stats_fetch.lock().unwrap()
    }

    fn is_instance_check_outdated(&self) -> bool {
        let val = self.last_uptime_check();
        Utc::now().signed_duration_since(val).to_std().unwrap()
            >= self.inner.config.instance_check_interval
    }

    fn is_instance_list_outdated(&self) -> bool {
        let val = self.last_list_fetch();
        Utc::now().signed_duration_since(val).to_std().unwrap()
            >= self.inner.config.list_fetch_interval
    }

    fn is_instance_stats_outdated(&self) -> bool {
        let val = self.last_stats_fetch();
        Utc::now().signed_duration_since(val).to_std().unwrap()
            >= self.inner.config.instance_stats_interval
    }

    async fn fetch_instance_list(&self) -> Result<String> {
        let (_, body) = self
            .fetch_url(&self.inner.config.instance_list_url, None)
            .await?;
        Ok(body)
    }

    async fn fetch_url(
        &self,
        url: &str,
        api_token: Option<&str>,
    ) -> std::result::Result<(u16, String), FetchError> {
        let mut request = self.inner.client.get(url);
        if let Some(token) = api_token {
            request = request.bearer_auth(token);
        }
        let fetch_res = request.send().await?;
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
                return Err(FetchError::KnownHttpResponseStatus(
                    code, message, body_text,
                ));
            }
            if code == 429 && body_text.contains("Instance has been rate limited") {
                // out of non-limited accounts
                return Err(FetchError::KnownHttpResponseStatus(
                    code, message, body_text,
                ));
            }
            if code == 404 {
                // don't spam the body on 404s
                return Err(FetchError::KnownHttpResponseStatus(
                    code, message, body_text,
                ));
            }
            if code >= 502 && code <= 504 {
                // don't spam the body on Bad Gateway/Service Unavailable/Gateway Timeout
                return Err(FetchError::KnownHttpResponseStatus(
                    code, message, body_text,
                ));
            }
            if code >= 520 && code <= 527 {
                // don't spam the body on Cloudflare errors
                // https://en.wikipedia.org/wiki/List_of_HTTP_status_codes
                return Err(FetchError::KnownHttpResponseStatus(
                    code, message, body_text,
                ));
            }
            return Err(FetchError::HttpResponseStatus(code, message, body_text));
        }
        let body = fetch_res
            .text()
            .await
            .map_err(|e| FetchError::RetrievingBody(url.to_owned(), e))?;

        Ok((code, body))
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use chrono::Duration;
    use entities::health_check;
    use entities::state::scanner::Config;
    use migration::MigratorTrait;
    use sea_orm::{ActiveModelTrait, ActiveValue, ConnectOptions, Database};
    use tokio::{fs::File, io::AsyncWriteExt};

    pub(crate) async fn db_init() -> DatabaseConnection {
        let db = Database::connect(ConnectOptions::new(
            "sqlite:./test_db.db?mode=rwc".to_owned(),
        ))
        .await
        .unwrap();
        migration::Migrator::up(&db, None).await.unwrap();
        db
    }

    // only for generating fake data
    // still requires copying over the DB for running on it
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn setup_test_data() {
        let db = db_init().await;
        let time: chrono::DateTime<Utc> = Utc::now();
        for v in 0..365 {
            let entry_time = time.checked_sub_signed(Duration::minutes(15 * v)).unwrap();
            health_check::ActiveModel {
                time: ActiveValue::Set(entry_time.timestamp()),
                host: ActiveValue::Set(208692),
                resp_time: ActiveValue::Set(Some(12)),
                healthy: ActiveValue::Set(v % 2 == 0),
                response_code: ActiveValue::Set(Some(200)),
            }
            .insert(&db)
            .await
            .unwrap();
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn test_fetch_instance_list() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new())
            .await
            .unwrap();
        let res = scanner.fetch_instance_list().await.unwrap();
        let mut file = File::create("test_data/instancelist.html").await.unwrap();
        file.write_all(&res.as_bytes()).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn fetch_test() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new())
            .await
            .unwrap();
        let (_, res) = scanner.fetch_url("example.com/jack", None).await.unwrap();
        let mut file = File::create("test_data/blocked.html").await.unwrap();
        file.write_all(&res.as_bytes()).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore]
    async fn stats_test() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new())
            .await
            .unwrap();
        dbg!(scanner.generate_cache_data().await.unwrap());
    }
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner").finish()
    }
}
