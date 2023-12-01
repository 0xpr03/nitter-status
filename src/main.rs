// SPDX-License-Identifier: AGPL-3.0-only
use std::{env::var, time::Duration};

use entities::state::scanner::ScannerConfig;
use miette::{Context, IntoDiagnostic};
use migration::MigratorTrait;
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection};
use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> miette::Result<()> {
    #[cfg(debug_assertions)]
    let build_mode = "debug mode";
    #[cfg(not(debug_assertions))]
    let build_mode = "release mode";
    println!(
        "Starting {} {} licensed under {}, {build_mode}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_LICENSE")
    );
    dotenvy::dotenv()
        .into_diagnostic()
        .wrap_err_with(|| "Failed to load .env file!")?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name(concat!(env!("CARGO_PKG_NAME"), "-wrk"))
        .build()
        .into_diagnostic()
        .wrap_err_with(|| "Failed to initialize async runtime!")?;

    rt.block_on(_main())
}

async fn _main() -> miette::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            var("RUST_LOG").unwrap_or_else(|_| {
                #[cfg(debug_assertions)]
                return format!(
                    "warn,tower_http=debug,migration=debug,scanner=trace,server=trace,{}=debug",
                    env!("CARGO_PKG_NAME")
                )
                .into();
                #[cfg(not(debug_assertions))]
                return format!(
                    "warn,migration=debug,scanner=info,server=info,{}=debug",
                    env!("CARGO_PKG_NAME")
                )
                .into();
            }),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::debug!("connecting to database");
    let dburl = require_env_str("DATABASE_URL")?;
    let mut db_opts = ConnectOptions::new(dburl);
    db_opts.connect_timeout(Duration::from_secs(2));
    let pool = Database::connect(db_opts)
        .await
        .into_diagnostic()
        .wrap_err("Failed connecting to database")?;

    let port: u16 = require_env_str("PORT")?
        .parse()
        .expect("PORT must be a number");

    let scanner_config = read_scanner_cfg()?;

    let server_config = read_server_config(scanner_config.instance_check_interval.as_secs() as _)?;

    test_init(&pool).await?;

    tracing::info!("migrating db");
    migration::Migrator::up(&pool, None)
        .await
        .into_diagnostic()
        .wrap_err_with(|| "Failed to perform database migration!")?;

    let cache = entities::state::new();

    let disable_health_checks = require_env_str("DISABLE_HEALTH_CHECKS")? == "true";

    scanner::run_scanner(
        pool.clone(),
        scanner_config.clone(),
        cache.clone(),
        disable_health_checks,
    )
    .await
    .wrap_err("Crash starting background scanner")?;

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    server::start(&addr, pool, server_config, scanner_config, cache)
        .await
        .into_diagnostic()?;

    tracing::info!("shutting down");

    Ok(())
}

fn read_scanner_cfg() -> miette::Result<ScannerConfig> {
    let nitter_instancelist: String = require_env_str("NITTER_INSTANCELIST")?;
    let instance_ping_interval: u64 = require_env_str("INSTANCE_PING_INTERVAL_S")?
        .parse()
        .expect("INSTANCE_PING_INTERVAL_S must be a number");
    let instance_list_interval: u64 = require_env_str("INSTANCE_LIST_INTERVAL_S")?
        .parse()
        .expect("INSTANCE_LIST_INTERVAL_S must be a number");
    let ping_range: u32 = require_env_str("PING_RANGE_H")?
        .parse()
        .expect("PING_RANGE_H must be a number");

    let profile_path = require_env_str("PROFILE_PATH")?;
    let rss_path = require_env_str("RSS_PATH")?;
    let about_path = require_env_str("ABOUT_PATH")?;
    let profile_name = require_env_str("PROFILE_NAME")?;
    let profile_posts_min = require_env_str("PROFILE_POSTS_MIN")?
        .parse()
        .expect("PROFILE_POSTS_MIN must be a positive number");
    let additional_hosts: Vec<String> = require_env_vec_str("ADDITIONAL_HOSTS")?;
    let additional_host_country = require_env_str("ADDITIONAL_HOSTS_COUNTRY")?;
    let rss_content = require_env_str("RSS_CONTENT")?;
    let bad_hosts: Vec<String> = require_env_vec_str("BAD_HOSTS")?;
    let auto_mute = require_env_str("AUTO_MUTE")? == "true";
    let source_git_branch = require_env_str("ORIGIN_SOURCE_GIT_BRANCH")?;
    let source_git_url = require_env_str("ORIGIN_SOURCE_GIT_URL")?;
    let cleanup_interval: u64 = require_env_str("CLEANUP_INTERVAL_S")?
        .parse()
        .expect("CLEANUP_INTERVAL_S must be a number");
    let error_retention_per_host: usize = require_env_str("ERROR_RETENTION_PER_HOST")?
        .parse()
        .expect("CLEANUP_INTERVAL_S must be a number");
    let instance_stats_interval: u64 = require_env_str("STATS_INTERVAL_S")?
        .parse()
        .expect("STATS_INTERVAL_S must be a positive number");

    Ok(Arc::new(entities::state::scanner::Config {
        instance_stats_interval: Duration::from_secs(instance_stats_interval),
        list_fetch_interval: Duration::from_secs(instance_list_interval),
        instance_check_interval: Duration::from_secs(instance_ping_interval),
        instance_list_url: nitter_instancelist,
        profile_path,
        rss_path,
        about_path,
        profile_name,
        profile_posts_min,
        rss_content,
        additional_hosts,
        additional_host_country,
        website_url: require_env_str("SITE_URL")?,
        ping_range: chrono::Duration::hours(ping_range as _),
        auto_mute,
        source_git_branch,
        source_git_url,
        bad_hosts,
        cleanup_interval: Duration::from_secs(cleanup_interval),
        error_retention_per_host,
        connectivity_path: String::from("/"),
    }))
}

async fn test_init(db: &DatabaseConnection) -> miette::Result<()> {
    let res = db
        .query_one(sea_orm::Statement::from_string(
            DatabaseBackend::Postgres,
            "SELECT sqlite_version() as version;".to_owned(),
        ))
        .await
        .into_diagnostic()?;
    let v = res.unwrap();
    let db_version: String = v.try_get("", "version").unwrap();
    tracing::debug!(db_version);

    Ok(())
}

fn read_server_config(instance_ping_interval: usize) -> miette::Result<server::Config> {
    let site_url = require_env_str("SITE_URL")?;
    let session_ttl_seconds = require_env_str("SESSION_TTL_SECONDS")?
        .parse()
        .expect("SESSION_TTL_SECONDS must be a positive number");
    let login_token_name = require_env_str("LOGIN_TOKEN_NAME")?;
    let admin_domains = require_env_str("ADMIN_DOMAINS")?
        .split(",")
        .map(|v| v.trim().to_string())
        .collect();
    let session_db_uri = require_env_str("SESSION_DB_URI")?;
    let mail_from = require_env_str("MAIL_FROM")?;
    let mail_smtp_host = require_env_str("MAIL_SMTP_HOST")?;
    let mail_smtp_user = require_env_str("MAIL_SMTP_USER")?;
    let mail_smtp_password = require_env_str("MAIL_SMTP_PASSWORD")?;
    let mail_token_ttl_s = require_env_str("MAIL_VALIDATION_TOKEN_TTL_S")?
        .parse()
        .expect("MAIL_VALIDATION_TOKEN_TTL_S must be a positive number");

    Ok(server::Config {
        site_url,
        max_age: instance_ping_interval,
        session_ttl_seconds,
        login_token_name,
        admin_domains,
        session_db_uri,
        mail_from,
        mail_smtp_host,
        mail_smtp_user,
        mail_smtp_password,
        mail_token_ttl_s,
    })
}

fn require_env_vec_str(name: &str) -> miette::Result<Vec<String>> {
    Ok(require_env_str(name)?
        .trim()
        .split(",")
        .map(|v| v.trim().to_owned())
        .collect())
}

fn require_env_str(name: &str) -> miette::Result<String> {
    var(name).map_err(|v| miette::miette!("missing `{}` in environment: {:?}", name, v))
}
