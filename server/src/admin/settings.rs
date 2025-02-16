use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Result, ServerError};
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    Form,
};
use chrono::Utc;
use entities::{
    host_overrides::{self, keys::*},
    log,
};
use sea_orm::sea_query::OnConflict;
use sea_orm::ActiveModelTrait;
use sea_orm::EntityTrait;
use sea_orm::{ActiveValue, DatabaseConnection};
use serde::Deserialize;
use tower_sessions::Session;
use tracing::trace;

use super::get_specific_login_host;

pub async fn settings_view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let (host, login) = get_specific_login_host(instance, &session, db).await?;

    let overrides = HostOverrides::load(&host, db).await?;

    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    context.insert("HOST_ID", &instance);
    context.insert("IS_ADMIN", &login.admin);
    context.insert("OVERRIDES", overrides.entries());

    let res = Html(template.render("instance_settings.html.j2", &context)?).into_response();
    Ok(res)
}

/// Override Form
#[derive(Deserialize, Debug)]
pub struct OverrideFormInput {
    /// Some if checked a checkbox, none otherwise
    value: Option<String>,
    key: String,
}

pub async fn post_settings(
    State(template): State<Arc<tera::Tera>>,
    State(db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
    Form(input): Form<OverrideFormInput>,
) -> Result<axum::response::Response> {
    trace!(form=?input,host=instance,"post_override");
    let (host, login) = get_specific_login_host(instance, &session, &db).await?;

    let overrides = HostOverrides::load(&host, &db).await?;
    let Some(entry) = overrides.entries().get(&input.key) else {
        trace!("unknown override key");
        return Err(ServerError::InvalidOverrideKey);
    };

    if entry.locked && !login.admin {
        trace!(locked = entry.locked, "missing permissions");
        return Err(ServerError::MissingPermission);
    }

    let value = match (input.value.as_deref().map(|v|v.trim()), entry.value_type) {
        (None, _) => None,
        // don't insert empty strings
        (Some(""), ValueType::String) => None,
        (value, ValueType::String) => value,
        (Some(VAL_BOOL_TRUE), ValueType::Bool) => Some("true"),
        // don't allow arbitrary data
        (Some(_), ValueType::Bool) => Some("false"),
    }
    .map(|v| v.to_owned());

    // only for first-time inserts, not updated!
    let locked_value = ActiveValue::Set(match entry.locked {
        true => LOCKED_TRUE,
        false => LOCKED_FALSE,
    });

    let time = Utc::now().timestamp();
    log::ActiveModel {
        user_host: ActiveValue::Set(host.id),
        host_affected: ActiveValue::Set(Some(host.id)),
        key: ActiveValue::Set(input.key.clone()),
        time: ActiveValue::Set(time),
        new_value: ActiveValue::Set(value.clone()),
    }
    .insert(&db)
    .await?;

    let model = host_overrides::ActiveModel {
        host: ActiveValue::Set(host.id),
        key: ActiveValue::Set(input.key),
        locked: locked_value,
        value: ActiveValue::Set(value),
    };
    host_overrides::Entity::insert(model)
        .on_conflict(
            OnConflict::columns([host_overrides::Column::Host, host_overrides::Column::Key])
                .update_columns([host_overrides::Column::Value])
                .to_owned(),
        )
        .exec(&db)
        .await?;

    settings_view(State(template), State(db), Path(instance), session).await
}
