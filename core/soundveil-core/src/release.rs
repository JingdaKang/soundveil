//! Release-tag versions and GitHub's latest-release response.

use serde::Deserialize;
use std::fmt;

/// Numeric major.minor.patch, optionally written as a "v"-prefixed release tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    parts: [i64; 3],
}

impl Version {
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('v').unwrap_or(text);
        let fields: Vec<&str> = digits.split('.').collect();
        let [major, minor, patch] = fields[..] else {
            return None;
        };
        let number = |field: &str| {
            let plain = !field.is_empty() && field.bytes().all(|b| b.is_ascii_digit());
            plain.then(|| field.parse().ok()).flatten()
        };
        Some(Self {
            parts: [number(major)?, number(minor)?, number(patch)?],
        })
    }

    pub fn parts(&self) -> [i64; 3] {
        self.parts
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.parts;
        write!(f, "{major}.{minor}.{patch}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub page: String,
    pub download: Option<String>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct Payload {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

fn is_https(url: &str) -> bool {
    url.starts_with("https://")
}

impl Release {
    /// Reads GitHub's latest-release response. Drafts, prereleases, and non-HTTPS links are never
    /// offered. `installer` is the file ending of this platform's download, such as ".dmg".
    pub fn from_github(data: &[u8], installer: &str) -> Option<Self> {
        let payload: Payload = serde_json::from_slice(data).ok()?;
        if payload.draft || payload.prerelease || !is_https(&payload.html_url) {
            return None;
        }
        Some(Self {
            version: Version::parse(&payload.tag_name)?,
            download: payload
                .assets
                .into_iter()
                .find(|a| a.name.ends_with(installer) && is_https(&a.browser_download_url))
                .map(|a| a.browser_download_url),
            page: payload.html_url,
        })
    }

    pub fn is_newer_than(&self, current: &str) -> bool {
        Version::parse(current).is_some_and(|version| version < self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASSETS: &str = r#"[{"name":"Soundveil-0.2.0-arm64.dmg.sha256","browser_download_url":"https://github.com/JingdaKang/soundveil/releases/download/v0.2.0/Soundveil-0.2.0-arm64.dmg.sha256"},{"name":"Soundveil-0.2.0-arm64.dmg","browser_download_url":"https://github.com/JingdaKang/soundveil/releases/download/v0.2.0/Soundveil-0.2.0-arm64.dmg"}]"#;

    fn payload(tag: &str, draft: bool, prerelease: bool, assets: &str) -> Vec<u8> {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/JingdaKang/soundveil/releases/tag/{tag}","draft":{draft},"prerelease":{prerelease},"assets":{assets},"body":"notes"}}"#
        )
        .into_bytes()
    }
    fn latest() -> Vec<u8> {
        payload("v0.2.0", false, false, ASSETS)
    }

    #[test]
    fn version_parsing() {
        assert_eq!(Version::parse("v1.2.3").unwrap().parts(), [1, 2, 3]);
        assert_eq!(Version::parse("0.10.0").unwrap().to_string(), "0.10.0");
        for bad in [
            "",
            "1.2",
            "1.2.3.4",
            "1..3",
            "v1.2.x",
            "+1.2.3",
            "1.2.3-beta",
            "99999999999999999999.0.0",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn numeric_ordering() {
        let version = |text| Version::parse(text).unwrap();
        assert!(version("0.9.0") < version("0.10.0"));
        assert!(version("0.1.9") < version("0.2.0"));
        assert_eq!(version("v1.0.0"), version("1.0.0"));
    }

    #[test]
    fn reads_latest_release() {
        let release = Release::from_github(&latest(), ".dmg").unwrap();
        assert_eq!(release.version.to_string(), "0.2.0");
        assert!(
            release
                .download
                .unwrap()
                .ends_with("/Soundveil-0.2.0-arm64.dmg")
        );
        assert_eq!(
            release.page,
            "https://github.com/JingdaKang/soundveil/releases/tag/v0.2.0"
        );
    }

    #[test]
    fn missing_installer_falls_back_to_page() {
        let data = payload("v0.2.0", false, false, "[]");
        assert_eq!(Release::from_github(&data, ".dmg").unwrap().download, None);
        assert_eq!(
            Release::from_github(&latest(), ".msi").unwrap().download,
            None
        );
    }

    #[test]
    fn ignores_unpublished_and_malformed() {
        let read = |data: Vec<u8>| Release::from_github(&data, ".dmg");
        assert_eq!(read(payload("v0.2.0", true, false, ASSETS)), None);
        assert_eq!(read(payload("v0.2.0", false, true, ASSETS)), None);
        assert_eq!(read(payload("nightly", false, false, ASSETS)), None);
        assert_eq!(read(br#"{"message":"Not Found"}"#.to_vec()), None);
    }

    #[test]
    fn only_newer_versions_are_offered() {
        let release = Release::from_github(&latest(), ".dmg").unwrap();
        assert!(release.is_newer_than("0.1.0"));
        assert!(!release.is_newer_than("0.2.0"));
        assert!(!release.is_newer_than("0.10.0"));
        assert!(!release.is_newer_than("unknown"));
    }
}
