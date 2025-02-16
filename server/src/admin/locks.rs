use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    Form,
};
use entities::host_overrides::{
    self,
    keys::{HostOverrides, LOCKED_FALSE, LOCKED_TRUE},
};
use sea_orm::{sea_query::OnConflict, ActiveValue, DatabaseConnection, EntityTrait};
use tower_sessions::Session;

use crate::{Result, ServerError};

use super::get_specific_login_host;

pub async fn locks_view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let (host, login) = get_specific_login_host(instance, &session, db).await?;

    if !login.admin {
        return Err(ServerError::MissingPermission);
    }

    let overrides = HostOverrides::load(&host, db).await?;

    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    context.insert("HOST_ID", &instance);
    context.insert("OVERRIDES", overrides.entries());

    let res = Html(template.render("instance_locks.html.j2", &context)?).into_response();
    Ok(res)
}

/// Override Form
// #[derive(Deserialize, Debug)]
pub type LocksFormInput = HashMap<String, String>;

pub async fn post_locks(
    State(db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
    Form(input): Form<LocksFormInput>,
) -> Result<axum::response::Response> {
    let (host, login) = get_specific_login_host(instance, &session, &db).await?;

    if !login.admin {
        return Err(ServerError::MissingPermission);
    }

    let overrides = HostOverrides::load(&host, &db).await?;

    let mut updated = Vec::with_capacity(input.len());

    for (key, _value) in overrides.entries() {
        let locked = match input.contains_key(key) {
            true => LOCKED_TRUE,
            false => LOCKED_FALSE,
        };
        updated.push(host_overrides::ActiveModel {
            host: ActiveValue::Set(host.id),
            key: ActiveValue::Set(key.to_string()),
            locked: ActiveValue::Set(locked),
            value: ActiveValue::Set(None),
        });
    }
    host_overrides::Entity::insert_many(updated)
        .on_conflict(
            OnConflict::columns([host_overrides::Column::Host, host_overrides::Column::Key])
                .update_columns([host_overrides::Column::Locked])
                .to_owned(),
        )
        .exec(&db)
        .await?;

    Ok(Redirect::to(&format!("/admin/instance/locks/{}", instance)).into_response())
}
