// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use sea_orm::DatabaseConnection;
use sea_orm::ModelTrait;
use tower_sessions::Session;

use super::get_specific_login_host;
use super::Result;
use entities::instance_alerts;
use entities::instance_mail;

pub async fn view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let host = get_specific_login_host(instance, &session, db).await?;
    let mail = host.find_related(instance_mail::Entity).one(db).await?;
    let alerts = host.find_related(instance_alerts::Entity).one(db).await?;
    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    context.insert("HOST_ID", &instance);
    context.insert("MAIL", &mail);
    context.insert("ALERTS", &alerts);

    let res = Html(template.render("alerts.html.j2", &context)?).into_response();
    Ok(res)
}

