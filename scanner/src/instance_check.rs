// SPDX-License-Identifier: AGPL-3.0-only
//! Instance health/uptime checking code
use std::time::Instant;

use chrono::Utc;
use entities::update_check;
use entities::{host, prelude::*};
use reqwest::Url;
use sea_orm::prelude::DateTimeUtc;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::{ActiveModelTrait, ActiveValue};
use tokio::task::JoinSet;
use tracing::instrument;

use crate::FetchError;
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

        for model in hosts.into_iter() {
            let scanner = self.clone();
            join_set.spawn(async move {
                scanner.update_check_host(model).await;
            });
        }
        // wait till all of them are finished, preventing DoS
        let tasks = join_set.len();
        while let Some(_) = join_set.join_next().await {}
        let end = Instant::now();
        let took_ms = end.saturating_duration_since(start).as_millis();
        *self.inner.last_uptime_check.lock().unwrap() = end;
        tracing::debug!(hosts = tasks, took_ms = took_ms, "checked uptime");
        Ok(())
    }

    #[instrument]
    async fn update_check_host(&self, host: host::Model) {
        let now = Utc::now();
        let mut url = match Url::parse(&host.url) {
            Err(e) => {
                tracing::error!(error=?e, url=host.url,"failed to parse instance URL");
                self.insert_failed_update_check(host.id, now, None, None)
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
                tracing::info!(
                    host = host.url,
                    took = took_ms,
                    "couldn't ping host: {e}, marking as dead"
                );
                let (http_code, resp_time) = match e {
                    FetchError::HttpResponseStatus(code, _, _) => {
                        (Some(code as _), Some(took_ms as _))
                    }
                    _ => (None, None),
                };
                self.insert_failed_update_check(host.id, now, resp_time, http_code)
                    .await;
            }
            Ok((http_code, content)) => {
                tracing::trace!(host = host.url, took = took_ms);
                // check for valid profile
                if !self.inner.health_check_regex.is_match(&content) {
                    tracing::info!(
                        content = content,
                        "host doesn't contain expected profile content"
                    );
                    self.insert_failed_update_check(
                        host.id,
                        now,
                        Some(took_ms as _),
                        Some(http_code as _),
                    )
                    .await;
                }

                // create successfull uptime entry
                if let Err(e) = (update_check::ActiveModel {
                    time: ActiveValue::Set(now),
                    host: ActiveValue::Set(host.id),
                    resp_time: ActiveValue::Set(Some(took_ms as _)),
                    response_code: ActiveValue::Set(Some(http_code as _)),
                    healthy: ActiveValue::Set(true),
                }
                .insert(&self.inner.db)
                .await)
                {
                    tracing::error!(error=?e,"Failed to insert update check");
                }
            }
        }
    }

    /// Check if rss is available
    pub(crate) async fn has_rss(&self, url: &mut Url) -> bool {
        url.set_path(&self.inner.config.rss_path);
        match self.fetch_url(url.as_str()).await {
            Ok((code, content)) => match self.inner.rss_check_regex.is_match(&content) {
                true => return true,
                false => {
                    tracing::debug!(code = code, content = content, "rss content not found");
                    return false;
                }
            },
            Err(e) => {
                tracing::debug!(error=?e,"fetching rss feed failed");
                return false;
            }
        }
    }

    /// Check nitter version
    pub(crate) async fn nitter_version(&self, url: &mut Url) -> Option<String> {
        url.set_path(&self.inner.config.about_path);
        match self.fetch_url(url.as_str()).await {
            Ok((code, content)) => match self.inner.about_parser.parse_about_version(&content) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::debug!(url=url.as_str(),code,content,error=?e,"failed parsing version from about page");
                    None
                }
            },
            Err(e) => {
                tracing::debug!(url=url.as_str(),error=?e,"failed fetching about page");
                None
            }
        }
    }

    async fn insert_failed_update_check(
        &self,
        host: i32,
        time: DateTimeUtc,
        resp_time: Option<i32>,
        http_code: Option<i32>,
    ) {
        if let Err(e) = (update_check::ActiveModel {
            time: ActiveValue::Set(time),
            host: ActiveValue::Set(host),
            resp_time: ActiveValue::Set(resp_time),
            healthy: ActiveValue::Set(false),
            response_code: ActiveValue::Set(http_code),
        }
        .insert(&self.inner.db)
        .await)
        {
            tracing::error!(error=?e,"Failed to insert update check");
        }
    }
}
