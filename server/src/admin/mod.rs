use std::fmt::format;
// SPDX-License-Identifier: AGPL-3.0-only
use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use axum::Json;
use chrono::DateTime;
use chrono::Utc;
use entities::check_errors;
use entities::health_check;
use entities::host;
use entities::instance_stats;
use entities::state::AppState;
use hyper::StatusCode;
use sea_orm::ColumnTrait;
use sea_orm::ConnectionTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;
use serde::Deserialize;
use serde::Serialize;
use tower_sessions::Session;

use crate::Result;
use crate::ServerError;

use self::session::ActiveLogin;
use self::session::LOGIN_KEY;

pub mod alerts;
pub mod mail;
pub mod session;

pub async fn overview(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    session: Session,
) -> Result<axum::response::Response> {
    tracing::info!(?session);

    let (login, hosts) = get_all_login_hosts(&session, db, true).await?;

    let mut context = tera::Context::new();
    let res = {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        let time = guard.last_update.format("%Y.%m.%d %H:%M").to_string();
        context.insert("last_updated", &time);
        context.insert("instances", &hosts);
        context.insert("is_admin", &login.admin);

        let res = Html(template.render("admin.html.j2", &context)?).into_response();
        drop(guard);
        res
    };
    Ok(res)
}

/// Json passed to select a date range
#[derive(Deserialize, Debug)]
pub struct DateRangeInput {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}
pub async fn history_json(
    State(ref db): State<DatabaseConnection>,
    session: Session,
    Json(input): Json<DateRangeInput>,
) -> Result<axum::response::Response> {
    #[derive(Debug, Serialize)]
    struct ReturnData {
        pub global: Vec<health_check::HealthyAmount>,
        pub user: Vec<health_check::HealthyAmount>,
        pub stats: Vec<instance_stats::StatsAmount>,
    }
    let (_login, hosts) = get_all_login_hosts(&session, db, false).await?;
    let host_ids: Vec<_> = hosts.iter().map(|host| host.id).collect();
    let data_owned =
        health_check::HealthyAmount::fetch(db, input.start, input.end, Some(&host_ids)).await?;
    let data_global = health_check::HealthyAmount::fetch(db, input.start, input.end, None).await?;
    let data_stats = instance_stats::StatsAmount::fetch(db, input.start, input.end, None).await?;

    Ok(Json(ReturnData {
        global: data_global,
        user: data_owned,
        stats: data_stats,
    })
    .into_response())
}

pub async fn history_json_specific(
    State(ref db): State<DatabaseConnection>,
    session: Session,
    Path(host): Path<i32>,
    Json(input): Json<DateRangeInput>,
) -> Result<axum::response::Response> {
    let host = get_specific_login_host(host, &session, db).await?;
    #[derive(Debug, Serialize)]
    struct ReturnData {
        pub stats: Vec<instance_stats::Model>,
        pub health: Vec<health_check::Model>,
    }

    let history_health: Vec<health_check::Model> = health_check::Entity::find()
        .filter(health_check::Column::Host.eq(host.id))
        .order_by_asc(health_check::Column::Time)
        .filter(health_check::Column::Time.between(input.start.timestamp(), input.end.timestamp()))
        .all(db)
        .await?;
    let history_stats: Vec<instance_stats::Model> = instance_stats::Entity::find()
        .filter(instance_stats::Column::Host.eq(host.id))
        .order_by_asc(instance_stats::Column::Time)
        .filter(
            instance_stats::Column::Time.between(input.start.timestamp(), input.end.timestamp()),
        )
        .all(db)
        .await?;

    Ok(Json(ReturnData {
        health: history_health,
        stats: history_stats,
    })
    .into_response())
}

pub async fn history_view(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(host): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    tracing::info!(?session);

    let host = get_specific_login_host(host, &session, db).await?;

    let mut context = tera::Context::new();
    let res = {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        let time = guard.last_update.format("%Y.%m.%d %H:%M").to_string();
        context.insert("last_updated", &time);
        context.insert("HOST_DOMAIN", &host.domain);

        let res = Html(template.render("errors_admin.html.j2", &context)?).into_response();
        drop(guard);
        res
    };
    Ok(res)
}

pub async fn instance_view(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    tracing::info!(?session);

    let host = get_specific_login_host(instance, &session, db).await?;

    let errors = check_errors::Entity::find()
        .filter(check_errors::Column::Host.eq(host.id))
        .order_by_desc(check_errors::Column::Time)
        .limit(20)
        .all(db)
        .await?;

    let mut context = tera::Context::new();
    let res = {
        let guard = app_state
            .cache
            .read()
            .map_err(|_| ServerError::MutexFailure)?;
        let time = guard.last_update.format("%Y.%m.%d %H:%M").to_string();
        context.insert("last_updated", &time);
        context.insert("ERRORS", &errors);
        context.insert("HOST_DOMAIN", &host.domain);
        context.insert("HOST_ID", &instance);

        let res = Html(template.render("instance_admin.html.j2", &context)?).into_response();
        drop(guard);
        res
    };
    Ok(res)
}

/// Get all [host::Model] for current [Session], optionally return all hosts for admins
async fn get_all_login_hosts(
    session: &Session,
    db: &DatabaseConnection,
    return_admin_hosts: bool,
) -> Result<(ActiveLogin, Vec<host::Model>)> {
    let login = get_session_login(&session)?;

    let host_res = match login.admin && return_admin_hosts {
        true => {
            host::Entity::find()
                .filter(host::Column::Enabled.eq(true))
                .all(db)
                .await?
        }
        false => {
            host::Entity::find()
                .filter(host::Column::Id.is_in(login.hosts.iter().map(|v| *v)))
                .all(db)
                .await?
        }
    };
    Ok((login, host_res))
}

/// Get wanted [host::Model] for current [Session] if valid for this user
async fn get_specific_login_host<T>(
    wanted_host_id: i32,
    session: &Session,
    db: &T,
) -> Result<host::Model>
where T: ConnectionTrait {
    let login = get_session_login(&session)?;

    if !login.hosts.contains(&wanted_host_id) && !login.admin {
        return Err(ServerError::MissingPermission);
    }

    let host_res = host::Entity::find()
        .filter(host::Column::Id.eq(wanted_host_id))
        .one(db)
        .await?;
    match host_res {
        None => {
            session.delete();
            return Err(ServerError::HostNotFound(wanted_host_id));
        }
        Some(host) => Ok(host),
    }
}

/// Check for valid session and return the stored [Login] data
fn get_session_login(session: &Session) -> Result<ActiveLogin> {
    if session.active() {
        if let Some(u) = session.get(LOGIN_KEY).map_err(|_| ServerError::NoLogin)? {
            return Ok(u);
        }
    }
    Err(ServerError::NoLogin)
}

/// Helper to render a human error page
fn render_error_page(template: &Arc<tera::Tera>,title: &str, message: &str, url_back: &str) -> Result<axum::response::Response> {
    let mut context = tera::Context::new();
    context.insert("TITLE", title);
    context.insert("ERROR_INFO", message);
    context.insert("URL_BACK", url_back);
    let mut res = Html(template.render("error.html.j2", &context)?).into_response();
    *res.status_mut() = StatusCode::BAD_REQUEST;
    Ok(res)
}

/// Url path for admin main page
const fn url_path_overview() -> &'static str {
    "/admin"
}

/// URL path for host alerts
fn url_path_alerts(host: i32) -> String {
    format!("/admin/alerts/{host}")
}