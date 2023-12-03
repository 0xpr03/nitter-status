// SPDX-License-Identifier: AGPL-3.0-only
//! Handles alert notifications
use std::cmp;
use std::collections::HashMap;
use std::fmt::format;
use std::time::Instant;

use chrono::{Days, Utc};
use chrono::{Duration, TimeZone};
use entities::prelude::*;
use entities::state::CacheData;
use entities::state::CacheHost;
use entities::{host, instance_alerts, instance_stats};
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::{prelude::DateTimeUtc, DbBackend, FromQueryResult, Statement};
use sea_orm::{ColumnTrait, QuerySelect};

use crate::{Result, Scanner, ScannerError};

impl Scanner {
    pub(crate) async fn check_for_alerts(&self) -> Result<()> {
        let start = Instant::now();
        let instance_alert_configs: HashMap<i32, instance_alerts::Model> = InstanceAlerts::find()
            .all(&self.inner.db)
            .await?
            .into_iter()
            .map(|m| (m.host, m))
            .collect();
        let host_mail = InstanceMail::find().all(&self.inner.db).await?;

        let newest_stats_timestamp =
            Scanner::query_last_entry_for_table(&self.inner.db, "instance_stats").await?;

        for entry in host_mail {
            if let Some(alert_config) = instance_alert_configs.get(&entry.host) {
                let host_stats = InstanceStats::find()
                    .filter(instance_stats::Column::Host.eq(entry.host))
                    .filter(instance_stats::Column::Time.eq(newest_stats_timestamp))
                    .one(&self.inner.db)
                    .await?;

                let mut mail = String::new();
                if let Some(message) = self.check_alert_host_unhealthy(alert_config).await? {
                    mail.push_str(&message);
                    mail.push('\n');
                }
                if let Some(message) = self.check_alert_min_alive_accounts(alert_config, &host_stats).await? {
                    mail.push_str(&message);
                    mail.push('\n');
                }
                match (
                    alert_config.alive_accs_min_percent,
                    alert_config.alive_accs_min_percent_enable,
                ) {
                    (Some(config), true) => (),
                    _ => (),
                }
                match (
                    alert_config.avg_account_age_days,
                    alert_config.avg_account_age_days_enable,
                ) {
                    (Some(config), true) => (),
                    _ => (),
                }
            }
        }

        let end = Instant::now();
        let diff = start - end;
        tracing::debug!(took_ms = diff.as_secs(), "alert check finished");

        Ok(())
    }

    /// Check if host needs an alert for being unhealthy.  
    /// Returns a string for the mail if applicable.
    async fn check_alert_host_unhealthy(
        &self,
        config: &instance_alerts::Model,
    ) -> Result<Option<String>> {
        let alert_threshold = match (config.host_down_amount, config.host_down_amount_enable) {
            (Some(config), true) => config,
            _ => return Ok(None),
        };

        let host = Host::find_by_id(config.host).one(&self.inner.db).await?;
        let host = host.ok_or_else(|| ScannerError::MissingData(config.host))?;

        if let Some(account_avg_age) = host.account_age_average {
            let account_avg_age = Utc.timestamp_opt(account_avg_age, 0).unwrap();
            let diff = Utc::now() - account_avg_age;
            if diff.abs() >= Duration::days(alert_threshold as _) {
                let message = format!(
                    "Average account age reached {}! Alert threshold at {} days.",
                    account_avg_age, alert_threshold
                );
                return Ok(Some(message));
            }
        }

        Ok(None)
    }

    /// Check is total - limited accounts < threshold and alert
    async fn check_alert_min_alive_accounts(
        &self,
        config: &instance_alerts::Model,
        stats: &instance_stats::Model,
    ) -> Result<Option<String>> {
        let alert_threshold = match (
            config.alive_accs_min_threshold,
            config.alive_accs_min_threshold_enable,
        ) {
            (Some(config), true) => config,
            _ => return Ok(None),
        };

        let unlimited_accs = stats.total_accs - stats.limited_accs;
        if stats.total_accs - stats.limited_accs < alert_threshold {
            let message = format!(
                "Usable accounts at {} from {} total. Threshold at {} unlimited accounts.",
                unlimited_accs, stats.total_accs, alert_threshold
            );
            return Ok(Some(message));
        }

        Ok(None)
    }
}
