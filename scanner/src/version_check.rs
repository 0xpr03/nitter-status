// SPDX-License-Identifier: AGPL-3.0-only

use std::{collections::HashMap, path::Path};

use entities::state::{scanner::ScannerConfig, CommitInfo};
use git2::Repository;

use crate::Result;

pub struct VersionCheck {
    config: ScannerConfig,
    commit_cache: HashMap<String, CommitCacheValue>,
    current_epoch: u8,
    // TODO: this is ugly and unnecessary
    repository: Repository,
}

/// Internal cache entry
struct CommitCacheValue {
    result: CommitInfo,
    epoch: u8,
}

impl VersionCheck {
    pub(crate) fn new(config: ScannerConfig) -> Result<Self> {
        let temp_dir = Path::new(&config.git_scratch_folder).join(format!("nitter_version_clone"));
        tracing::debug!(git_check_dir=?temp_dir);
        let repository = Repository::init(temp_dir)?;
        let mut checker = VersionCheck {
            config,
            commit_cache: Default::default(),
            current_epoch: 0,
            repository,
        };

        checker.update_remote()?;
        Ok(checker)
    }
    /// Increases the epoch, invalidating unused cache entries and removes them.
    ///
    /// Blocking for large caches.
    pub fn cycle_epoch(&mut self) {
        self.current_epoch = self.current_epoch.wrapping_add(1);
        let curent_epoch = self.current_epoch;
        self.commit_cache
            .retain(|_key, entry| curent_epoch.wrapping_sub(entry.epoch) > 1);
    }
    /// Fetch remote, updating to new version, blocking
    pub(crate) fn update_remote(&mut self) -> Result<()> {
        self.cycle_epoch();
        let mut remote = match self.repository.find_remote(REMOTE_NAME) {
            Ok(remote) => {
                if remote.url() != Some(&self.config.source_git_url) {
                    tracing::warn!(found=?remote.url(),expected=self.config.source_git_url,"remote URL for {REMOTE_NAME} is unexpected");
                    let mut config = self.repository.config()?;
                    config.set_str(
                        &format!("remote.{REMOTE_NAME}.url"),
                        &self.config.source_git_url,
                    )?;
                }
                remote
            }
            Err(_) => self
                .repository
                .remote(REMOTE_NAME, &self.config.source_git_url)?,
        };

        remote.fetch(&["refs/heads/*:refs/heads/*"], None, None)?;
        Ok(())
    }

    /// Returns latest commit on main branch.
    pub(crate) fn latest_commit(&self) -> Result<String> {
        let main_branch = self.repository.find_reference(&format!(
            "refs/remotes/{REMOTE_NAME}/{}",
            self.config.source_git_branch
        ))?;
        let current_main_commit = main_branch.peel_to_commit()?;
        Ok(current_main_commit.id().to_string())
    }

    /// Check nitter git URL with SHA for its commit state.
    ///
    /// Example is `https://github.com/zedeus/nitter/commit/a92e79e`
    pub(crate) fn check_url(&mut self, url: &str) -> Result<CommitInfo> {
        match url.split('/').last() {
            Some(commit) => self.check_commit(commit),
            None => Ok(CommitInfo::UnknownCommit),
        }
    }

    pub(crate) fn check_commit(&mut self, commit_sha: &str) -> Result<CommitInfo> {
        if let Some(value) = self.commit_cache.get_mut(commit_sha) {
            if self.current_epoch.wrapping_sub(value.epoch) <= 1 {
                value.epoch = self.current_epoch;
                return Ok(value.result.clone());
            }
        }
        let result = self.check_commit_inner(commit_sha)?;
        self.commit_cache.insert(
            commit_sha.to_string(),
            CommitCacheValue {
                result: result.clone(),
                epoch: self.current_epoch,
            },
        );

        Ok(result)
    }

    fn check_commit_inner(&self, commit_sha: &str) -> Result<CommitInfo> {
        let commit = match self.repository.revparse_single(commit_sha) {
            Ok(commit) => commit,
            Err(_) => return Ok(CommitInfo::UnknownCommit),
        };

        let main_branch = self.repository.find_reference(&format!(
            "refs/remotes/{REMOTE_NAME}/{}",
            self.config.source_git_branch
        ))?;
        let current_main_commit = main_branch.peel_to_commit()?;

        if current_main_commit.id() == commit.id() {
            return Ok(CommitInfo::Current);
        }

        let mut revwalk = self.repository.revwalk()?;
        revwalk.push(main_branch.target().unwrap())?;

        let is_in_main_branch =
            revwalk.any(|parent| parent.map(|v| v == commit.id()).unwrap_or_default());

        match is_in_main_branch {
            true => Ok(CommitInfo::Outdated),
            false => Ok(CommitInfo::CustomBranch),
        }
    }
}

#[cfg(test)]
mod test {
    use entities::state::scanner::Config;

    use crate::version_check::{CommitInfo, VersionCheck};

    #[test]
    fn test_git_commit_exists() {
        let mut checker = VersionCheck::new(Config::test_defaults()).unwrap();
        assert_eq!(
            CommitInfo::Outdated,
            checker
                .check_commit("064ec8808022abb071f93f0fc976a8aa123699dc",)
                .unwrap(),
            "long old hash should be outdated"
        );
        assert_eq!(
            CommitInfo::Outdated,
            checker
                .check_url("https://github.com/zedeus/nitter/commit/51b5485",)
                .unwrap(),
            "old URL should be outdated"
        );
        assert_eq!(
            CommitInfo::UnknownCommit,
            checker
                .check_url("https://github.com/zedeus/nitter/commit/3295bdb",)
                .unwrap(),
            "URL for unknown commit should be unknown"
        );
        assert_eq!(
            CommitInfo::Outdated,
            checker.check_commit("064ec88",).unwrap(),
            "short old hash should be outdated"
        );
        assert_eq!(
            CommitInfo::UnknownCommit,
            checker
                .check_commit("064ec8808022abb071f93f0fc976b8aa123699dc",)
                .unwrap(),
            "long invalid hash should be Unknown"
        );
        // relies on https://github.com/zedeus/nitter/commits/tweets-parser/
        assert_eq!(
            CommitInfo::CustomBranch,
            checker
                .check_commit("c9b261a79303189f61ef5f5c6bf2c2600cdba792",)
                .unwrap(),
            "long invalid hash should be Unknown"
        );
        checker.update_remote().unwrap();
    }
}

static REMOTE_NAME: &str = "origin";
