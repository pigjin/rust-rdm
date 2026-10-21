use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::BTreeMap, time::Duration};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const VERSION_URL: &str = "https://tinyrdm.com/client_version.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Release {
    pub version: String,
    pub description: BTreeMap<String, String>,
    pub download_page: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "null_list")]
    pub sponsor: Vec<Sponsor>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sponsor {
    pub name: String,
    pub link: String,
    #[serde(default, deserialize_with = "null_list")]
    pub region: Vec<String>,
}

fn null_list<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

pub fn language_code(language: &str) -> &str {
    match language {
        "zh-tw" | "zh-hant" | "zh-hk" => "tw",
        other => other.split('-').next().unwrap_or("en"),
    }
}

impl Sponsor {
    pub fn select<'a>(items: &'a [Self], language: &str) -> Option<&'a Self> {
        let code = language_code(language);
        items
            .iter()
            .find(|item| item.region.is_empty() || item.region.iter().any(|region| region == code))
    }
}

impl Release {
    pub fn available(&self, current: &str, skipped: &str, manual: bool) -> bool {
        compare_versions(&self.version, current) == Ordering::Greater
            && (manual || compare_versions(&self.version, skipped) != Ordering::Equal)
    }

    pub fn text<'a>(&'a self, values: &'a BTreeMap<String, String>, language: &str) -> &'a str {
        let language = language_code(language);
        values
            .get(language)
            .or_else(|| values.get("en"))
            .map(String::as_str)
            .unwrap_or_default()
    }
}

pub fn fetch() -> Result<Release> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .new_agent();
    let release: Release = agent
        .get(VERSION_URL)
        .header(
            "User-Agent",
            concat!("Tiny-RDM/", env!("CARGO_PKG_VERSION")),
        )
        .call()?
        .body_mut()
        .read_json()?;
    if release.version.is_empty() {
        bail!("版本接口返回了空版本号");
    }
    Ok(release)
}

pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn parts(version: &str) -> Vec<u64> {
        version
            .trim_start_matches('v')
            .split('.')
            .map(|part| {
                part.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    }
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len()))
        .find_map(|i| {
            let order = a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0));
            (order != Ordering::Equal).then_some(order)
        })
        .unwrap_or(Ordering::Equal)
}
