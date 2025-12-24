// SPDX-License-Identifier: AGPL-3.0-only
//! Global state and structures.
//! For build process decoupling

use std::sync::{Arc, RwLock};

use chrono::Utc;
use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;

use crate::host::Connectivity;

/// Log for recent host errors
pub mod error_cache;
pub mod scanner;

pub type Cache = RwLock<CacheData>;

pub type AppState = Arc<InnerState>;

pub struct InnerState {
    pub cache: RwLock<CacheData>,
}

/// Resolved information about an instances nitter source commit
#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub enum CommitInfo {
    /// Commit is behind main
    Outdated,
    /// Commit equals current main
    Current,
    /// Commit is inside a custom branch on main
    CustomBranch,
    /// Commit doesn't exist in the repo
    UnknownCommit,
    /// Missing commit (invalid URL etc)
    Missing,
}

impl CommitInfo {
    pub fn is_latest_version(&self) -> bool {
        *self == Self::Current
    }
    pub fn is_upstream(&self) -> bool {
        match self {
            CommitInfo::Outdated | CommitInfo::Current => true,
            CommitInfo::CustomBranch | CommitInfo::UnknownCommit | CommitInfo::Missing => false,
        }
    }
}

pub fn new() -> AppState {
    Arc::new(InnerState {
        cache: RwLock::new(CacheData {
            hosts: vec![],
            last_update: Utc::now(),
            latest_commit: String::new(),
        }),
    })
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
    /// State of the sources nitter version information
    pub version_state: CommitInfo,
    /// Whether the source is from the normal upstream repo
    #[deprecated = "use version_state"]
    pub is_upstream: bool,
    /// Whether the source is from the latest upstream commit
    #[deprecated = "use version_state"]
    pub is_latest_version: bool,
    /// Whether this host is known to be bad (ip blocking)
    pub is_bad_host: bool,
    /// Country from the wiki
    pub country: String,
    /// Last health checks time formatted, healthy
    pub recent_checks: Vec<(String, bool)>,
    /// Percentage of healthy checks since first seen
    pub healthy_percentage_overall: u8,
    pub connectivity: Option<Connectivity>,
    /// Internal: show last-seen information
    pub __show_last_seen: bool,
}
