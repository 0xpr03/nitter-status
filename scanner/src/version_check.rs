use entities::state::scanner::ScannerConfig;
use git2::{Direction, Remote};

use crate::{Result, ScannerError};

pub(crate) struct CurrentVersion {
    pub version: String,
    config: ScannerConfig,
}

const SHORT_COMMIT_LEN: usize = 7;

impl CurrentVersion {
    /// Whether a nitter version URL is from the same repo
    pub fn is_same_repo(&self, url: &str) -> bool {
        url.starts_with(self.config.source_git_url.trim_end_matches(".git"))
    }
    /// Whether a nitter version URL is pointing to the same version as this one
    pub fn is_same_version(&self, url: &str) -> bool {
        // look for last path segment (don't parse) and look if that matches
        // from the start
        self.is_same_repo(url)
            && match url.split('/').last() {
                Some(other_version) => {
                    if other_version.len() == self.version.len() {
                        // we get the long version
                        other_version.starts_with(&self.version)
                    } else if other_version.len() == SHORT_COMMIT_LEN {
                        other_version.starts_with(&self.version[..SHORT_COMMIT_LEN])
                    } else {
                        // everything else is no short commit id..
                        false
                    }
                }
                None => false,
            }
    }
}

pub(crate) fn fetch_git_state(config: ScannerConfig) -> Result<CurrentVersion> {
    let mut remote = Remote::create_detached(config.source_git_url.as_str())?;

    remote.connect(Direction::Fetch)?;

    let reference = format!("refs/heads/{}", config.source_git_branch);
    let commit = remote
        .list()?
        .into_iter()
        .find(|v| v.name() == &reference)
        .map(|v| v.oid().to_string());

    remote.disconnect()?;

    Ok(commit
        .map(|commit| CurrentVersion {
            version: commit,
            config,
        })
        .ok_or(ScannerError::GitBranch)?)
}

#[cfg(test)]
mod test {
    use entities::state::scanner::Config;

    use super::fetch_git_state;

    #[test]
    fn test_git() {
        fetch_git_state(Config::test_defaults()).unwrap();
    }
}
