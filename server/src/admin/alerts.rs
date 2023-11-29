// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use axum::extract::State;
use entities::state::AppState;
use sea_orm::DatabaseConnection;
use tower_sessions::Session;

use super::get_all_login_hosts;
use super::Result;

pub async fn view(
    State(ref app_state): State<AppState>,
    State(ref template): State<Arc<tera::Tera>>,
    State(ref db): State<DatabaseConnection>,
    session: Session,
) -> Result<axum::response::Response> {
    let (login, hosts) = get_all_login_hosts(&session, db, true).await?;

    todo!()
}
