// SPDX-License-Identifier: AGPL-3.0-only
//! Handles alert notifications
use std::cmp;
use std::collections::HashMap;
use std::time::Instant;

use chrono::{Days, Utc};
use chrono::{Duration, TimeZone};
use entities::{host, instance_alerts};
use entities::prelude::*;
use entities::state::CacheData;
use entities::state::CacheHost;
use sea_orm::{ColumnTrait, QuerySelect};
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::{prelude::DateTimeUtc, DbBackend, FromQueryResult, Statement};

use crate::{Result, Scanner, ScannerError};


impl Scanner {
    pub(crate) async fn check_for_alerts(&self) -> Result<()> {
        let start = Instant::now();
        let instance_alert_configs: HashMap<i32,instance_alerts::Model> = InstanceAlerts::find()
            .all(&self.inner.db).await?.into_iter().map(|m|(m.host,m)).collect();
        let host_mail = InstanceMail::find().all(&self.inner.db).await?;

        for entry in host_mail {
            if let Some(alert_config) = instance_alert_configs.get(&entry.host) {
                let mut mail = String::new();
                if let Some(message) = self.check_alert_host_unhealthy(alert_config).await? {
                    mail.push_str(&message);
                    mail.push('\n');
                }
                match (alert_config.alive_accs_min_threshold,alert_config.alive_accs_min_threshold_enable) {
                    (Some(config),true) => (),
                    _ => ()
                }
                match (alert_config.alive_accs_min_percent,alert_config.alive_accs_min_percent_enable) {
                    (Some(config),true) => (),
                    _ => ()
                }
                match (alert_config.avg_account_age_days,alert_config.avg_account_age_days_enable) {
                    (Some(config),true) => (),
                    _ => ()
                }
            }
        }

        let end = Instant::now();
        let diff = start - end;
        tracing::debug!(took_ms=diff.as_secs(),"alert check finished");

        Ok(())
    }

    /// Check if host needs an alert for being unhealthy.  
    /// Returns a string for the mail if applicable.
    async fn check_alert_host_unhealthy(&self, config: &instance_alerts::Model) -> Result<Option<String>> {
        let alert_threshold = match (config.host_down_amount,config.host_down_amount_enable) {
            (Some(config),true) => config,
            _ => return Ok(None),
        };

        let host = Host::find_by_id(config.host).one(&self.inner.db).await?;
        let host = host.ok_or_else(||ScannerError::MissingData(config.host))?;

        if let Some(account_avg_age) = host.account_age_average {
            if account_avg_age >
        }

        Ok(None)
    }
}