// SPDX-License-Identifier: AGPL-3.0-only
//! Updates the list of available instances, fetching all required fields

use std::time::Duration;
use std::time::Instant;

use chrono::Utc;
use entities::host;
use entities::prelude::Host;
use reqwest::Url;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use sea_query::OnConflict;
use tokio::task::JoinSet;
use tracing::instrument;

use crate::Result;
use crate::Scanner;

impl Scanner {
    /// Fetches the list of all instances from the wiki.  
    /// Updates all fields for host::Model, including connectivity, rss, version and enabled.
    #[instrument]
    pub(crate) async fn update_instacelist(&mut self) -> Result<()> {
        let start = Instant::now();
        let html: String = self.fetch_instance_list().await?;
        let parsed_instances = self.inner.instance_parser.parse_instancelist(
            &html,
            &self.inner.config.additional_hosts,
            &self.inner.config.additional_host_country,
            false,
        )?;

        let transaction = self.inner.db.begin().await?;

        // find all currently enabled instances
        let enabled_hosts = Host::find()
            .filter(host::Column::Enabled.eq(true))
            .all(&transaction)
            .await?;
        // make a diff and remove the ones not found while parsing
        let time: chrono::DateTime<Utc> = Utc::now();
        let mut removed = 0;
        for host in enabled_hosts.iter() {
            if !parsed_instances.contains_key(&host.domain) {
                host::ActiveModel {
                    id: ActiveValue::Set(host.id),
                    enabled: ActiveValue::Set(false),
                    updated: ActiveValue::Set(time.timestamp()),
                    ..Default::default()
                }
                .update(&transaction)
                .await?;
                removed += 1;
            }
        }
        // now update/insert the existing ones
        let found_instances: usize = parsed_instances.len();
        // find last update checks to detect spam
        let last_status = self.query_latest_check(&transaction).await?;
        let mut join_set = JoinSet::new();
        for (_, instance) in parsed_instances {
            // TODO: parallelize this!
            let scanner_c = self.clone();
            // detect already offline host and prevent log spam
            let muted_host = match self.inner.config.auto_mute {
                false => false,
                true => last_status
                    .iter()
                    .find(|v| v.domain == instance.domain)
                    .map_or(false, |check| !check.healthy),
            };
            // tracing::trace!(muted_host,instance=?instance,last_status=?last_status);
            join_set.spawn(async move {
                let (connectivity, rss, version, version_url) = match Url::parse(&instance.url) {
                    Err(_) => {
                        if !muted_host {
                            tracing::info!(url = instance.url, "Instance URL invalid");
                        }
                        (None, false, None, None)
                    }
                    Ok(mut url) => {
                        let connectivity = scanner_c.check_connectivity(&mut url).await;
                        // prevent DoS
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        let rss = scanner_c.has_rss(&mut url, muted_host).await;
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        match scanner_c.nitter_version(&mut url, muted_host).await {
                            Some(version) => (
                                connectivity,
                                rss,
                                Some(version.version_name),
                                Some(version.url),
                            ),
                            None => (connectivity, rss, None, None),
                        }
                    }
                };

                host::ActiveModel {
                    id: ActiveValue::NotSet,
                    domain: ActiveValue::Set(instance.domain),
                    country: ActiveValue::Set(instance.country),
                    url: ActiveValue::Set(instance.url),
                    enabled: ActiveValue::Set(true),
                    version: ActiveValue::Set(version),
                    version_url: ActiveValue::Set(version_url),
                    rss: ActiveValue::Set(rss),
                    updated: ActiveValue::Set(time.timestamp()),
                    connectivity: ActiveValue::Set(connectivity),
                }
            });
        }
        while let Some(update_model) = join_set.join_next().await.map(|v| v.unwrap()) {
            Host::insert(update_model)
                .on_conflict(
                    OnConflict::column(host::Column::Domain)
                        .update_columns([
                            host::Column::Enabled,
                            host::Column::Updated,
                            host::Column::Url,
                            host::Column::Rss,
                            host::Column::Version,
                            host::Column::VersionUrl,
                            host::Column::Country,
                            host::Column::Connectivity,
                        ])
                        .to_owned(),
                )
                .exec(&transaction)
                .await?;
        }

        transaction.commit().await?;
        let end = Instant::now();
        let took_ms = end.saturating_duration_since(start).as_millis();
        {
            *self.inner.last_list_fetch.lock().unwrap() = Utc::now();
        }
        tracing::debug!(
            removed = removed,
            found = found_instances,
            took_ms = took_ms
        );
        Ok(())
    }

    /// Check ipv4/6 connectivity of host
    async fn check_connectivity(&self, url: &mut Url) -> Option<host::Connectivity> {
        url.set_path(&self.inner.config.connectivity_path);
        let ipv4 = self
            .inner
            .client_ipv4
            .get(url.as_str())
            .send()
            .await
            .map_or(false, |res| res.status().is_success());
        // prevent DoS
        tokio::time::sleep(Duration::from_secs(1)).await;
        let ipv6 = self
            .inner
            .client_ipv6
            .get(url.as_str())
            .send()
            .await
            .map_or(false, |res| res.status().is_success());

        match (ipv4, ipv6) {
            (true, true) => Some(host::Connectivity::All),
            (false, true) => Some(host::Connectivity::IPv6),
            (true, false) => Some(host::Connectivity::IPv4),
            (false, false) => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use entities::state::scanner::Config;
    use tracing_test::traced_test;

    use crate::{test::db_init, Scanner};

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[traced_test]
    #[ignore]
    async fn connectivity_test() {
        let db = db_init().await;
        let scanner = Scanner::new(db, Config::test_defaults(), entities::state::new())
            .await
            .unwrap();
        assert_eq!(
            scanner
                .check_connectivity(&mut Url::parse("https://v4.ipv6test.app").unwrap())
                .await,
            Some(host::Connectivity::IPv4)
        );
        assert_eq!(
            scanner
                .check_connectivity(&mut Url::parse("https://ipv6test.app").unwrap())
                .await,
            Some(host::Connectivity::All)
        );
        assert_eq!(
            scanner
                .check_connectivity(&mut Url::parse("https://v6.ipv6test.app").unwrap())
                .await,
            Some(host::Connectivity::IPv6)
        );
    }
}
