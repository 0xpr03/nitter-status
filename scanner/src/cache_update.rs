// SPDX-License-Identifier: AGPL-3.0-only
use std::collections::HashMap;

use chrono::TimeZone;
use chrono::{Days, Utc};
use entities::host;
use entities::prelude::*;
use entities::state::CacheData;
use entities::state::CacheHost;
use sea_orm::ColumnTrait;
use sea_orm::EntityTrait;
use sea_orm::QueryFilter;
use sea_orm::QueryOrder;
use sea_orm::{prelude::DateTimeUtc, DbBackend, FromQueryResult, Statement};

use crate::version_check::fetch_git_state;
use crate::LatestCheck;
use crate::{Result, Scanner};

#[derive(Debug, FromQueryResult)]
pub struct HostStats {
    host: i32,
    good: u32,
    total: u32,
}

#[derive(Debug, Default)]
struct LastPings {
    avg: Option<i32>,
    min: Option<i32>,
    max: Option<i32>,
    pings: Vec<Option<i32>>,
}

#[derive(Debug, FromQueryResult)]
pub struct Version {
    version: String,
}

impl Scanner {
    pub(crate) async fn update_cache(&self) -> Result<()> {
        let new_data = self.generate_cache_data().await?;
        {
            let mut guard = self.inner.cache.write().unwrap();
            *guard = new_data;
        }
        Ok(())
    }

    /// Generate host stats and returns a new CacheData
    pub(crate) async fn generate_cache_data(&self) -> Result<CacheData> {
        let hosts = self.query_hosts_enabled().await?;
        let config_c = self.inner.config.clone();
        let current_version = tokio::task::spawn_blocking(move || fetch_git_state(config_c))
            .await
            .unwrap()?;
        if hosts.is_empty() {
            return Ok(CacheData {
                hosts: Vec::new(),
                last_update: Utc::now(),
                latest_commit: current_version.version,
            });
        }

        let time_now = Utc::now();
        let time_3h = time_now
            .checked_sub_signed(chrono::Duration::hours(3))
            .unwrap();
        let time_30d = time_now.checked_sub_days(Days::new(30)).unwrap();
        let time_120d = time_now.checked_sub_days(Days::new(120)).unwrap();

        let stats_3h = self.query_stats_range(time_3h, time_now).await?;
        let stats_30d = self.query_stats_range(time_30d, time_now).await?;
        let stats_120d = self.query_stats_range(time_120d, time_30d).await?;

        let mut last_healthy_check = self.query_last_healthy().await?;

        let version_points = self.query_versions(time_30d).await?;
        let latest_check = self.query_latest_check(&self.inner.db).await?;
        let latest_check: HashMap<i32, LatestCheck> =
            latest_check.into_iter().map(|v| (v.host, v)).collect();

        let mut ping_data = self
            .query_pings(time_now - self.inner.config.ping_range)
            .await?;

        let mut host_statistics = Vec::with_capacity(hosts.len());
        let default_health_check = LatestCheck::default();
        for host in hosts {
            let stats_3h_host = stats_3h
                .get(&host.id)
                .map_or(0.0, |stats| stats.good as f64 / stats.total as f64);
            let points_3h: f64 = 0.3 * stats_3h_host;
            let points_30d: f64 = 0.2
                * stats_30d
                    .get(&host.id)
                    .map_or(0.0, |stats| stats.good as f64 / stats.total as f64);
            let points_120d: f64 = 0.2
                * stats_120d
                    .get(&host.id)
                    .map_or(0.0, |stats| stats.good as f64 / stats.total as f64);
            let points_version = 0.1
                * host
                    .version
                    .as_ref()
                    .map_or(0.0, |version| *version_points.get(version).unwrap_or(&0.0));
            let points = points_30d + points_120d + points_version + points_3h;
            let points = stats_3h_host * points;

            let last_check = latest_check.get(&host.id).unwrap_or(&default_health_check);
            // // don't rank currently down instances highly
            // let points = match last_check.healthy {
            //     true => (points * 100.0) as i32,
            //     false => 0,
            // };
            let points = (points * 100.0) as i32;

            let latest_version = host
                .version_url
                .as_ref()
                .map_or(false, |url| current_version.is_same_version(&url));
            let is_upstream = host
                .version_url
                .as_ref()
                .map_or(false, |url| current_version.is_same_repo(&url));

            let is_bad_host =
                (!last_check.healthy) && self.inner.config.bad_hosts.contains(&host.domain);

            let host_ping_data = ping_data.remove(&host.id);
            host_statistics.push(CacheHost {
                last_healthy: last_healthy_check.remove(&host.id),
                url: host.url,
                domain: host.domain,
                points,
                rss: host.rss,
                version: host.version,
                healthy: last_check.healthy,
                ping_max: host_ping_data.as_ref().and_then(|v| v.max),
                ping_min: host_ping_data.as_ref().and_then(|v| v.min),
                ping_avg: host_ping_data.as_ref().and_then(|v| v.avg),
                recent_pings: host_ping_data.map(|v| v.pings).unwrap_or_default(),
                is_latest_version: latest_version,
                is_upstream,
                version_url: host.version_url,
                is_bad_host,
                country: host.country,
            })
        }
        host_statistics.sort_unstable_by_key(|k| k.points);
        host_statistics.reverse();
        Ok(CacheData {
            hosts: host_statistics,
            last_update: time_now,
            latest_commit: current_version.version,
        })
    }

    async fn query_hosts_enabled(&self) -> Result<Vec<host::Model>> {
        Ok(Host::find()
            .filter(host::Column::Enabled.eq(true))
            .order_by_asc(host::Column::Id)
            .all(&self.inner.db)
            .await?)
    }

    async fn query_pings(&self, age: DateTimeUtc) -> Result<HashMap<i32, LastPings>> {
        #[derive(Debug, FromQueryResult, Default)]
        struct PingEntry {
            host: i32,
            ping: Option<i32>,
        }
        let last_pings = PingEntry::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            SELECT u.host,(CASE u.healthy WHEN true THEN u.resp_time ELSE null END) as ping FROM health_check u
            JOIN host h ON h.id = u.host
            WHERE h.enabled = true AND u.time >= $1
            ORDER BY u.host,u.time ASC
            "#,
            [age.timestamp().into()],
        ))
        .all(&self.inner.db)
        .await?;
        // this is pretty verbose, but allows to
        // - calculate the avg
        // - map by host
        // - get a list of all past entries
        // in one go and with one DB request
        let mut map = HashMap::with_capacity(100);
        let mut iter = last_pings.iter();
        let first = match iter.next() {
            None => {
                return Ok(HashMap::new());
            }
            Some(v) => v,
        };
        let mut current_entry = LastPings::default();
        let mut last_host = first.host;
        let mut non_null_entries = first.ping.as_ref().map_or(0, |_| 1);
        if let Some(ping) = first.ping.as_ref() {
            current_entry.avg = Some(*ping);
            non_null_entries += 1;
            current_entry.min = Some(*ping);
            current_entry.max = Some(*ping);
        }
        current_entry.pings.push(first.ping);
        for ping in iter {
            if last_host != ping.host {
                let mut new_entry = LastPings::default();
                // will overflow only if we hit > 1500 days of backlog
                // when having 5 minutes interval and only 5000ms response times
                if let Some(sum) = current_entry.avg {
                    current_entry.avg = Some(sum / non_null_entries);
                }
                non_null_entries = 0;
                std::mem::swap(&mut new_entry, &mut current_entry);
                // insert back the old (swapped) entry
                assert_eq!(map.insert(last_host, new_entry).is_some(), false);
                last_host = ping.host;
            }
            if let Some(ping) = ping.ping.as_ref() {
                current_entry.avg = Some(current_entry.avg.unwrap_or(0) + ping);
                non_null_entries += 1;
                current_entry.min = Some(current_entry.min.map_or(*ping, |v| v.min(*ping)));
                current_entry.max = Some(current_entry.max.map_or(*ping, |v| v.max(*ping)));
            }
            current_entry.pings.push(ping.ping);
        }
        // Insert last entry
        if let Some(sum) = current_entry.avg {
            current_entry.avg = Some(sum / non_null_entries);
        }
        assert_eq!(map.insert(last_host, current_entry).is_some(), false);
        Ok(map)
    }

    async fn query_versions(&self, age: DateTimeUtc) -> Result<HashMap<String, f64>> {
        let stats = Version::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"SELECT version FROM host h
            JOIN health_check u ON u.host = h.id
            WHERE h.enabled = true AND u.time >= $1 AND version IS NOT NULL
            GROUP BY version
            ORDER BY version ASC"#,
            [age.timestamp().into()],
        ))
        .all(&self.inner.db)
        .await?;

        let amount = stats.len();
        let points_per_level: f64 = 1.0 / amount as f64;
        let stats_rated: HashMap<String, f64> = stats
            .into_iter()
            .zip(1..)
            .map(|(version, i)| (version.version, i as f64 * points_per_level))
            .collect();
        Ok(stats_rated)
    }

    /// Timestamp of last healthy host check
    async fn query_last_healthy(&self) -> Result<HashMap<i32, DateTimeUtc>> {
        #[derive(Debug, FromQueryResult)]
        struct LastHealthyEntry {
            host: i32,
            time: i64,
        }
        let last_healthy_times =
            LastHealthyEntry::find_by_statement(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                r#"
            SELECT u.host,MAX(u.time) as time FROM health_check u
            JOIN host h ON h.id = u.host
            WHERE h.enabled = true AND u.healthy = true
            GROUP BY u.host
            "#,
                [],
            ))
            .all(&self.inner.db)
            .await?;
        let last_healthy_times: HashMap<_, _> = last_healthy_times
            .into_iter()
            .map(|v| (v.host, Utc.timestamp_opt(v.time, 0).unwrap()))
            .collect();
        Ok(last_healthy_times)
    }

    /// Query uptime statistics per host
    async fn query_stats_range(
        &self,
        from: DateTimeUtc,
        to: DateTimeUtc,
    ) -> Result<HashMap<i32, HostStats>> {
        let stats: Vec<HostStats> = HostStats::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"SELECT u.host, COUNT(CASE WHEN healthy = true THEN 1 END) as good,COUNT(*) as total FROM health_check u
            JOIN host h ON h.id = u.host
            WHERE h.enabled = true AND u.time BETWEEN $1 AND $2
            GROUP BY u.host "#,
            [from.timestamp().into(), to.timestamp().into()],
        ))
        .all(&self.inner.db)
        .await?;
        let stats: HashMap<_, _> = stats.into_iter().map(|v| (v.host, v)).collect();
        Ok(stats)
    }
}
