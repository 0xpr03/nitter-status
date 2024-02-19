// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Result, ServerError};
use axum::response::IntoResponse;
use axum::{extract::State, Json};
use chrono::{TimeZone, Utc};
use entities::health_check;
use entities::state::AppState;
use hyper::http::HeaderValue;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use std::fmt::Write;
use std::sync::Arc;

pub async fn instances(
    State(ref app_state): State<AppState>,
    State(ref config): State<Arc<crate::Config>>,
) -> Result<axum::response::Response> {
    let mut res = {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        let res = Json(&*guard).into_response();
        drop(guard);
        res
    };
    res.headers_mut().insert(
        "cache-control",
        HeaderValue::from_str(&format!("public, max-age={}", config.max_age)).unwrap(),
    );
    res.headers_mut().insert(
        "X-Robots-Tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    Ok(res)
}

pub async fn graph(
    State(ref db): State<DatabaseConnection>,
    State(ref config): State<Arc<crate::Config>>,
) -> Result<axum::response::Response> {
    let start = std::time::Instant::now();
    let healthy_data = health_check::InstanceGraphEntry::fetch(db, None).await?;
    let queried = std::time::Instant::now();
    let mut data = String::with_capacity(8*healthy_data.len());

    data.push_str("Date,Healthy,Dead,Tokens AVG,Requests AVG,Limited Tokens AVG\n");

    for entry in healthy_data {
        let time = Utc.timestamp_opt(entry.time,0).unwrap().format("%Y-%m-%dT%H:%M:%SZ");
        if let (Some(total_accs_avg),Some(total_requests_avg),Some(limited_accs_avg)) = (entry.total_accs_avg,entry.total_requests_avg,entry.limited_accs_avg){
            writeln!(&mut data, "{time},{},{},{},{},{}",entry.alive,entry.dead,total_accs_avg,total_requests_avg,limited_accs_avg)
        } else {
            writeln!(&mut data, "{time},{},{},,,",entry.alive,entry.dead)
        }
        .map_err(|e|ServerError::CSV(e.to_string()))?;
    }
    let formatted = std::time::Instant::now();
    let query_time = queried - start;
    let format_time = formatted - queried;
    tracing::debug!(?query_time,?format_time);

    let mut res = data.into_response();
    res.headers_mut().insert(
        "content-type",
        HeaderValue::from_str("text/csv").unwrap(),
    );
    res.headers_mut().insert(
        "cache-control",
        HeaderValue::from_str(&format!("public, max-age={}", config.max_age)).unwrap(),
    );
    res.headers_mut().insert(
        "X-Robots-Tag",
        HeaderValue::from_static("noindex, nofollow"),
    );
    Ok(res)
}