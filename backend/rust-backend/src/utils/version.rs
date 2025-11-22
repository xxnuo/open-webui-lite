use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub published_at: Option<String>,
    pub html_url: String,
    pub prerelease: bool,
    pub draft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
}

/// Check for updates from GitHub releases
#[allow(dead_code)]
pub async fn check_for_updates(current_version: &str) -> AppResult<VersionInfo> {
    let client = Client::builder()
        .user_agent("open-webui-rust")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Fetch latest release from GitHub API
    let url = "https://api.github.com/repos/knoxchat/open-webui-rust/releases/latest";

    let response = client.get(url).send().await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            let release: GitHubRelease = resp.json().await?;

            // Remove 'v' prefix from tag_name if present
            let latest_version = release.tag_name.trim_start_matches('v');
            let current = current_version.trim_start_matches('v');

            let update_available = is_newer_version(current, latest_version);

            info!(
                "Version check: current={}, latest={}, update_available={}",
                current, latest_version, update_available
            );

            Ok(VersionInfo {
                current: current.to_string(),
                latest: latest_version.to_string(),
                update_available,
                release_notes: release.body,
                release_url: Some(release.html_url),
            })
        }
        Ok(resp) => {
            let status = resp.status();
            let error_text = resp.text().await.unwrap_or_default();
            error!("GitHub API error: {} - {}", status, error_text);

            // Return current version as both current and latest if check fails
            Ok(VersionInfo {
                current: current_version.to_string(),
                latest: current_version.to_string(),
                update_available: false,
                release_notes: None,
                release_url: None,
            })
        }
        Err(e) => {
            error!("Failed to check for updates: {}", e);

            // Return current version as both current and latest if check fails
            Ok(VersionInfo {
                current: current_version.to_string(),
                latest: current_version.to_string(),
                update_available: false,
                release_notes: None,
                release_url: None,
            })
        }
    }
}

/// Compare two semantic versions
/// Returns true if `latest` is newer than `current`
#[allow(dead_code)]
fn is_newer_version(current: &str, latest: &str) -> bool {
    let current_parts: Vec<u32> = current.split('.').filter_map(|s| s.parse().ok()).collect();

    let latest_parts: Vec<u32> = latest.split('.').filter_map(|s| s.parse().ok()).collect();

    // Compare major.minor.patch
    for i in 0..3 {
        let curr = current_parts.get(i).copied().unwrap_or(0);
        let late = latest_parts.get(i).copied().unwrap_or(0);

        if late > curr {
            return true;
        } else if late < curr {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("0.6.30", "0.6.31"));
        assert!(is_newer_version("0.6.30", "0.7.0"));
        assert!(is_newer_version("0.6.30", "1.0.0"));
        assert!(!is_newer_version("0.6.31", "0.6.30"));
        assert!(!is_newer_version("0.7.0", "0.6.31"));
        assert!(!is_newer_version("0.6.30", "0.6.30"));
    }

    #[test]
    fn test_version_stripping() {
        let v1 = "v0.6.30";
        let v2 = "0.6.30";

        assert_eq!(v1.trim_start_matches('v'), v2);
        assert!(!is_newer_version(
            v1.trim_start_matches('v'),
            v2.trim_start_matches('v')
        ));
    }
}
