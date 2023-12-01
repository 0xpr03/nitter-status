// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use axum::Form;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Html;
use axum::response::IntoResponse;
use entities::host;
use entities::state::new;
use sea_orm::ActiveModelTrait;
use sea_orm::ActiveValue;
use sea_orm::DatabaseConnection;
use sea_orm::EntityTrait;
use sea_orm::IntoActiveModel;
use sea_orm::ModelTrait;
use sea_orm::TransactionTrait;
use serde::Deserialize;
use tower_sessions::Session;

use crate::ServerError;

use super::get_specific_login_host;
use super::Result;
use entities::instance_alerts;
use entities::instance_mail;

/// cap alerts to <= 50% healthy accounts
const MAX_PERCENT_HEALTHY: i32 = 50;
/// Cap alerts to less than 10_000 healthy accounts
const MIN_HEALTHY_ACCOUNTS: i32 = 10000;
/// Cap alerts to at least 20 days AVG age
const MIN_ACCOUNT_AGE_AVG: i32 = 19;
/// Cap alerts to at least 3 times of unhealthy checks in a row
const MIN_HOST_UNHEALTHY_AMOUNT: i32 = 3;

pub async fn view(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    Path(instance): Path<i32>,
    session: Session,
) -> Result<axum::response::Response> {
    let host = get_specific_login_host(instance, &session, db).await?;
    render_settings(host, template, db).await
}

/// Render alerts.html.j2 for re-use in form answer and general view
async fn render_settings(host: host::Model, template: &Arc<tera::Tera>, db: &DatabaseConnection) -> Result<axum::response::Response> {
    let mail = host.find_related(instance_mail::Entity).one(db).await?;
    let alerts = host.find_related(instance_alerts::Entity).one(db).await?;
    tracing::info!(?mail,?alerts);
    let alerts = alerts.unwrap_or_else(||instance_alerts::Model::gen_defaults(host.id));
    let mut context = tera::Context::new();
    context.insert("HOST_DOMAIN", &host.domain);
    context.insert("HOST_ID", &host.id);
    context.insert("MAIL", &mail);
    context.insert("ALERTS", &alerts);
    context.insert("MAX_PERCENT_HEALTHY", &MAX_PERCENT_HEALTHY);
    context.insert("MIN_HEALTHY_ACCOUNTS", &MIN_HEALTHY_ACCOUNTS);
    context.insert("MIN_ACCOUNT_AGE_AVG", &MIN_ACCOUNT_AGE_AVG);
    context.insert("MIN_HOST_UNHEALTHY_AMOUNT", &MIN_HOST_UNHEALTHY_AMOUNT);

    let res = Html(template.render("alerts.html.j2", &context)?).into_response();
    Ok(res)
}

mod form_i32_opt {
    use serde::{self, Deserialize, Deserializer};
    use std::str::FromStr;

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<i32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.is_empty() {
            true => None,
            false => Some(i32::from_str(&s).map_err(serde::de::Error::custom)?)
        })
    }
}

mod form_bool {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(s == "true")
    }
}

#[derive(Debug,Deserialize)]
pub struct AlertSettingsForm {
    /// number of unhealthy checks after which to alert
    #[serde(with = "form_i32_opt")]
    pub host_down_amount: Option<i32>,
    #[serde(with = "form_bool")]
    #[serde(default)]
    pub host_down_amount_enable: bool,
    /// minimum number of alive accounts under which to alert
    #[serde(with = "form_i32_opt")]
    pub alive_accs_min_threshold: Option<i32>,
    #[serde(with = "form_bool")]
    #[serde(default)]
    pub alive_accs_min_threshold_enable: bool,
    /// minimum percentage of alive accounts under which to aliert
    #[serde(with = "form_i32_opt")]
    pub alive_accs_min_percent: Option<i32>,
    #[serde(with = "form_bool")]
    #[serde(default)]
    pub alive_accs_min_percent_enable: bool,
    /// Avg account age threshold for which to alert when crossed
    #[serde(with = "form_i32_opt")]
    pub avg_account_age_days: Option<i32>,
    #[serde(with = "form_bool")]
    #[serde(default)]
    pub avg_account_age_days_enable: bool,
}
pub async fn update(
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    session: Session,
    Path(instance): Path<i32>,
    Form(form): Form<AlertSettingsForm>,
) -> Result<axum::response::Response> {
    let host = get_specific_login_host(instance, &session, db).await?;
    tracing::debug!(?form);
    // easier to delete the row and re-create it
    // avoids missing rows for on_conflict update
    let transaction = db.begin().await?;
    instance_alerts::Entity::delete_by_id(host.id).exec(&transaction).await?;

    if form.alive_accs_min_percent.map_or(false, |v|v > MAX_PERCENT_HEALTHY || v < 0) {
        return Err(ServerError::FormValueError("alive_accs_min_percent"));
    }
    if form.alive_accs_min_threshold.map_or(false, |v|v > MIN_HEALTHY_ACCOUNTS) {
        return Err(ServerError::FormValueError("alive_accs_min_threshold"));
    }
    if form.avg_account_age_days.map_or(false, |v|v < MIN_ACCOUNT_AGE_AVG) {
        return Err(ServerError::FormValueError("avg_account_age_days"));
    }
    if form.host_down_amount.map_or(false, |v|v < MIN_HOST_UNHEALTHY_AMOUNT) {
        return Err(ServerError::FormValueError("host_down_amount"));
    }

    let new_model = instance_alerts::ActiveModel {
        host: ActiveValue::Set(host.id),
        host_down_amount: ActiveValue::Set(form.host_down_amount),
        host_down_amount_enable: ActiveValue::Set(form.host_down_amount_enable),
        alive_accs_min_threshold: ActiveValue::Set(form.alive_accs_min_threshold),
        alive_accs_min_threshold_enable: ActiveValue::Set(form.alive_accs_min_threshold_enable),
        alive_accs_min_percent: ActiveValue::Set(form.alive_accs_min_percent),
        alive_accs_min_percent_enable: ActiveValue::Set(form.alive_accs_min_percent_enable),
        avg_account_age_days: ActiveValue::Set(form.avg_account_age_days),
        avg_account_age_days_enable: ActiveValue::Set(form.avg_account_age_days_enable),
    };
    new_model.insert(&transaction).await?;
    transaction.commit().await?;

    render_settings(host, template, db).await
}

