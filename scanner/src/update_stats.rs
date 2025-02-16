// SPDX-License-Identifier: AGPL-3.0-only

//! for .health endpoint statistics gathering

use std::time::Instant;

use chrono::Utc;
use entities::host_overrides::keys::HostOverrides;
use entities::prelude::Host;
use entities::{host, instance_stats};
use reqwest::Url;
use sea_orm::prelude::DateTimeUtc;
use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use serde::Deserialize;
use tokio::task::JoinSet;

use crate::{Result, Scanner, ScannerError};

/// Instance stats reported by .health
#[derive(Debug, Deserialize)]
struct InstanceStats {
    accounts: InstanceStatsAccs,
    requests: RequestStats,
}

#[derive(Debug, Deserialize)]
struct InstanceStatsAccs {
    total: i32,
    limited: i32,
    oldest: DateTimeUtc,
    newest: DateTimeUtc,
    average: DateTimeUtc,
}

#[derive(Debug, Deserialize)]
struct RequestStats {
    total: i64,
    apis: APIStats,
}

/// Instance api stats reported by .health
#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct APIStats {
    pub photoRail: i32,
    pub userScreenName: i32,
    pub search: i32,
    pub listTweets: i32,
    pub userMedia: i32,
    pub tweetDetail: i32,
    pub list: i32,
    pub userTweets: i32,
    pub userTweetsAndReplies: i32,
}

impl Scanner {
    pub(crate) async fn check_health(&self) -> Result<()> {
        let hosts = Host::find()
            .filter(host::Column::Enabled.eq(true))
            .all(&self.inner.db)
            .await?;
        let start = Instant::now();

        let mut join_set = JoinSet::new();
        let time = Utc::now();
        for model in hosts.into_iter() {
            let scanner = self.clone();
            join_set.spawn(async move {
                let res = scanner.fetch_instance_stats(time, &model).await;
                if let Err(e) = &res {
                    tracing::debug!(host=model.id, error=?e,"Failed to fetch instance stats");
                }
                res.ok()
            });
        }

        let mut stat_data = Vec::with_capacity(join_set.len());
        while let Some(join_res) = join_set.join_next().await {
            if let Some(data) = join_res? {
                stat_data.push(data);
            }
        }
        tracing::debug!(db_stats_entries = stat_data.len());
        if !stat_data.is_empty() {
            instance_stats::Entity::insert_many(stat_data)
                .exec(&self.inner.db)
                .await?;
        }

        let end = Instant::now();
        let duration = end - start;
        {
            *self.inner.last_stats_fetch.lock().unwrap() = Utc::now();
        }
        tracing::debug!(duration=?duration,"stats check finished");
        Ok(())
    }

    async fn fetch_instance_stats(
        &self,
        time: DateTimeUtc,
        host: &host::Model,
    ) -> Result<instance_stats::ActiveModel> {
        let overrides = HostOverrides::load(&host, &self.inner.db).await?;
        let mut url = Url::parse(&host.url).map_err(|e| ScannerError::InstanceUrlParse)?;
        url.set_path(".health");
        if let Some(url_override) = overrides.health_path() {
            url.set_path(url_override);
        }
        if let Some(path_override) = overrides.health_query() {
            url.set_query(Some(path_override));
        }
        let (_code, body) = self.fetch_url(url.as_str(), overrides.bearer()).await?;

        let stats_data: InstanceStats =
            serde_json::from_str(&body).map_err(|e| ScannerError::StatsParsing(e, body))?;

        let stats_model = instance_stats::ActiveModel {
            time: ActiveValue::Set(time.timestamp()),
            host: ActiveValue::Set(host.id),
            limited_accs: ActiveValue::Set(stats_data.accounts.limited),
            total_accs: ActiveValue::Set(stats_data.accounts.total),
            total_requests: ActiveValue::Set(stats_data.requests.total),
            req_photo_rail: ActiveValue::Set(stats_data.requests.apis.photoRail),
            req_user_screen_name: ActiveValue::Set(stats_data.requests.apis.userScreenName),
            req_search: ActiveValue::Set(stats_data.requests.apis.search),
            req_list_tweets: ActiveValue::Set(stats_data.requests.apis.listTweets),
            req_user_media: ActiveValue::Set(stats_data.requests.apis.userMedia),
            req_tweet_detail: ActiveValue::Set(stats_data.requests.apis.tweetDetail),
            req_list: ActiveValue::Set(stats_data.requests.apis.list),
            req_user_tweets: ActiveValue::Set(stats_data.requests.apis.userTweets),
            req_user_tweets_and_replies: ActiveValue::Set(
                stats_data.requests.apis.userTweetsAndReplies,
            ),
        };

        Ok(stats_model)
    }
}
