use std::{collections::HashMap, fmt::Write, sync::Arc};

use axum::{
    extract::{Path, State},
    http::HeaderValue,
    response::{Html, IntoResponse, Redirect},
    Form,
};
use chrono::{TimeZone, Utc};
use entities::{
    health_check::{self, ResponseTimeStat},
    host_overrides::{
        self,
        keys::{HostOverrides, LOCKED_FALSE, LOCKED_TRUE},
    },
    instance_stats,
};
use sea_orm::{sea_query::OnConflict, ActiveValue, DatabaseConnection, EntityTrait};
use tower_sessions::Session;

use crate::{Result, ServerError};

use super::get_specific_login_host;

pub async fn stats_view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let (host, _login) = get_specific_login_host(instance, &session, db).await?;

    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    context.insert("HOST_ID", &instance);

    let res = Html(template.render("instance_stats.html.j2", &context)?).into_response();
    Ok(res)
}

/// Returns the statistics in the CSV format required by dygraph
pub async fn health_csv_api(
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let (host, _login) = get_specific_login_host(instance, &session, db).await?;
    // Nächstes mal:
    // Fetch request times and show an additional(?) red dot for failed requests"
    // Siehe globale? API mit response times
    let start = std::time::Instant::now();
    let healthy_data = ResponseTimeStat::fetch(db, None, None, host.id).await?;
    let queried = std::time::Instant::now();
    let mut data = String::with_capacity((4 + 3 + 3 + 3 + 3 + 3 + 4) * healthy_data.len());

    data.push_str("Date,Healthy Rsp Time,Dead Rsp Time\n");

    for entry in healthy_data {
        let time = Utc
            .timestamp_opt(entry.time, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ");
        let (rsp_healthy, rsp_dead) = match (entry.resp_time, entry.healthy) {
            (None, true) => (-1, 0),  //  no respons etime but healthy/unhealthy
            (None, false) => (0, -1), // we don't really expect thse two cases and return -1
            (Some(time), true) => (time, 0),
            (Some(time), false) => (0, time),
        };
        writeln!(&mut data, "{time},{},{}", rsp_healthy, rsp_dead)
            .map_err(|e| ServerError::CSV(e.to_string()))?;
    }
    let formatted = std::time::Instant::now();
    let query_time = queried - start;
    let format_time = formatted - queried;
    tracing::debug!(?query_time, ?format_time);

    let mut res = data.into_response();
    res.headers_mut()
        .insert("content-type", HeaderValue::from_str("text/csv").unwrap());
    Ok(res)
}

/// Returns the statistics in the CSV format required by dygraph
pub async fn stats_csv_api(
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let (host, _login) = get_specific_login_host(instance, &session, db).await?;

    let start = std::time::Instant::now();
    let healthy_data = instance_stats::StatsCSVEntry::fetch(db, Some(&[host.id])).await?;
    let queried = std::time::Instant::now();
    let mut data = String::with_capacity(8 * healthy_data.len());

    data.push_str("Date,Tokens AVG,Limited Tokens AVG,Requests AVG\n");

    for entry in healthy_data {
        let time = Utc
            .timestamp_opt(entry.time, 0)
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ");
        writeln!(
            &mut data,
            "{time},{},{},{}",
            entry.total_accs_avg, entry.limited_accs_avg, entry.total_requests_avg
        )
        .map_err(|e| ServerError::CSV(e.to_string()))?;
    }
    let formatted = std::time::Instant::now();
    let query_time = queried - start;
    let format_time = formatted - queried;
    tracing::debug!(?query_time, ?format_time);

    let mut res = data.into_response();
    res.headers_mut()
        .insert("content-type", HeaderValue::from_str("text/csv").unwrap());
    Ok(res)
}
