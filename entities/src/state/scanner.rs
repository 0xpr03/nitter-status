// SPDX-License-Identifier: AGPL-3.0-only
use std::{sync::Arc, time::Duration};
pub type ScannerConfig = Arc<Config>;
pub struct Config {
    /// time until next instance list fetch
    pub list_fetch_interval: Duration,
    /// time until next instance ping check
    pub instance_check_interval: Duration,
    /// instances list URL
    pub instance_list_url: String,
    /// profile path for health check
    pub profile_path: String,
    /// rss path for health check
    pub rss_path: String,
    /// about path for version check
    pub about_path: String,
    /// Expected string for a valid profile health check
    pub profile_content: String,
    /// Expected string for a valid RSS health check
    pub rss_content: String,
    /// List of additional hosts to include during health checks
    pub additional_hosts: Vec<String>,
    /// Referer to use
    pub referrer: String,
    /// Duration to average the ping/response times over
    pub ping_range: chrono::Duration,
    /// don't emit errors for hosts which are already listed as down
    pub auto_mute: bool,
    /// Git URL for source fetching
    pub source_git_url: String,
    /// Git branch to fetch the current commit from
    pub source_git_branch: String,
    /// Hosts known to be bad (ip block), domain per entry
    pub bad_hosts: Vec<String>,
}

impl Config {
    pub fn test_defaults() -> ScannerConfig {
        Arc::new(Config {
            list_fetch_interval: Duration::from_secs(60 * 5),
            instance_check_interval: Duration::from_secs(60 * 5),
            instance_list_url: String::from("https://github.com/zedeus/nitter/wiki/Instances"),
            profile_path: String::from("/jack"),
            rss_path: String::from("/jack/rss"),
            about_path: String::from("/about"),
            profile_content: String::from(r#"jack.?\(@jack\)"#),
            rss_content: String::from(r#"<rss xmlns\:atom"#),
            additional_hosts: vec![String::from("https://nitter.net")],
            referrer: String::from(""),
            ping_range: chrono::Duration::hours(3),
            auto_mute: true,
            source_git_branch: String::from("master"),
            source_git_url: String::from("https://github.com/zedeus/nitter.git"),
            bad_hosts: vec![String::from("tweet.whateveritworks.org")],
        })
    }
}
