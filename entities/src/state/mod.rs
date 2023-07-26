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
    }))
}

#[derive(Debug, Serialize)]
pub struct CacheData {
    pub hosts: Vec<CacheHost>,
    pub last_update: DateTimeUtc,
}

#[derive(Debug, Serialize)]
pub struct CacheHost {
    pub url: String,
    pub domain: String,
    pub points: i32,
    pub rss: bool,
    pub recent_pings: Vec<i32>,
    pub ping_max: i32,
    pub ping_min: i32,
    pub ping_avg: Option<i32>,
    pub version: Option<String>,
    pub healthy: bool,
}
