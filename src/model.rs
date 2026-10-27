use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionFieldError {
    Required,
    IllegalCharacters,
}

pub fn connection_form_error(field: &str, value: &str) -> Option<ConnectionFieldError> {
    if !matches!(field, "name" | "addr" | "default_filter" | "key_separator") {
        return None;
    }
    if value.is_empty() {
        Some(ConnectionFieldError::Required)
    } else if field == "name" && value.contains(['/', '\\']) {
        Some(ConnectionFieldError::IllegalCharacters)
    } else {
        None
    }
}
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use unicode_properties::{GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default = "Connection::stored_empty")]
pub struct Connection {
    pub name: String,
    pub group: String,
    #[serde(rename = "type")]
    pub category: String,
    pub connections: Vec<Connection>,
    #[serde(alias = "lastDB")]
    pub last_db: u32,
    pub network: String,
    pub sock: String,
    pub addr: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    #[serde(alias = "defaultFilter")]
    pub default_filter: String,
    #[serde(alias = "keySeparator")]
    pub key_separator: String,
    #[serde(alias = "connTimeout")]
    pub conn_timeout: u64,
    #[serde(alias = "execTimeout")]
    pub exec_timeout: u64,
    #[serde(alias = "dbFilterType")]
    pub db_filter_type: String,
    #[serde(alias = "dbFilterList")]
    pub db_filter_list: Vec<u32>,
    #[serde(alias = "keyView")]
    pub key_view: u8,
    #[serde(alias = "loadSize")]
    pub load_size: usize,
    #[serde(alias = "markColor")]
    pub mark_color: String,
    #[serde(alias = "refreshInterval")]
    pub refresh_interval: i64,
    pub alias: BTreeMap<i64, String>,
    pub ssl: Ssl,
    pub ssh: Ssh,
    pub sentinel: Sentinel,
    pub cluster: Cluster,
    pub proxy: Proxy,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            name: String::new(),
            group: String::new(),
            category: String::new(),
            connections: vec![],
            last_db: 0,
            network: "tcp".into(),
            sock: String::new(),
            addr: "127.0.0.1".into(),
            port: 6379,
            username: String::new(),
            password: String::new(),
            default_filter: "*".into(),
            key_separator: ":".into(),
            conn_timeout: 60,
            exec_timeout: 60,
            db_filter_type: "none".into(),
            db_filter_list: vec![],
            key_view: 0,
            load_size: 10000,
            mark_color: String::new(),
            refresh_interval: 5,
            alias: BTreeMap::new(),
            ssl: Ssl::default(),
            ssh: Ssh::default(),
            sentinel: Sentinel::default(),
            cluster: Cluster::default(),
            proxy: Proxy::default(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ssl {
    pub enable: bool,
    #[serde(rename = "keyfile", alias = "keyFile", alias = "key_file")]
    pub key_file: String,
    #[serde(rename = "certfile", alias = "certFile", alias = "cert_file")]
    pub cert_file: String,
    #[serde(rename = "cafile", alias = "caFile", alias = "ca_file")]
    pub ca_file: String,
    #[serde(alias = "allowInsecure")]
    pub allow_insecure: bool,
    pub sni: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SshHostKey {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default = "Ssh::stored_empty")]
pub struct Ssh {
    pub enable: bool,
    pub addr: String,
    pub port: u16,
    #[serde(alias = "loginType")]
    pub login_type: String,
    pub username: String,
    pub password: String,
    #[serde(alias = "pkFile")]
    pub pk_file: String,
    pub passphrase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted_host: Option<SshHostKey>,
}
impl Default for Ssh {
    fn default() -> Self {
        Self {
            enable: false,
            addr: String::new(),
            port: 22,
            login_type: "pwd".into(),
            username: String::new(),
            password: String::new(),
            pk_file: String::new(),
            passphrase: String::new(),
            trusted_host: None,
        }
    }
}
impl Ssh {
    fn stored_empty() -> Self {
        Self {
            port: 0,
            login_type: String::new(),
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default = "Sentinel::stored_empty")]
pub struct Sentinel {
    pub enable: bool,
    pub master: String,
    pub username: String,
    pub password: String,
}
impl Default for Sentinel {
    fn default() -> Self {
        Self {
            enable: false,
            master: "mymaster".into(),
            username: String::new(),
            password: String::new(),
        }
    }
}
impl Sentinel {
    fn stored_empty() -> Self {
        Self {
            master: String::new(),
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cluster {
    pub enable: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Proxy {
    #[serde(rename = "type")]
    pub kind: u8,
    pub schema: String,
    pub addr: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

impl Connection {
    fn stored_empty() -> Self {
        Self {
            network: String::new(),
            addr: String::new(),
            port: 0,
            default_filter: String::new(),
            key_separator: String::new(),
            conn_timeout: 0,
            exec_timeout: 0,
            db_filter_type: String::new(),
            load_size: 0,
            refresh_interval: 0,
            ssh: Ssh::stored_empty(),
            sentinel: Sentinel::stored_empty(),
            ..Self::default()
        }
    }
    pub fn form_default() -> Self {
        let mut connection = Self::default();
        connection.sock = "/tmp/redis.sock".into();
        connection.ssl.allow_insecure = true;
        connection.proxy.schema = "http".into();
        connection.refresh_interval = 0;
        connection
    }
    pub fn form_data(&self) -> serde_json::Value {
        use serde_json::Value;
        fn omit_rpc_zeroes(value: &mut Value, profile: bool) {
            let Value::Object(fields) = value else {
                return;
            };
            fields.retain(|key, value| {
                if profile
                    && matches!(
                        key.as_str(),
                        "name" | "last_db" | "db_filter_type" | "db_filter_list"
                    )
                {
                    return true;
                }
                match value {
                    Value::Object(fields) if key == "alias" => !fields.is_empty(),
                    Value::Object(_) => {
                        omit_rpc_zeroes(value, false);
                        true
                    }
                    Value::Array(values) => {
                        for child in values.iter_mut() {
                            omit_rpc_zeroes(child, true);
                        }
                        !values.is_empty()
                    }
                    Value::String(value) => !value.is_empty(),
                    Value::Number(value) => value.as_f64() != Some(0.),
                    Value::Bool(value) => *value,
                    Value::Null => false,
                }
            });
        }
        fn merge(dest: &mut Value, source: &Value) {
            if let (Value::Object(dest), Value::Object(source)) = (&mut *dest, source) {
                for key in source.keys() {
                    dest.entry(key.clone()).or_insert(Value::Null);
                }
                for (key, dest) in dest.iter_mut() {
                    merge(dest, source.get(key).unwrap_or(&Value::Null));
                }
                return;
            }
            match source {
                Value::String(value) if value.is_empty() => {}
                Value::Number(value) if value.as_f64() == Some(0.) => {}
                source => *dest = source.clone(),
            }
        }
        fn normalize(value: &mut Value, template: &Value) {
            if let (Value::Object(value), Value::Object(template)) = (&mut *value, template) {
                for (key, field) in template {
                    normalize(value.entry(key.clone()).or_insert(Value::Null), field);
                }
            } else if value.is_null() {
                *value = match template {
                    Value::String(_) => Value::String(String::new()),
                    Value::Bool(_) => Value::Bool(false),
                    Value::Array(_) => Value::Array(vec![]),
                    Value::Object(_) => Value::Object(serde_json::Map::new()),
                    _ => Value::Null,
                };
            }
        }
        let template = serde_json::to_value(Self::form_default()).unwrap();
        let mut data = template.clone();
        let mut rpc = serde_json::to_value(self).unwrap();
        omit_rpc_zeroes(&mut rpc, true);
        merge(&mut data, &rpc);
        normalize(&mut data, &template);
        if data["ssh"]["login_type"].as_str().is_none_or(str::is_empty) {
            data["ssh"]["login_type"] = Value::String("pwd".into());
        }
        data["proxy"]["auth"] = Value::Bool(!self.proxy.username.is_empty());
        data
    }
    pub fn form_profile(&self) -> Self {
        fn numbers(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(fields) => fields.values_mut().for_each(numbers),
                serde_json::Value::Null => *value = serde_json::Value::from(0),
                _ => {}
            }
        }
        let mut data = self.form_data();
        numbers(&mut data);
        serde_json::from_value(data).unwrap()
    }
    pub fn prepare_form_save(&mut self, proxy_auth: bool) {
        if self.network == "unix" {
            self.addr.clear();
            self.port = 0;
            self.sock = self.sock.trim().to_string();
        } else {
            self.network = "tcp".into();
            self.addr = self.addr.trim().to_string();
            self.sock.clear();
        }
        if self.db_filter_type == "none" {
            self.db_filter_list.clear();
        }
        if !self.ssl.enable {
            self.ssl = Ssl::default();
        }
        if !self.ssh.enable {
            self.ssh = Ssh::default();
        } else {
            match self.ssh.login_type.as_str() {
                "pkfile" => self.ssh.password.clear(),
                "agent" => {
                    self.ssh.password.clear();
                    self.ssh.pk_file.clear();
                    self.ssh.passphrase.clear();
                }
                _ => {
                    self.ssh.pk_file.clear();
                    self.ssh.passphrase.clear();
                }
            }
        }
        if !self.sentinel.enable {
            self.sentinel = Sentinel::default();
        }
        if !self.cluster.enable {
            self.cluster = Cluster::default();
        }
        if self.proxy.kind != 2 {
            let kind = self.proxy.kind;
            self.proxy = Proxy {
                kind,
                ..Default::default()
            };
        } else if !proxy_auth {
            self.proxy.username.clear();
            self.proxy.password.clear();
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.contains(['/', '\\']) {
            bail!("连接名称不能为空，且不能包含 / 或 \\");
        }
        if self.category == "group" {
            return Ok(());
        }
        if self.network == "unix" {
            if self.sock.is_empty() {
                bail!("UNIX 套接字路径不能为空");
            }
        } else if self.addr.trim().is_empty() || self.port == 0 {
            bail!("地址和端口不能为空");
        }
        if self.default_filter.is_empty() || self.key_separator.is_empty() {
            bail!("默认筛选和键分隔符不能为空");
        }
        if self.ssh.enable && (self.ssh.addr.is_empty() || self.ssh.username.is_empty()) {
            bail!("SSH 地址和用户名不能为空");
        }
        if self.cluster.enable && self.sentinel.enable {
            bail!("集群和哨兵模式不能同时启用");
        }
        Ok(())
    }
    pub fn from_url(text: &str) -> Result<Self> {
        let url = url::Url::parse(text)?;
        if !matches!(url.scheme(), "redis" | "rediss" | "unix") {
            bail!("仅支持 redis://、rediss://、unix://");
        }
        let mut c = Self::default();
        c.addr = url.host_str().unwrap_or("127.0.0.1").into();
        c.port = url.port().unwrap_or(6379);
        c.name = format!("{}:{}", c.addr, c.port);
        c.username = percent_decode(url.username());
        c.password = percent_decode(url.password().unwrap_or(""));
        c.last_db = url.path().trim_start_matches('/').parse().unwrap_or(0);
        c.ssl.enable = url.scheme() == "rediss";
        if url.scheme() == "unix" {
            c.network = "unix".into();
            c.sock = url.path().into();
        }
        Ok(c)
    }
}
pub(crate) fn percent_decode(text: &str) -> String {
    let mut bytes = vec![];
    let mut i = 0;
    while i < text.len() {
        if text.as_bytes()[i] == b'%'
            && i + 2 < text.len()
            && let Ok(n) = u8::from_str_radix(&text[i + 1..i + 3], 16)
        {
            bytes.push(n);
            i += 3;
            continue;
        }
        bytes.push(text.as_bytes()[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub behavior: Behavior,
    pub general: General,
    pub editor: Editor,
    pub cli: Cli,
    pub decoder: Vec<Decoder>,
}

impl Preferences {
    /// Match the original Go RPC's integer parameters while allowing floating
    /// point font sizes in the unsaved UI preview.
    pub fn from_form_json(mut value: serde_json::Value) -> Result<Self> {
        for (section, key, rpc_key, go_type) in [
            ("general", "font_size", "fontSize", "PreferencesGeneral"),
            ("general", "scan_size", "scanSize", "PreferencesGeneral"),
            ("editor", "font_size", "fontSize", "PreferencesEditor"),
            ("cli", "font_size", "fontSize", "PreferencesCli"),
        ] {
            let Some(section_value) = value.get_mut(section) else {
                continue;
            };
            let field = if section_value.get(key).is_some() {
                key
            } else {
                rpc_key
            };
            let Some(number) = section_value.get_mut(field) else {
                continue;
            };
            if number.is_null() {
                *number = 0.into();
            } else if number.as_i64().is_none() {
                bail!(
                    "json: cannot unmarshal number {number} into Go struct field {go_type}.{section}.{rpc_key} of type int"
                );
            }
        }
        Ok(serde_json::from_value(value)?)
    }

    pub fn welcome_choice(&mut self, allow_tracking: bool) {
        self.behavior.welcomed = true;
        self.general.allow_track = allow_tracking;
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Behavior {
    pub welcomed: bool,
    #[serde(alias = "asideWidth")]
    pub aside_width: f32,
    #[serde(alias = "windowWidth")]
    pub window_width: f32,
    #[serde(alias = "windowHeight")]
    pub window_height: f32,
    #[serde(alias = "windowMaximised")]
    pub window_maximised: bool,
    #[serde(alias = "windowPosX")]
    pub window_pos_x: f32,
    #[serde(alias = "windowPosY")]
    pub window_pos_y: f32,
}
impl Default for Behavior {
    fn default() -> Self {
        Self {
            welcomed: false,
            aside_width: 300.,
            window_width: 1024.,
            window_height: 768.,
            window_maximised: false,
            window_pos_x: 0.,
            window_pos_y: 0.,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub theme: String,
    pub language: String,
    pub font: String,
    #[serde(alias = "fontFamily")]
    pub font_family: Vec<String>,
    #[serde(alias = "fontSize")]
    pub font_size: f32,
    #[serde(alias = "scanSize")]
    pub scan_size: usize,
    #[serde(alias = "keyIconStyle")]
    pub key_icon_style: u8,
    #[serde(alias = "useSysProxy")]
    pub use_sys_proxy: bool,
    #[serde(alias = "useSysProxyHttp")]
    pub use_sys_proxy_http: bool,
    #[serde(alias = "allowTrack")]
    pub allow_track: bool,
}
impl Default for General {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            language: "auto".into(),
            font: String::new(),
            font_family: vec![],
            font_size: 14.,
            scan_size: 3000,
            key_icon_style: 0,
            use_sys_proxy: false,
            use_sys_proxy_http: false,
            allow_track: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Editor {
    pub font: String,
    #[serde(alias = "fontFamily")]
    pub font_family: Vec<String>,
    #[serde(alias = "fontSize")]
    pub font_size: f32,
    #[serde(alias = "showLineNum")]
    pub show_line_num: bool,
    #[serde(alias = "showFolding")]
    pub show_folding: bool,
    #[serde(alias = "dropText")]
    pub drop_text: bool,
    pub links: bool,
    #[serde(alias = "entryTextAlign")]
    pub entry_text_align: u8,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            font: String::new(),
            font_family: vec![],
            font_size: 14.,
            show_line_num: true,
            show_folding: true,
            drop_text: true,
            links: true,
            entry_text_align: 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Cli {
    #[serde(alias = "fontFamily")]
    pub font_family: Vec<String>,
    #[serde(alias = "fontSize")]
    pub font_size: f32,
    #[serde(alias = "cursorStyle")]
    pub cursor_style: String,
}
impl Default for Cli {
    fn default() -> Self {
        Self {
            font_family: vec![],
            font_size: 14.,
            cursor_style: "block".into(),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Decoder {
    pub name: String,
    pub enable: bool,
    pub auto: bool,
    #[serde(alias = "decodePath")]
    pub decode_path: String,
    #[serde(alias = "decodeArgs")]
    pub decode_args: Vec<String>,
    #[serde(alias = "encodePath")]
    pub encode_path: String,
    #[serde(alias = "encodeArgs")]
    pub encode_args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum KeyType {
    #[default]
    None,
    Unknown,
    String,
    Hash,
    List,
    Set,
    ZSet,
    Stream,
    Json,
    Other(String),
}
impl KeyType {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "string" => Self::String,
            "hash" => Self::Hash,
            "list" => Self::List,
            "set" => Self::Set,
            "zset" => Self::ZSet,
            "stream" => Self::Stream,
            "rejson-rl" | "json" => Self::Json,
            _ => Self::Other(s.into()),
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Self::None => "NONE",
            Self::Unknown => "…",
            Self::String => "STRING",
            Self::Hash => "HASH",
            Self::List => "LIST",
            Self::Set => "SET",
            Self::ZSet => "ZSET",
            Self::Stream => "STREAM",
            Self::Json => "JSON",
            Self::Other(s) => s,
        }
    }
    pub fn short(&self) -> &str {
        match self {
            Self::None => "NONE",
            Self::Unknown => "…",
            Self::String => "S",
            Self::Hash => "H",
            Self::List => "L",
            Self::Set => "E",
            Self::ZSet => "Z",
            Self::Stream => "X",
            Self::Json => "J",
            _ => "N",
        }
    }
    pub fn colors(&self) -> (u32, u32) {
        match self {
            Self::String => (0x8B5CF6, 0xF2EDFB),
            Self::Hash => (0x3B82F6, 0xE4F0FC),
            Self::List => (0x10B981, 0xE3F3EB),
            Self::Set => (0xF59E0B, 0xFDF1DF),
            Self::ZSet => (0xEF4444, 0xFAEAED),
            Self::Stream => (0xEC4899, 0xFDE6F1),
            Self::Json => (0x828766, 0xECECD9),
            _ => (0x888888, 0xEEEEEE),
        }
    }
}
#[derive(Clone, Debug)]
pub struct KeyEntry {
    pub key: Vec<u8>,
    pub kind: KeyType,
}
#[derive(Clone, Debug, Default)]
pub struct KeyDetail {
    pub key: Vec<u8>,
    pub kind: KeyType,
    pub ttl: i64,
    pub memory: u64,
    pub length: u64,
    pub encoding: String,
    pub value: Vec<u8>,
    pub rows: Vec<Entry>,
    pub cursor: u64,
    pub stream_cursor: String,
    pub end: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub id: Vec<u8>,
    pub value: Vec<u8>,
    pub score: String,
    pub fields: Vec<(Vec<u8>, Vec<u8>)>,
    /// A returned AddFields display value until the next explicit conversion.
    pub display_value: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Database {
    pub index: u32,
    pub keys: u64,
    pub expires: u64,
    pub avg_ttl: u64,
    pub alias: String,
}
pub type Info = BTreeMap<String, BTreeMap<String, String>>;

pub fn contains_binary(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return true;
    };
    text.chars().any(|c| {
        c == '\u{fffd}'
            || (!c.is_whitespace()
                && c.general_category() != GeneralCategory::SpaceSeparator
                && !matches!(
                    c.general_category_group(),
                    GeneralCategoryGroup::Letter
                        | GeneralCategoryGroup::Mark
                        | GeneralCategoryGroup::Number
                        | GeneralCategoryGroup::Punctuation
                        | GeneralCategoryGroup::Symbol
                ))
    })
}

pub fn display_bytes(b: &[u8]) -> String {
    if !contains_binary(b)
        && let Ok(s) = std::str::from_utf8(b)
    {
        return s.to_owned();
    }
    b.iter()
        .map(|n| {
            if (32..127).contains(n) && *n != b'\\' {
                (*n as char).to_string()
            } else {
                format!("\\x{n:02X}")
            }
        })
        .collect()
}

pub fn parse_bytes(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'x' if i + 3 < bytes.len() => {
                    out.push(u8::from_str_radix(&s[i + 2..i + 4], 16)?);
                    i += 4;
                    continue;
                }
                b'n' => {
                    out.push(b'\n');
                    i += 2;
                    continue;
                }
                b'r' => {
                    out.push(b'\r');
                    i += 2;
                    continue;
                }
                b't' => {
                    out.push(b'\t');
                    i += 2;
                    continue;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Ok(out)
}
