use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use url::Url;

pub(crate) const CHECK_FILE: &str = "update-check.yaml";
#[cfg(feature = "desktop")]
pub(crate) const API_URL: &str = "https://api.github.com/repos/pigjin/rust-rdm/releases/latest";
const RELEASES_URL: &str = "https://github.com/pigjin/rust-rdm/releases";
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Download {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    pub downloads: Vec<Download>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CheckError {
    Network,
    RateLimited,
    NoRelease,
    InvalidResponse,
}

pub(crate) type CheckResult = Result<Option<Release>, CheckError>;

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct CheckRecord {
    pub last_check_at: Option<i64>,
}

impl CheckRecord {
    pub fn due(&self, now: i64) -> bool {
        self.last_check_at
            .is_none_or(|last| last <= 0 || now < last || now.saturating_sub(last) >= 24 * 60 * 60)
    }
}

#[derive(Default)]
pub(crate) struct Checker {
    pub running: bool,
    manual: bool,
    cached: Option<(Instant, CheckResult)>,
}

pub(crate) enum Start {
    Request,
    Pending,
    Cached(CheckResult),
}

impl Checker {
    pub fn start(&mut self, manual: bool, now: Instant) -> Start {
        if self.running {
            self.manual |= manual;
            return Start::Pending;
        }
        if let Some((at, result)) = &self.cached
            && now.saturating_duration_since(*at) < Duration::from_secs(60)
        {
            return Start::Cached(result.clone());
        }
        self.running = true;
        self.manual = manual;
        Start::Request
    }

    pub fn finish(&mut self, result: CheckResult, now: Instant) -> bool {
        self.running = false;
        self.cached = Some((now, result));
        std::mem::take(&mut self.manual)
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    html_url: String,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn release_url(tag: &str, asset: Option<&str>) -> Url {
    let mut url = Url::parse(RELEASES_URL).unwrap();
    let mut path = url.path_segments_mut().unwrap();
    path.extend([if asset.is_some() { "download" } else { "tag" }, tag]);
    if let Some(asset) = asset {
        path.push(asset);
    }
    drop(path);
    url
}

fn checked_url(value: &str, expected: Url) -> Result<String, CheckError> {
    // A build metadata '+' can also be percent-encoded in GitHub release paths.
    let value = value.replace("%2B", "+").replace("%2b", "+");
    let url = Url::parse(&value).map_err(|_| CheckError::InvalidResponse)?;
    if url != expected {
        return Err(CheckError::InvalidResponse);
    }
    Ok(url.into())
}

pub(crate) fn parse_release(body: &[u8], current: &str, os: &str, arch: &str) -> CheckResult {
    if body.len() > MAX_RESPONSE {
        return Err(CheckError::InvalidResponse);
    }
    let raw: GithubRelease =
        serde_json::from_slice(body).map_err(|_| CheckError::InvalidResponse)?;
    let version_text = raw.tag_name.strip_prefix('v').unwrap_or(&raw.tag_name);
    let version = Version::parse(version_text).map_err(|_| CheckError::InvalidResponse)?;
    let current = Version::parse(current).map_err(|_| CheckError::InvalidResponse)?;
    if raw.draft || raw.prerelease || !version.pre.is_empty() {
        return Err(CheckError::InvalidResponse);
    }
    if !version.cmp_precedence(&current).is_gt() {
        return Ok(None);
    }
    let page_url = checked_url(&raw.html_url, release_url(&raw.tag_name, None))?;
    let arch = if arch == "aarch64" { "arm64" } else { arch };
    let extensions: &[(&str, &str)] = match (os, arch) {
        (_, "x86_64" | "arm64") => match os {
            "macos" => &[("DMG", ".dmg")],
            "windows" => &[("EXE", "-setup.exe")],
            "linux" => &[("AppImage", ".AppImage"), ("DEB", ".deb")],
            _ => &[],
        },
        _ => &[],
    };
    let mut downloads = Vec::new();
    for &(label, extension) in extensions {
        let name = format!("RedisDock-{version_text}-{os}-{arch}{extension}");
        let mut assets = raw.assets.iter().filter(|asset| asset.name == name);
        if let Some(asset) = assets.next() {
            if assets.next().is_some() {
                return Err(CheckError::InvalidResponse);
            }
            downloads.push(Download {
                label: label.into(),
                url: checked_url(
                    &asset.browser_download_url,
                    release_url(&raw.tag_name, Some(&name)),
                )?,
            });
        }
    }
    Ok(Some(Release {
        version: version_text.into(),
        notes: raw.body.unwrap_or_default(),
        page_url,
        downloads,
    }))
}

#[cfg(feature = "desktop")]
pub(crate) async fn fetch(client: &dyn http_client::HttpClient) -> CheckResult {
    use futures::AsyncReadExt;
    use http_client::{AsyncBody, HttpRequestExt, Request};

    let request = Request::get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("RedisDock/", env!("CARGO_PKG_VERSION")),
        )
        .timeout(Duration::from_secs(10))
        .body(AsyncBody::default())
        .map_err(|_| CheckError::InvalidResponse)?;
    let mut response = client
        .send(request)
        .await
        .map_err(|_| CheckError::Network)?;
    match response.status().as_u16() {
        200 => {}
        404 => return Err(CheckError::NoRelease),
        403 | 429 => return Err(CheckError::RateLimited),
        500..=599 => return Err(CheckError::Network),
        _ => return Err(CheckError::InvalidResponse),
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .take(MAX_RESPONSE as u64 + 1)
        .read_to_end(&mut body)
        .await
        .map_err(|_| CheckError::Network)?;
    parse_release(
        &body,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn release(version: &str) -> Value {
        let tag = format!("v{version}");
        let mut assets = Vec::new();
        for os in ["macos", "windows", "linux"] {
            for arch in ["x86_64", "arm64"] {
                for extension in match os {
                    "macos" => vec![".dmg"],
                    "windows" => vec!["-setup.exe"],
                    _ => vec![".AppImage", ".deb"],
                } {
                    let name = format!("RedisDock-{version}-{os}-{arch}{extension}");
                    assets.push(json!({"name": name, "browser_download_url": release_url(&tag, Some(&name)).as_str()}));
                }
            }
        }
        json!({"tag_name":tag,"draft":false,"prerelease":false,
            "html_url":release_url(&tag, None).as_str(),"body":"# Changes\n<plain text>","assets":assets})
    }

    fn parse(value: &Value, current: &str, os: &str, arch: &str) -> CheckResult {
        parse_release(&serde_json::to_vec(value).unwrap(), current, os, arch)
    }

    #[test]
    fn compares_version_precedence_and_rejects_unstable_or_invalid_tags() {
        for (new, current, newer) in [
            ("0.1.10", "0.1.9", true),
            ("0.1.9", "0.1.10", false),
            ("0.1.5", "0.1.5", false),
            ("0.1.5+new", "0.1.5+old", false),
            ("0.1.5", "0.1.5-rc.1", true),
            ("0.1.6+build.2", "0.1.5", true),
        ] {
            assert_eq!(
                parse(&release(new), current, "macos", "aarch64")
                    .unwrap()
                    .is_some(),
                newer
            );
        }
        for tag in ["v1.0.0-rc.1", "latest", "v1.2", "v01.2.3", "v1.2.3/evil"] {
            let mut value = release("1.0.0");
            value["tag_name"] = tag.into();
            assert_eq!(
                parse(&value, "0.1.5", "macos", "aarch64"),
                Err(CheckError::InvalidResponse)
            );
        }
        for field in ["draft", "prerelease"] {
            let mut value = release("1.0.0");
            value[field] = true.into();
            assert_eq!(
                parse(&value, "0.1.5", "macos", "aarch64"),
                Err(CheckError::InvalidResponse)
            );
        }
    }

    #[test]
    fn matches_only_the_installed_platform_and_available_packages() {
        let value = release("1.0.0");
        for os in ["macos", "windows", "linux"] {
            for arch in ["x86_64", "arm64", "aarch64"] {
                let info = parse(&value, "0.1.5", os, arch).unwrap().unwrap();
                assert_eq!(info.downloads.len(), if os == "linux" { 2 } else { 1 });
                for download in info.downloads {
                    assert!(download.url.contains(&format!(
                        "-{os}-{}",
                        if arch == "aarch64" { "arm64" } else { arch }
                    )));
                }
                assert_eq!(info.notes, "# Changes\n<plain text>");
            }
        }
        for (os, arch) in [("freebsd", "x86_64"), ("linux", "riscv64")] {
            assert!(
                parse(&value, "0.1.5", os, arch)
                    .unwrap()
                    .unwrap()
                    .downloads
                    .is_empty()
            );
        }
        let mut missing = value;
        missing["assets"] = json!([]);
        missing["body"] = Value::Null;
        let info = parse(&missing, "0.1.5", "linux", "x86_64")
            .unwrap()
            .unwrap();
        assert!(info.downloads.is_empty());
        assert!(info.notes.is_empty());
    }

    #[test]
    fn accepts_percent_encoded_build_metadata_in_release_links() {
        let mut value = release("1.0.0+build.2");
        value["html_url"] = value["html_url"]
            .as_str()
            .unwrap()
            .replace('+', "%2B")
            .into();
        for asset in value["assets"].as_array_mut().unwrap() {
            asset["browser_download_url"] = asset["browser_download_url"]
                .as_str()
                .unwrap()
                .replace('+', "%2b")
                .into();
        }
        let info = parse(&value, "0.1.5", "macos", "aarch64").unwrap().unwrap();
        assert_eq!(info.version, "1.0.0+build.2");
        assert_eq!(info.downloads.len(), 1);
    }

    #[test]
    fn rejects_untrusted_links_duplicate_assets_and_bad_json() {
        for bad in [
            "http://github.com/pigjin/rust-rdm/releases/tag/v1.0.0",
            "https://github.com.evil.test/pigjin/rust-rdm/releases/tag/v1.0.0",
            "https://evil@github.com/pigjin/rust-rdm/releases/tag/v1.0.0",
            "https://github.com/pigjin/other/releases/tag/v1.0.0",
            "https://github.com/pigjin/rust-rdm/releases/tag/v1.0.0?redirect=evil",
            "javascript:alert(1)",
        ] {
            let mut value = release("1.0.0");
            value["html_url"] = bad.into();
            assert_eq!(
                parse(&value, "0.1.5", "macos", "x86_64"),
                Err(CheckError::InvalidResponse)
            );
        }
        for bad in [
            "https://github.com/pigjin/rust-rdm/releases/download/v0.0.1/wrong.dmg",
            "https://evil.test/RedisDock-1.0.0-macos-x86_64.dmg",
            "https://github.com/pigjin/other/releases/download/v1.0.0/RedisDock-1.0.0-macos-x86_64.dmg",
            "http://github.com/pigjin/rust-rdm/releases/download/v1.0.0/RedisDock-1.0.0-macos-x86_64.dmg",
            "https://github.com/pigjin/rust-rdm/releases/download/v1.0.0/RedisDock-1.0.0-macos-x86_64.dmg?redirect=evil",
        ] {
            let mut value = release("1.0.0");
            value["assets"][0]["browser_download_url"] = bad.into();
            assert_eq!(
                parse(&value, "0.1.5", "macos", "x86_64"),
                Err(CheckError::InvalidResponse)
            );
        }
        let mut value = release("1.0.0");
        let duplicate = value["assets"][0].clone();
        value["assets"].as_array_mut().unwrap().push(duplicate);
        assert_eq!(
            parse(&value, "0.1.5", "macos", "x86_64"),
            Err(CheckError::InvalidResponse)
        );
        for bytes in [b"not json".as_slice(), b"{}", &vec![b' '; MAX_RESPONSE + 1]] {
            assert_eq!(
                parse_release(bytes, "0.1.5", "linux", "x86_64"),
                Err(CheckError::InvalidResponse)
            );
        }
    }

    #[test]
    fn persists_daily_attempts_and_recovers_invalid_records() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::new(dir.path());
        let now = 1_800_000_000;
        assert!(store.load::<CheckRecord>(CHECK_FILE).unwrap().due(now));
        store
            .save(
                CHECK_FILE,
                &CheckRecord {
                    last_check_at: Some(now),
                },
            )
            .unwrap();
        let loaded: CheckRecord = store.load(CHECK_FILE).unwrap();
        assert!(!loaded.due(now + 86_399));
        assert!(loaded.due(now + 86_400));
        assert!(loaded.due(now - 1));
        assert!(
            CheckRecord {
                last_check_at: Some(0)
            }
            .due(now)
        );
        std::fs::write(dir.path().join(CHECK_FILE), "last_check_at: [broken").unwrap();
        assert!(
            store
                .load::<CheckRecord>(CHECK_FILE)
                .unwrap_or_default()
                .due(now)
        );
    }

    #[test]
    fn coalesces_manual_requests_and_caches_success_and_failure_for_one_minute() {
        let now = Instant::now();
        let mut checker = Checker::default();
        assert!(matches!(checker.start(false, now), Start::Request));
        assert!(matches!(checker.start(true, now), Start::Pending));
        assert!(checker.finish(Err(CheckError::Network), now));
        assert!(!checker.running);
        assert!(matches!(
            checker.start(true, now + Duration::from_secs(59)),
            Start::Cached(Err(CheckError::Network))
        ));
        assert!(matches!(
            checker.start(true, now + Duration::from_secs(60)),
            Start::Request
        ));
        assert!(checker.finish(Ok(None), now + Duration::from_secs(60)));
        assert!(matches!(
            checker.start(true, now + Duration::from_secs(61)),
            Start::Cached(Ok(None))
        ));
        assert!(matches!(
            checker.start(false, now + Duration::from_secs(120)),
            Start::Request
        ));
        assert!(!checker.finish(Err(CheckError::RateLimited), now + Duration::from_secs(120)));
        assert!(matches!(
            checker.start(true, now + Duration::from_secs(180)),
            Start::Request
        ));
        assert!(checker.finish(Err(CheckError::NoRelease), now + Duration::from_secs(180)));
    }

    #[test]
    fn update_messages_cover_every_language_and_preserve_placeholders() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/langs");
        let english: Value =
            serde_json::from_slice(&std::fs::read(root.join("en-us.json")).unwrap()).unwrap();
        let expected = english["updates"].as_object().unwrap();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert!(
                !value["menu"]["check_update"].as_str().unwrap().is_empty(),
                "{}",
                path.display()
            );
            let actual = value["updates"].as_object().unwrap();
            assert_eq!(actual.len(), expected.len(), "{}", path.display());
            for (key, text) in expected {
                let translated = actual[key].as_str().unwrap();
                assert!(!translated.is_empty(), "{} {key}", path.display());
                for placeholder in ["{version}", "{format}"] {
                    assert_eq!(
                        translated.contains(placeholder),
                        text.as_str().unwrap().contains(placeholder),
                        "{} {key}",
                        path.display()
                    );
                }
            }
        }
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn http_request_handles_failures_bounds_and_sends_no_credentials() {
        use futures::future::BoxFuture;
        use http_client::{AsyncBody, HttpClient, Request, RequestTimeout, Response};

        struct Fake {
            status: u16,
            body: Vec<u8>,
        }
        impl HttpClient for Fake {
            fn send(
                &self,
                request: Request<AsyncBody>,
            ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
                assert_eq!(request.uri(), API_URL);
                assert_eq!(
                    request.extensions().get::<RequestTimeout>().unwrap().0,
                    Duration::from_secs(10)
                );
                assert!(request.headers().get("Authorization").is_none());
                assert!(
                    request
                        .headers()
                        .get("User-Agent")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .starts_with("RedisDock/")
                );
                let result = if self.status == 0 {
                    Err(anyhow::anyhow!("network timeout"))
                } else {
                    Ok(Response::builder()
                        .status(self.status)
                        .body(self.body.clone().into())
                        .unwrap())
                };
                Box::pin(async { result })
            }
            fn user_agent(&self) -> Option<&http_client::http::HeaderValue> {
                None
            }
            fn proxy(&self) -> Option<&Url> {
                None
            }
        }
        for (status, expected) in [
            (0, CheckError::Network),
            (404, CheckError::NoRelease),
            (403, CheckError::RateLimited),
            (429, CheckError::RateLimited),
            (500, CheckError::Network),
            (302, CheckError::InvalidResponse),
        ] {
            assert_eq!(
                futures::executor::block_on(fetch(&Fake {
                    status,
                    body: vec![]
                })),
                Err(expected)
            );
        }
        for body in [b"invalid json".to_vec(), vec![b' '; MAX_RESPONSE + 1]] {
            assert_eq!(
                futures::executor::block_on(fetch(&Fake { status: 200, body })),
                Err(CheckError::InvalidResponse)
            );
        }
        let body = serde_json::to_vec(&release("999.0.0")).unwrap();
        assert!(
            futures::executor::block_on(fetch(&Fake { status: 200, body }))
                .unwrap()
                .is_some()
        );
    }
}
