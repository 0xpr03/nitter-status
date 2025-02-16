use std::sync::Arc;

use crate::Result;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
};
use entities::{check_errors, state::AppState};
use sea_orm::ColumnTrait;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::QuerySelect;
use tower_sessions::Session;

use crate::{admin::get_specific_login_host, ServerError};

pub async fn errors_view(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    tracing::info!(?session);

    let (host, _login) = get_specific_login_host(instance, &session, db).await?;

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

        let res = Html(template.render("instance_errors.html.j2", &context)?).into_response();
        drop(guard);
        res
    };
    Ok(res)
}
