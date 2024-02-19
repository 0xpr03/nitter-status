// SPDX-License-Identifier: AGPL-3.0-only
use std::sync::Arc;
use std::time::Instant;

use crate::Result;
use crate::ServerError;
use axum::response::IntoResponse;
use axum::{extract::State, response::Html};
use entities::state::scanner::ScannerConfig;
use entities::state::AppState;
use hyper::http::HeaderValue;

pub async fn instances(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref config): State<Arc<crate::Config>>,
) -> Result<axum::response::Response> {
    let mut context = tera::Context::new();
    let mut res = {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        context.insert("instances", &guard.hosts);
        let time = guard.last_update.format("%Y.%m.%d %H:%M").to_string();
        context.insert("last_updated", &time);
        let start = Instant::now();
        let res = Html(template.render("instances.html.j2", &context)?).into_response();
        let end = Instant::now();
        drop(guard);
        let templating_time = end - start;
        tracing::trace!(templating_time = templating_time.as_millis());
        res
    };
    res.headers_mut().insert(
        "cache-control",
        HeaderValue::from_str(&format!("public, max-age={}", config.max_age)).unwrap(),
    );
    Ok(res)
}

pub async fn about(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref scanner_config): State<ScannerConfig>,
) -> Result<axum::response::Response> {
    let mut context = tera::Context::new();
    let mut paths = Vec::with_capacity(5);
    paths.push(&scanner_config.about_path);
    paths.push(&scanner_config.rss_path);
    paths.push(&scanner_config.profile_path);
    paths.push(&scanner_config.connectivity_path);
    context.insert("checked_paths", &paths);
    context.insert(
        "uptime_interval_s",
        &scanner_config.instance_check_interval.as_secs(),
    );
    context.insert(
        "wiki_interval_s",
        &scanner_config.list_fetch_interval.as_secs(),
    );
    context.insert(
        "ping_avg_interval_h",
        &scanner_config.ping_range.num_hours(),
    );
    {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        context.insert("latest_commit", &guard.latest_commit);
    }

    let mut res = Html(template.render("about.html.j2", &context)?).into_response();
    res.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("public, max-age=900"),
    );
    Ok(res)
}

pub async fn rip(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref scanner_config): State<ScannerConfig>,
) -> Result<axum::response::Response> {
    let mut context = tera::Context::new();
    {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        let time = guard.last_update.format("%Y.%m.%d %H:%M").to_string();
        context.insert("last_updated", &time);
    }

    let mut res = Html(template.render("rip.html.j2", &context)?).into_response();
    res.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("public, max-age=900"),
    );
    Ok(res)
}
