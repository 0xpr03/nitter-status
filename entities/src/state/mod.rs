// SPDX-License-Identifier: AGPL-3.0-only
//! Global state and structures.
//! For build process decoupling

use std::sync::{Arc, RwLock};

use chrono::Utc;
use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;

pub mod scanner;

pub type Cache = Arc<RwLock<CacheData>>;

pub fn new() -> Cache {
    Arc::new(RwLock::new(CacheData {
        hosts: vec![],
        last_update: Utc::now(),
        latest_commit: String::new(),
    }))
}

#[derive(Debug, Serialize)]
pub struct CacheData {
    pub hosts: Vec<CacheHost>,
    pub last_update: DateTimeUtc,
    pub latest_commit: String,
}

#[derive(Debug, Serialize)]
pub struct CacheHost {
    pub url: String,
    pub domain: String,
    pub points: i32,
    pub rss: bool,
    pub recent_pings: Vec<Option<i32>>,
    pub ping_max: Option<i32>,
    pub ping_min: Option<i32>,
    pub ping_avg: Option<i32>,
    pub version: Option<String>,
    pub version_url: Option<String>,
    pub healthy: bool,
    pub last_healthy: Option<DateTimeUtc>,
    /// Whether the source is from the normal upstream repo
    pub is_upstream: bool,
    /// Whether the source is from the latest upstream commit
    pub is_latest_version: bool,
    /// Whether this host is known to be bad (ip blocking)
    pub is_bad_host: bool,
    pub country: String,
}
