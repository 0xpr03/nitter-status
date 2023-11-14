// SPDX-License-Identifier: AGPL-3.0-only
//! Instance health/uptime checking code
use std::time::Instant;

use chrono::Utc;
use entities::state::error_cache::HostError;
use entities::{check_errors, health_check};
use entities::{host, prelude::*};
use reqwest::Url;
use sea_orm::prelude::DateTimeUtc;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::{ActiveModelTrait, ActiveValue};
use tokio::task::JoinSet;
use tracing::instrument;

use crate::about_parser::AboutParsed;
use crate::Result;
use crate::Scanner;

impl Scanner {
    /// Check uptime for host and create a new uptime entry in the database
    pub(crate) async fn check_uptime(&mut self) -> Result<()> {
        let start = Instant::now();
        let hosts = Host::find()
            .filter(host::Column::Enabled.eq(true))
            .all(&self.inner.db)
            .await?;

        let mut join_set = JoinSet::new();

        let last_check = self.query_latest_check(&self.inner.db).await?;

        for model in hosts.into_iter() {
            let scanner = self.clone();
            let muted_host = last_check
                .iter()
                .find(|v| v.host == model.id)
                .map_or(false, |check| !check.healthy);
            join_set.spawn(async move {
                scanner.health_check_host(model, muted_host).await;
                
            });
        }
        // wait till all of them are finished, preventing DoS
        let tasks = join_set.len();
        while let Some(_) = join_set.join_next().await {}
        let end = Instant::now();
        let took_ms = end.saturating_duration_since(start).as_millis();
        *self.inner.last_uptime_check.lock().unwrap() = Utc::now();
        tracing::debug!(hosts = tasks, took_ms = took_ms, "checked uptime");
        Ok(())
    }

    #[instrument]
    async fn health_check_host(&self, host: host::Model, muted: bool) {
        let now = Utc::now();
        let mut url = match Url::parse(&host.url) {
            Err(e) => {
                if !muted {
                    tracing::error!(error=?e, url=host.url,"failed to parse instance URL");
                }
                self.insert_failed_health_check(
                    host.id,
                    now,
                    HostError::new_message(format!("Not a valid URL")),
                    None,
                )
                .await;
                return;
            }
            Ok(v) => v,
        };
        url.set_path(&self.inner.config.profile_path);
        let start = Instant::now();
        let fetch_res = self.fetch_url(url.as_str()).await;
        let end = Instant::now();
        let took_ms = end.saturating_duration_since(start).as_millis();
        match fetch_res {
            Err(e) => {
                if !muted {
                    tracing::info!(
                        host = host.url,
                        took = took_ms,
                        "couldn't ping host: {e}, marking as dead"
                    );
                }
                self.insert_failed_health_check(
                    host.id,
                    now,
                    e.to_host_error(),
                    Some(took_ms as _),
                )
                .await;
            }
            Ok((http_code, content)) => {
                if !muted {
                    tracing::trace!(host = host.url, took = took_ms);
                }
                // check for valid profile
                match self.inner.profile_parser.parse_profile_content(&content) {
                    Err(e) => {
                        if !muted {
                            tracing::debug!(
                                error=?e,
                                content = content,
                                "host doesn't contain a valid profile"
                            );
                        }
                        self.insert_failed_health_check(
                            host.id,
                            now,
                            HostError::new(e.to_string(), content, http_code),
                            Some(took_ms as _),
                        )
                        .await;
                    }
                    Ok(profile_content) => {
                        if self.inner.config.profile_name != profile_content.name
                            || self.inner.config.profile_posts_min > profile_content.post_count
                        {
                            if !muted {
                                tracing::debug!(
                                    profile_content = ?profile_content,
                                    "host doesn't contain expected profile content"
                                );
                            }
                            self.insert_failed_health_check(
                                host.id,
                                now,
                                HostError::new(
                                    format!("profile content mismatch"),
                                    content,
                                    http_code,
                                ),
                                Some(took_ms as _),
                            )
                            .await;
                        } else {
                            // create successful uptime entry
                            if let Err(e) = (health_check::ActiveModel {
                                time: ActiveValue::Set(now.timestamp()),
                                host: ActiveValue::Set(host.id),
                                resp_time: ActiveValue::Set(Some(took_ms as _)),
                                response_code: ActiveValue::Set(Some(http_code as _)),
                                healthy: ActiveValue::Set(true),
                            }
                            .insert(&self.inner.db)
                            .await)
                            {
                                tracing::error!(host=host.id, error=?e,"Failed to insert update check");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Check if rss is available
    pub(crate) async fn has_rss(&self, url: &mut Url, mute: bool) -> bool {
        url.set_path(&self.inner.config.rss_path);
        match self.fetch_url(url.as_str()).await {
            Ok((code, content)) => match self.inner.rss_check_regex.is_match(&content) {
                true => return true,
                false => {
                    if !mute {
                        // 404 = disabled
                        tracing::debug!(
                            url = url.as_str(),
                            code = code,
                            content = content,
                            "rss content not found"
                        );
                    }
                    return false;
                }
            },
            Err(e) => {
                if !mute && e.http_status_code() != Some(404) {
                    tracing::debug!(error=?e,url=url.as_str(),"fetching rss feed failed");
                }
                return false;
            }
        }
    }

    /// Check nitter version
    pub(crate) async fn nitter_version(&self, url: &mut Url, mute: bool) -> Option<AboutParsed> {
        url.set_path(&self.inner.config.about_path);
        match self.fetch_url(url.as_str()).await {
            Ok((code, content)) => match self.inner.about_parser.parse_about_version(&content) {
                Ok(v) => Some(v),
                Err(e) => {
                    if !mute {
                        tracing::debug!(url=url.as_str(),code,content,error=?e,"failed parsing version from about page");
                    }
                    None
                }
            },
            Err(e) => {
                if !mute {
                    tracing::debug!(url=url.as_str(),error=?e,"failed fetching about page");
                }
                None
            }
        }
    }

    async fn insert_failed_health_check(
        &self,
        host: i32,
        time: DateTimeUtc,
        host_error: HostError,
        resp_time: Option<i32>,
    ) {
        if let Err(e) = (health_check::ActiveModel {
            time: ActiveValue::Set(time.timestamp()),
            host: ActiveValue::Set(host),
            resp_time: ActiveValue::Set(resp_time),
            healthy: ActiveValue::Set(false),
            response_code: ActiveValue::Set(host_error.http_status),
        }
        .insert(&self.inner.db)
        .await)
        {
            tracing::error!(error=?e,"Failed to insert update check");
        }
        if let Err(e) = (check_errors::ActiveModel {
            time: ActiveValue::Set(host_error.time.timestamp()),
            host: ActiveValue::Set(host),
            message: ActiveValue::Set(host_error.message),
            http_body: ActiveValue::Set(host_error.http_body),
            http_status: ActiveValue::Set(host_error.http_status),
        }
        .insert(&self.inner.db)
        .await)
        {
            tracing::error!(host=host, error=?e,"Failed to insert error for host");
        }
    }
}
