// SPDX-License-Identifier: AGPL-3.0-only
use std::{sync::Arc, time::Duration};
pub type ScannerConfig = Arc<Config>;
#[derive(Debug)]
pub struct Config {
    /// time until next instance list fetch
    pub list_fetch_interval: Duration,
    /// time until next instance ping check
    pub instance_check_interval: Duration,
    /// time until next instance statistics check
    pub instance_stats_interval: Duration,
    /// instances list URL
    pub instance_list_url: String,
    /// profile path for health check
    pub profile_path: String,
    /// rss path for health check
    pub rss_path: String,
    /// about path for version check
    pub about_path: String,
    /// Expected profile name for a valid profile health check
    pub profile_name: String,
    /// Expected minimum of timeline posts for a valid profile health check
    pub profile_posts_min: usize,
    /// Expected string for a valid RSS health check
    pub rss_content: String,
    /// List of additional hosts to include during health checks
    pub additional_hosts: Vec<String>,
    /// Country to use for additional hosts
    pub additional_host_country: String,
    /// Website URL of this service
    pub website_url: String,
    /// Duration to average the ping/response times over
    pub ping_range: chrono::Duration,
    /// don't emit errors for hosts which are already listed as down
    pub auto_mute: bool,
    /// Git URL for source fetching
    pub source_git_url: String,
    /// Git branch to fetch the current commit from
    pub source_git_branch: String,
    /// Folder to use for git operations during nitter version checks
    pub git_scratch_folder: String,
    /// Interval to run cleanup operations in, to remove old data
    pub cleanup_interval: Duration,
    /// Amount of latest errors to keep per instance/host
    pub error_retention_per_host: usize,
    /// Path for connectivity checks
    pub connectivity_path: String,
}

impl Config {
    pub fn test_defaults() -> ScannerConfig {
        Arc::new(Config {
            instance_stats_interval: Duration::from_secs(15 * 60),
            list_fetch_interval: Duration::from_secs(15 * 60),
            instance_check_interval: Duration::from_secs(15 * 60),
            instance_list_url: String::from("https://github.com/zedeus/nitter/wiki/Instances"),
            profile_path: String::from("/jack"),
            rss_path: String::from("/jack/rss"),
            about_path: String::from("/about"),
            profile_name: String::from("@jack"),
            profile_posts_min: 5,
            rss_content: String::from(r#"<rss xmlns\:atom"#),
            additional_hosts: vec![String::from("https://nitter.net")],
            additional_host_country: String::from("🇳🇱"),
            website_url: String::from(""),
            ping_range: chrono::Duration::hours(3),
            auto_mute: true,
            source_git_branch: String::from("master"),
            source_git_url: String::from("https://github.com/zedeus/nitter.git"),
            cleanup_interval: Duration::from_secs(24 * 60 * 60),
            error_retention_per_host: 100,
            connectivity_path: String::from("/"),
            git_scratch_folder: String::from("."),
        })
    }
}
