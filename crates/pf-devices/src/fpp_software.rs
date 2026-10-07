//! An FPP's software (read-only) and whether FPP has a newer release for it.
//!
//! What's running comes from `/api/system/info` (`common.php` `GetSystemInfoJsonInternal()` in
//! FPP 9.5.3): `Version`, `OSVersion` (the OS image build), `OSRelease`, `Platform`, `Variant`,
//! and `Kernel`. What's available comes from the list FPP's own Upgrade OS menu uses
//! (`about.php` calls `api/git/releases/os`, which `git.php` builds from the public release list
//! of FalconChristmas/fpp, keeping each release's `*.fppos` files). PixelFlow reads that same
//! public list itself, unauthenticated, and never upgrades anything: the user does that on FPP's
//! own About page.

use crate::error::DeviceError;
use crate::fpp::{get_json, str_field};
use crate::http::Http;
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The software an FPP reports about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FppSoftware {
    /// FPP's version, e.g. `9.3` (a development build reads like `9.3-4-g97360ca2`).
    pub version: String,
    /// The OS image it was installed from, e.g. `v2025-11`.
    pub os_build: String,
    /// The operating system, e.g. `Raspbian GNU/Linux 12 (bookworm)`.
    pub os_release: String,
    /// The board, e.g. `Pi 3 Model B+` (or the platform when FPP gives no board).
    pub platform: String,
    /// 32 or 64 when the kernel says which.
    pub bits: Option<u8>,
    /// The file-name prefix of this box's OS images (`Pi-`, `Pi64-`, `BBB-`, `BB64-`), when known.
    pub image_prefix: Option<String>,
}

/// The kernel's word size: a `-v8` or `aarch64`/`arm64` kernel is 64-bit; `-v7`, `-v6`, or `armv`
/// is 32-bit.
fn bits_of(kernel: &str) -> Option<u8> {
    let k = kernel.to_ascii_lowercase();
    if k.contains("aarch64") || k.contains("arm64") || k.contains("-v8") {
        Some(64)
    } else if k.contains("-v7") || k.contains("-v6") || k.contains("armv") {
        Some(32)
    } else {
        None
    }
}

fn image_prefix(platform: &str, bits: Option<u8>) -> Option<&'static str> {
    let platform = platform.to_ascii_lowercase();
    match (platform.contains("raspberry"), platform.contains("beagle"), bits?) {
        (true, _, 32) => Some("Pi-"),
        (true, _, _) => Some("Pi64-"),
        (_, true, 32) => Some("BBB-"),
        (_, true, _) => Some("BB64-"),
        _ => None,
    }
}

/// Reads what the FPP is running (changes nothing; only `/api/system/info`).
pub fn software(http: &dyn Http, host: &str) -> Result<FppSoftware, DeviceError> {
    let info = get_json(http, host, "/api/system/info")?;
    let text = |key| str_field(&info, key).trim().to_string();
    let platform_name = text("Platform");
    let bits = bits_of(str_field(&info, "Kernel"));
    let prefix = image_prefix(&platform_name, bits).map(String::from);
    let variant = text("Variant");
    Ok(FppSoftware {
        version: text("Version"),
        os_build: text("OSVersion"),
        os_release: text("OSRelease"),
        platform: if variant.is_empty() {
            platform_name
        } else {
            variant
        },
        bits,
        image_prefix: prefix,
    })
}

/// One OS image of a stable FPP release, e.g. `Pi-10.2_2026-10.fppos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsImage {
    pub file: String,
    /// `Pi-`, `Pi64-`, `BBB-`, `BB64-`.
    pub prefix: String,
    /// The FPP version in the file name, as numbers (`[10, 2]`).
    pub version: Vec<u32>,
    pub version_text: String,
}

/// A version's leading dotted numbers: `9.3-4-g97360ca2` is `[9, 3]`; nightly reads as none.
fn version_numbers(text: &str) -> Vec<u32> {
    let lead: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    lead.split('.').map_while(|p| p.parse().ok()).collect()
}

/// The stable OS images in the release list. Drafts, pre-releases (nightly, beta), and files that
/// aren't `<prefix>-<version>_<build>.fppos` are left out.
pub fn parse_releases(json: &str) -> Vec<OsImage> {
    let Ok(Value::Array(releases)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let flag = |r: &Value, key: &str| r.get(key).and_then(Value::as_bool).unwrap_or(false);
    let mut images = Vec::new();
    for release in releases
        .iter()
        .filter(|r| !flag(r, "draft") && !flag(r, "prerelease"))
    {
        for asset in release
            .get("assets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let file = str_field(asset, "name");
            let Some(stem) = file.strip_suffix(".fppos") else {
                continue;
            };
            let Some((prefix, rest)) = stem.split_once('-') else {
                continue;
            };
            let version_text = rest.split('_').next().unwrap_or("");
            let version = version_numbers(version_text);
            // "10.0-beta5" and "nightly" aren't releases to recommend.
            if version.is_empty() || version_text.chars().any(|c| !(c.is_ascii_digit() || c == '.')) {
                continue;
            }
            images.push(OsImage {
                file: file.to_string(),
                prefix: format!("{prefix}-"),
                version,
                version_text: version_text.to_string(),
            });
        }
    }
    images
}

/// A newer FPP release that fits this box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotice {
    /// e.g. `10.2`.
    pub version: String,
    /// The exact file to choose in FPP's Upgrade OS list.
    pub file: String,
    pub prefix: String,
    /// A new major version: back up first.
    pub major: bool,
}

fn newer(a: &[u32], b: &[u32]) -> bool {
    let len = a.len().max(b.len());
    let pad = |v: &[u32]| {
        (0..len)
            .map(|i| v.get(i).copied().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    pad(a) > pad(b)
}

/// The newest stable image for this box that's newer than what it runs. `None` when it's up to
/// date, or when the box's type isn't known well enough to pick a file.
pub fn recommend(software: &FppSoftware, images: &[OsImage]) -> Option<UpdateNotice> {
    let prefix = software.image_prefix.as_deref()?;
    let current = version_numbers(&software.version);
    if current.is_empty() {
        return None;
    }
    let best = images
        .iter()
        .filter(|i| i.prefix == prefix && newer(&i.version, &current))
        .fold(None::<&OsImage>, |best, i| match best {
            Some(b) if !newer(&i.version, &b.version) => Some(b),
            _ => Some(i),
        })?;
    Some(UpdateNotice {
        version: best.version_text.clone(),
        file: best.file.clone(),
        prefix: best.prefix.clone(),
        major: best.version.first() != current.first(),
    })
}

/// Where the release list comes from (the public list online; recorded in tests).
pub trait ReleaseSource: Send + Sync {
    /// The release list as text.
    fn fetch(&self) -> Result<String, String>;
}

/// The public release list for FPP, unauthenticated, with a time limit.
pub struct OnlineReleases {
    agent: ureq::Agent,
}

impl OnlineReleases {
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        Self { agent: config.into() }
    }
}

impl ReleaseSource for OnlineReleases {
    fn fetch(&self) -> Result<String, String> {
        let mut response = self
            .agent
            .get("https://api.github.com/repos/FalconChristmas/fpp/releases?per_page=100")
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "PixelFlow")
            .call()
            .map_err(|e| e.to_string())?;
        if response.status().as_u16() != 200 {
            return Err(format!(
                "the release list answered HTTP {}",
                response.status().as_u16()
            ));
        }
        response
            .body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| e.to_string())
    }
}

type Fetched = Option<Arc<Vec<OsImage>>>;

/// The release list, fetched at most once per `ttl` (and a failed fetch isn't retried for
/// `retry`, so a rate limit isn't hammered).
pub struct ReleaseCache {
    source: Arc<dyn ReleaseSource>,
    ttl: Duration,
    retry: Duration,
    state: Mutex<Option<(Instant, Fetched)>>,
}

impl ReleaseCache {
    pub fn new(source: Arc<dyn ReleaseSource>, ttl: Duration, retry: Duration) -> Self {
        Self {
            source,
            ttl,
            retry,
            state: Mutex::new(None),
        }
    }

    /// The real thing: kept 6 hours, retried after 15 minutes when it failed.
    pub fn online() -> Self {
        Self::new(
            Arc::new(OnlineReleases::new(Duration::from_secs(8))),
            Duration::from_secs(6 * 3600),
            Duration::from_secs(15 * 60),
        )
    }

    /// The stable images, or `None` when the list couldn't be read.
    pub fn images(&self) -> Fetched {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, images)) = state.as_ref() {
            let keep = if images.is_some() { self.ttl } else { self.retry };
            if at.elapsed() < keep {
                return images.clone();
            }
        }
        let images = self
            .source
            .fetch()
            .ok()
            .map(|body| parse_releases(&body))
            .filter(|images| !images.is_empty())
            .map(Arc::new);
        *state = Some((Instant::now(), images.clone()));
        images
    }
}

/// What the Software section shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareReport {
    #[serde(flatten)]
    pub software: FppSoftware,
    /// A newer release for this box, if there is one.
    pub update: Option<UpdateNotice>,
    /// Whether the release list could be read (when not, the page says so only in a tooltip).
    pub checked: bool,
}

/// The FPP's software and any newer release (changes nothing).
pub fn report(http: &dyn Http, host: &str, releases: &ReleaseCache) -> Result<SoftwareReport, DeviceError> {
    let software = software(http, host)?;
    let images = releases.images();
    Ok(SoftwareReport {
        update: images.as_ref().and_then(|images| recommend(&software, images)),
        checked: images.is_some(),
        software,
    })
}

/// A recorded release list, for tests (here and in the app).
#[cfg(feature = "test-fixtures")]
pub struct RecordedReleases(pub Option<&'static str>);

#[cfg(feature = "test-fixtures")]
impl ReleaseSource for RecordedReleases {
    fn fetch(&self) -> Result<String, String> {
        self.0.map(String::from).ok_or_else(|| "rate limited".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::FakeHttp;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const RELEASES: &str = crate::testing::FPP_RELEASES;
    const HOST: &str = "192.0.2.10";

    fn info(version: &str, platform: &str, kernel: &str) -> FakeHttp {
        FakeHttp::new().with_get(
            HOST,
            "/api/system/info",
            &format!(
                r#"{{"Version":"{version}","Platform":"{platform}","Variant":"Pi 4","OSVersion":"v2025-11",
                "OSRelease":"Raspbian GNU/Linux 12 (bookworm)","Kernel":"{kernel}"}}"#
            ),
        )
    }

    struct Counting(AtomicUsize, Option<String>);
    impl ReleaseSource for Counting {
        fn fetch(&self) -> Result<String, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            self.1.clone().ok_or_else(|| "rate limited".to_string())
        }
    }

    #[test]
    fn the_kernel_says_32_or_64_bit() {
        assert_eq!(bits_of("6.12.57-v7+"), Some(32));
        assert_eq!(bits_of("6.1.21-v7l+"), Some(32));
        assert_eq!(bits_of("6.12.47+rpt-rpi-v8"), Some(64));
        assert_eq!(bits_of("6.6.31-v8+"), Some(64));
        assert_eq!(bits_of("5.15.0 aarch64"), Some(64));
        assert_eq!(bits_of("6.1.0-amd64"), None);
        assert_eq!(bits_of(""), None);
    }

    #[test]
    fn reads_what_the_fpp_runs() {
        let s = software(&info("9.5.3", "Raspberry Pi", "6.12.57-v7+"), HOST).unwrap();
        assert_eq!(s.version, "9.5.3");
        assert_eq!(s.os_build, "v2025-11");
        assert_eq!(s.platform, "Pi 4");
        assert_eq!(s.bits, Some(32));
        assert_eq!(s.image_prefix.as_deref(), Some("Pi-"));
        let s64 = software(&info("9.5.3", "Raspberry Pi", "6.6.31-v8+"), HOST).unwrap();
        assert_eq!(s64.image_prefix.as_deref(), Some("Pi64-"));
    }

    #[test]
    fn only_stable_images_are_kept() {
        let images = parse_releases(RELEASES);
        assert!(images.iter().any(|i| i.file == "Pi-10.2_2026-10.fppos"));
        assert!(images.iter().any(|i| i.file == "Pi64-10.2_2026-10.fppos"));
        assert!(images.iter().any(|i| i.file == "Pi-9.5.3_2025-11.fppos"));
        assert!(
            images
                .iter()
                .all(|i| !i.file.contains("nightly") && !i.file.contains("beta"))
        );
        assert!(parse_releases("not json").is_empty());
    }

    #[test]
    fn a_newer_point_release_is_not_a_major_upgrade() {
        let s = software(&info("9.4", "Raspberry Pi", "6.12.57-v7+"), HOST).unwrap();
        let images: Vec<OsImage> = parse_releases(RELEASES)
            .into_iter()
            .filter(|i| i.version[0] == 9)
            .collect();
        let notice = recommend(&s, &images).unwrap();
        assert_eq!(notice.file, "Pi-9.5.3_2025-11.fppos");
        assert_eq!(notice.version, "9.5.3");
        assert!(!notice.major);
    }

    #[test]
    fn a_new_major_version_is_flagged_and_matches_the_box() {
        let s = software(&info("9.5.3", "Raspberry Pi", "6.12.57-v7+"), HOST).unwrap();
        let notice = recommend(&s, &parse_releases(RELEASES)).unwrap();
        assert_eq!(notice.file, "Pi-10.2_2026-10.fppos");
        assert!(notice.major);
        let s64 = software(&info("9.5.3", "Raspberry Pi", "6.6.31-v8+"), HOST).unwrap();
        assert_eq!(
            recommend(&s64, &parse_releases(RELEASES)).unwrap().file,
            "Pi64-10.2_2026-10.fppos"
        );
    }

    #[test]
    fn nothing_when_up_to_date_ahead_or_unsure() {
        let images = parse_releases(RELEASES);
        let current = software(&info("10.2", "Raspberry Pi", "6.12.57-v7+"), HOST).unwrap();
        assert_eq!(recommend(&current, &images), None);
        let dev = software(&info("10.2-4-g97360ca2", "Raspberry Pi", "6.12.57-v7+"), HOST).unwrap();
        assert_eq!(recommend(&dev, &images), None);
        let ahead = software(&info("11.0", "Raspberry Pi", "6.12.57-v7+"), HOST).unwrap();
        assert_eq!(recommend(&ahead, &images), None);
        let unsure_bits = software(&info("9.5", "Raspberry Pi", "6.12.57"), HOST).unwrap();
        assert_eq!(recommend(&unsure_bits, &images), None);
        let other = software(&info("9.5", "Debian", "6.1.0-amd64"), HOST).unwrap();
        assert_eq!(recommend(&other, &images), None);
    }

    #[test]
    fn the_release_list_is_fetched_once_per_ttl_and_failures_are_not_hammered() {
        let source = Arc::new(Counting(AtomicUsize::new(0), Some(RELEASES.to_string())));
        let cache = ReleaseCache::new(source.clone(), Duration::from_secs(3600), Duration::from_secs(60));
        assert!(cache.images().is_some() && cache.images().is_some());
        assert_eq!(source.0.load(Ordering::SeqCst), 1);

        let expired = ReleaseCache::new(source.clone(), Duration::ZERO, Duration::ZERO);
        expired.images();
        expired.images();
        assert_eq!(source.0.load(Ordering::SeqCst), 3);

        let down = Arc::new(Counting(AtomicUsize::new(0), None));
        let cache = ReleaseCache::new(down.clone(), Duration::from_secs(3600), Duration::from_secs(60));
        assert!(cache.images().is_none() && cache.images().is_none());
        assert_eq!(down.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn when_the_list_is_down_the_report_has_no_notice_and_says_unchecked() {
        let down = ReleaseCache::new(
            Arc::new(Counting(AtomicUsize::new(0), None)),
            Duration::ZERO,
            Duration::ZERO,
        );
        let http = info("9.5.3", "Raspberry Pi", "6.12.57-v7+");
        let report = report(&http, HOST, &down).unwrap();
        assert!(!report.checked);
        assert_eq!(report.update, None);
        assert_eq!(report.software.version, "9.5.3");
        // Only the approved info endpoint was read.
        assert_eq!(http.requests(), vec![format!("GET {HOST}/api/system/info")]);
    }
}
