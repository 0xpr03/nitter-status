// SPDX-License-Identifier: AGPL-3.0-only
use std::{borrow::Cow, net::SocketAddr, sync::Arc};

use axum::{extract::DefaultBodyLimit, http::HeaderValue, response::{Html, Redirect}, routing::{get, get_service}, Router, error_handling::HandleErrorLayer};
use entities::state::{scanner::ScannerConfig, AppState};
use hyper::{header, StatusCode};
use reqwest::Client;
use sea_orm::DatabaseConnection;
use tera::Tera;
use thiserror::Error;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer, limit::RequestBodyLimitLayer, services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer, trace::TraceLayer,
};
use tower_sessions::{SessionManagerLayer, cookie::SameSite};

mod api;
mod website;
mod admin;

const LOGIN_URL: &'static str = "/admin/login";
const ADMIN_OVERVIEW_URL: &'static str = "/admin";

#[derive(Debug)]
pub struct Config {
    pub site_url: String,
    pub max_age: usize,
    pub session_ttl_seconds: u64,
    pub login_token_name: String,
}

#[derive(Clone, axum::extract::FromRef)]
struct WebState {
    db: DatabaseConnection,
    config: Arc<Config>,
    scanner_config: ScannerConfig,
    app_state: AppState,
    templates: Arc<Tera>,
    login_client: Client,
}

/// Start webserver
pub async fn start(
    addr: &SocketAddr,
    db: DatabaseConnection,
    config: Config,
    scanner_config: ScannerConfig,
    app_state: AppState,
) -> Result<()> {
    #[cfg(debug_assertions)]
    let session_secure = false;
    #[cfg(not(debug_assertions))]
    let session_secure = true;
    if !session_secure {
        tracing::warn!("debug build, sessions are not secure!");
    }

    let session_store = tower_sessions::MemoryStore::default();
    let session_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|e: axum::BoxError| async move {
            tracing::debug!(session_error=?e);
            StatusCode::BAD_REQUEST
        }))
        .layer(
            SessionManagerLayer::new(session_store)
                .with_secure(session_secure)
                .with_path("/admin".to_string())
                .with_name("admin_login")
                .with_same_site(SameSite::Strict)
                .with_max_age(time::Duration::seconds(config.session_ttl_seconds as _)),
        );
    
    let user_agent = format!("nitter-status (+{}/about)",scanner_config.website_url);
    let login_client = Client::builder()
        .cookie_store(false)
        .brotli(true)
        .deflate(true)
        .gzip(true)
        .use_rustls_tls()
        .user_agent(user_agent)
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(10))
        .build().unwrap();

    let config = Arc::new(config);
    let mut tera = Tera::new("server/templates/*")?;
    tera.autoescape_on(vec![".html.j2"]);
    let state = WebState {
        config: config.clone(),
        db,
        app_state,
        scanner_config,
        templates: Arc::new(tera),
        login_client,
    };

    let router = Router::new()
        .nest_service(
            "/static",
            ServeDir::new("server/static").append_index_html_on_directories(false),
        )
        .route("/api/v1/instances", get(api::instances))
        .nest(ADMIN_OVERVIEW_URL, Router::new()
            .route("/", get(admin::overview))
            .route("/errors/:host", get(admin::errors_view))
            // .route("/history/:host", get(admin::history_view))
            // .route("/api/history", get(admin::history_json))
            .route("/login", get(admin::login_view).post(admin::login))
            .route("/logout", get(admin::logout))
            // .layer(ServiceBuilder::new().layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, "must-revalidate")))
            .layer(session_service)
        )
        // .route("/admin", get(admin::view))
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
                    "default-src 'self'; child-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:;"
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
    #[error("Not logged in")]
    NoLogin,
    #[error("Internal Error during DB request: {0:?}")]
    DBError(#[from] sea_orm::DbErr),
    #[error("Internal Error session handling: {0:?}")]
    SessionError(#[from] tower_sessions::session::SessionError),
    #[error("Host '{0}' can't be found, logging out user!")]
    HostNotFound(i32),
    #[error("No permission to access this resource")]
    MissingPermission,
}

impl axum::response::IntoResponse for ServerError {
    fn into_response(self) -> axum::response::Response {
        use ServerError::*;
        let msg = match &self {
            NoLogin =>  {
                let mut resp = Redirect::temporary(LOGIN_URL).into_response();
                // *resp.status_mut() = StatusCode::FOUND; // have to use a 301, [Redirect] 307 won't work for referrer
                return resp;
            },
            MissingPermission =>  
                (StatusCode::FORBIDDEN,Cow::Borrowed("Missing permission to access this resource"))
            ,
            MutexFailure | Templating(_) | DBError(_) | SessionError(_) | HostNotFound(_) => (
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
