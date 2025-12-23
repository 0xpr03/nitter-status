use std::{collections::HashMap, sync::Arc};

use crate::{Result, ServerError};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
};
use chrono::{TimeZone, Utc};
use entities::{host, log};
use sea_orm::EntityTrait;
use sea_orm::{DatabaseConnection, QueryOrder, QuerySelect};
use serde::Serialize;
use tower_sessions::Session;

use super::get_session_login;

#[derive(Serialize)]
struct LogEntry {
    time: String,
    by_user_host: String,
    key: String,
    for_host: Option<ForHost>,
    value: Option<String>,
}
#[derive(Serialize)]
struct ForHost {
    domain: String,
    id: i32,
}

pub async fn log_view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    session: Session,
) -> Result<axum::response::Response> {
    let login = get_session_login(&session)?;
    if !login.admin {
        return Err(ServerError::MissingPermission);
    }

    let mut host_cache = HostCache::default();

    let logs_raw = log::Entity::find()
        .order_by_desc(log::Column::Time)
        .all(db)
        .await?;
    let mut logs = Vec::with_capacity(logs_raw.len());

    for entry in logs_raw {
        let by_host = host_cache.get(entry.user_host, db).await?;
        let for_host = match entry.host_affected {
            None => None,
            Some(v) => Some(ForHost {
                domain: host_cache.get(v, db).await?,
                id: v,
            }),
        };
        let time = Utc
            .timestamp_opt(entry.time, 0)
            .unwrap()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        logs.push(LogEntry {
            by_user_host: by_host,
            for_host,
            key: entry.key,
            time,
            value: entry.new_value,
        });
    }

    let mut context = tera::Context::new();
    context.insert("LOGS", &logs);

    let res = Html(template.render("admin_logs.html.j2", &context)?).into_response();
    Ok(res)
}

#[derive(Default)]
struct HostCache(HashMap<i32, String>);

impl HostCache {
    async fn get(&mut self, host_id: i32, db: &DatabaseConnection) -> Result<String> {
        if let Some(v) = self.0.get(&host_id) {
            return Ok(v.clone());
        }
        let domain: Option<String> = host::Entity::find_by_id(host_id)
            .select_only()
            .column(host::Column::Domain)
            .into_tuple()
            .one(db)
            .await?;
        let Some(domain) = domain else {
            return Err(ServerError::HostNotFound(host_id));
        };
        self.0.insert(host_id, domain.clone());
        Ok(domain)
    }
}
