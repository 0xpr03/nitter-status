// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Result, ServerError};
use axum::response::IntoResponse;
use axum::{extract::State, Json};
use entities::state::Cache;
use hyper::http::HeaderValue;
use std::sync::Arc;

pub async fn instances(
    State(ref cache): State<Cache>,
    State(ref config): State<Arc<crate::Config>>,
) -> Result<axum::response::Response> {
    let mut res = {
        let guard = cache.read().map_err(|_| ServerError::MutexFailure)?;
        let res = Json(&*guard).into_response();
        drop(guard);
        res
    };
    res.headers_mut().insert(
        "cache-control",
        HeaderValue::from_str(&format!("public, max-age={}", config.max_age)).unwrap(),
    );
    res.headers_mut().insert("X-Robots-Tag", HeaderValue::from_static("noindex, nofollow"));
    Ok(res)
}
