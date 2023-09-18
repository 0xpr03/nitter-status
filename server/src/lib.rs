// SPDX-License-Identifier: AGPL-3.0-only
use std::{borrow::Cow, net::SocketAddr, sync::Arc};

use axum::{extract::DefaultBodyLimit, http::HeaderValue, response::Html, routing::{get, get_service}, Router};
use entities::state::{scanner::ScannerConfig, Cache};
use hyper::{header, StatusCode};
use sea_orm::DatabaseConnection;
use tera::Tera;
use thiserror::Error;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer, limit::RequestBodyLimitLayer, services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer, trace::TraceLayer,
};

mod api;
mod website;

#[derive(Debug)]
pub struct Config {
    pub site_url: String,
    pub max_age: usize,
}

#[derive(Clone, axum::extract::FromRef)]
struct AppState {
    db: DatabaseConnection,
    config: Arc<Config>,
    scanner_config: ScannerConfig,
    cache: Cache,
    templates: Arc<Tera>,
}

/// Start webserver
pub async fn start(
    addr: &SocketAddr,
    db: DatabaseConnection,
    config: Config,
    scanner_config: ScannerConfig,
    cache: Cache,
) -> Result<()> {
    let config = Arc::new(config);
    let tera = Tera::new("server/templates/*")?;
    let state = AppState {
        config: config.clone(),
        db,
        cache,
        scanner_config,
        templates: Arc::new(tera),
    };
    let router = Router::new()
        .nest_service(
            "/static",
            ServeDir::new("server/static").append_index_html_on_directories(false),
        )
        .route("/api/v1/instances", get(api::instances))
        .route("/about", get(website::about))
        .route(
            "/robots.txt",
            get_service(ServeFile::new("server/static/robots.txt")),
        )
        .route("/", get(website::instances))
        .layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::disable())
                .layer(RequestBodyLimitLayer::new(2usize.pow(20) * 2))
                .layer(TraceLayer::new_for_http())
                .layer(cors_policy(&config.site_url))
                .layer(SetResponseHeaderLayer::overriding(
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'self'; child-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline';"
                        .parse::<HeaderValue>()
                        .expect("Invalid CSP header value"),
                )),
        )
        .with_state(state.clone());
    tracing::debug!("Starting server with config {:?}", *config);
    tracing::info!("listening on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(router.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("Failed to start webserver");
    Ok(())
}

fn cors_policy(_site_url: &str) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}

type Result<T = Html<String>> = std::result::Result<T, ServerError>;

#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Failed to access mutex")]
    MutexFailure,
    #[error("Failed to perform templating: {0:?}")]
    Templating(#[from] tera::Error),
}

impl axum::response::IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        use ServerError::*;
        let msg = match &self {
            MutexFailure | Templating(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Cow::Borrowed("Internal Server Error"),
            ),
        };
        if msg.0 == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(?self);
        }
        msg.into_response()
    }
}
