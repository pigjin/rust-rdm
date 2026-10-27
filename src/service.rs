use crate::{
    model::*,
    redis::{self, Cancellation, Client, Value, Wire, command},
    storage::Store,
};
use anyhow::{Result, bail};
mod entry_edit;
pub use entry_edit::EntryUpdate;
use parking_lot::{ArcMutexGuard, Mutex, RawMutex};
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct KeyNotFound;

impl std::fmt::Display for KeyNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("key not exists")
    }
}

impl std::error::Error for KeyNotFound {}

#[derive(Clone, Debug)]
pub struct CommandLog {
    pub time: String,
    pub server: String,
    pub db: u32,
    pub command: String,
    pub elapsed_ms: f64,
    pub error: String,
}
#[derive(Clone, Debug, Default)]
pub struct ScanState {
    pub nodes: Vec<(String, u16)>,
    pub node: usize,
    pub cursor: u64,
    pub started: bool,
    pub end: bool,
}
#[derive(Clone, Debug, Default)]
pub struct ScanPage {
    pub keys: Vec<KeyEntry>,
    pub state: ScanState,
    pub total: u64,
}
#[derive(Clone, Debug, Default)]
pub struct TransferProgress {
    pub done: usize,
    pub failed: usize,
    pub skipped: usize,
    pub total: usize,
    /// Current input item, including the export item whose DUMP is in flight.
    pub current: usize,
    pub canceled: bool,
}
#[derive(Clone, Debug)]
pub struct SlowLog {
    pub id: i64,
    pub time: i64,
    pub duration: i64,
    pub args: String,
    pub address: String,
    pub name: String,
}

pub struct Service {
    clients: Mutex<HashMap<(String, u32), Arc<Mutex<Client>>>>,
    cli_clients: Mutex<HashMap<String, Arc<Mutex<Client>>>>,
    pub history: Mutex<VecDeque<CommandLog>>,
    pub store: Store,
}
impl Service {
    pub fn new(store: Store) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            cli_clients: Mutex::new(HashMap::new()),
            history: Mutex::new(VecDeque::new()),
            store,
        }
    }
    pub fn connect(&self, c: &Connection, db: u32) -> Result<()> {
        let client = Client::connect(c, db)?;
        self.clients
            .lock()
            .insert((c.name.clone(), db), Arc::new(Mutex::new(client)));
        Ok(())
    }
    pub fn close(&self, name: &str) {
        self.clients.lock().retain(|(n, _), _| n != name);
        self.cli_clients.lock().remove(name);
    }
    pub fn client(&self, c: &Connection, db: u32) -> Result<ArcMutexGuard<RawMutex, Client>> {
        let id = (c.name.clone(), db);
        let existing = self.clients.lock().get(&id).cloned();
        let client = if let Some(client) = existing {
            client
        } else {
            let client = Arc::new(Mutex::new(Client::connect(c, db)?));
            self.clients.lock().entry(id).or_insert(client).clone()
        };
        Ok(client.lock_arc())
    }
    pub fn run(&self, c: &Connection, db: u32, args: Vec<Vec<u8>>) -> Result<Value> {
        self.logged(c, db, std::slice::from_ref(&args), || {
            self.client(c, db)?.query(&args)
        })
    }
    pub fn run_cli(&self, c: &Connection, db: u32, args: Vec<Vec<u8>>) -> Result<Value> {
        self.logged(c, db, std::slice::from_ref(&args), || {
            let existing = self.cli_clients.lock().get(&c.name).cloned();
            let client = if let Some(client) = existing {
                client
            } else {
                let client = Arc::new(Mutex::new(Client::connect(c, 0)?));
                self.cli_clients
                    .lock()
                    .entry(c.name.clone())
                    .or_insert(client)
                    .clone()
            };
            client.lock().query(&args)
        })
    }
    fn run_at(
        &self,
        c: &Connection,
        db: u32,
        host: &str,
        port: u16,
        args: Vec<Vec<u8>>,
    ) -> Result<Value> {
        self.logged(c, db, std::slice::from_ref(&args), || {
            self.client(c, db)?.query_at(host, port, &args)
        })
    }
    pub fn key_types(&self, c: &Connection, db: u32, keys: &[Vec<u8>], cancel: &Cancellation) -> Result<Vec<KeyType>> {
        if keys.is_empty() || cancel.canceled() {
            return Ok(vec![]);
        }
        let commands = keys.iter().map(|key| vec![b"TYPE".to_vec(), key.clone()]).collect::<Vec<_>>();
        self.logged(c, db, &commands, || {
            self.client(c, db)?.key_types(keys, cancel)
                .map(|values| values.into_iter().map(|value| KeyType::parse(&value.text())).collect())
        })
    }
    fn logged<T>(
        &self,
        c: &Connection,
        db: u32,
        commands: &[Vec<Vec<u8>>],
        work: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let start = Instant::now();
        let result = work();
        if commands.is_empty() {
            return result;
        }
        let mut history = self.history.lock();
        history.push_back(CommandLog {
            time: chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S%.3f")
                .to_string(),
            server: c.name.clone(),
            db,
            command: commands
                .iter()
                .map(|args| {
                    if args
                        .first()
                        .is_some_and(|arg| arg.eq_ignore_ascii_case(b"AUTH"))
                    {
                        "AUTH ***".into()
                    } else {
                        args.iter()
                            .map(|arg| display_bytes(arg))
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.,
            error: result
                .as_ref()
                .err()
                .map(|error| error.to_string())
                .unwrap_or_default(),
        });
        while history.len() > 5000 {
            history.pop_front();
        }
        result
    }
    pub fn info(&self, c: &Connection) -> Result<Info> {
        Ok(parse_info(
            &self.run(c, 0, command(&["INFO", "ALL"]))?.text(),
        ))
    }
    pub fn client_list(
        &self,
        c: &Connection,
    ) -> Result<Vec<std::collections::BTreeMap<String, String>>> {
        let nodes = self.client(c, 0)?.masters()?;
        let mut clients: Vec<std::collections::BTreeMap<String, String>> = vec![];
        for (host, port) in nodes {
            let value = self.run_at(c, 0, &host, port, command(&["CLIENT", "LIST"]))?;
            for line in value.text().lines().filter(|line| !line.trim().is_empty()) {
                clients.push(
                    line.split_whitespace()
                        .filter_map(|field| field.split_once('='))
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect(),
                );
            }
        }
        clients.sort_by_key(|client| {
            std::cmp::Reverse(
                client
                    .get("age")
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0),
            )
        });
        Ok(clients)
    }
    pub fn databases(&self, c: &Connection) -> Result<Vec<Database>> {
        let info = self.info(c)?;
        let count = if c.cluster.enable {
            1
        } else {
            self.run(c, 0, command(&["CONFIG", "GET", "databases"]))
                .ok()
                .and_then(|v| {
                    v.array()
                        .ok()
                        .and_then(|a| a.get(1).and_then(|v| v.integer().ok()))
                })
                .unwrap_or(16)
                .clamp(1, 65536) as u32
        };
        let mut out = vec![];
        for index in 0..count {
            if c.db_filter_type == "show" && !c.db_filter_list.contains(&index)
                || c.db_filter_type == "hide" && c.db_filter_list.contains(&index)
            {
                continue;
            }
            let text = info
                .get("Keyspace")
                .and_then(|s| s.get(&format!("db{index}")))
                .cloned()
                .unwrap_or_default();
            let parts = text
                .split(',')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k, v.parse::<u64>().unwrap_or(0)))
                .collect::<HashMap<_, _>>();
            out.push(Database {
                index,
                keys: *parts.get("keys").unwrap_or(&0),
                expires: *parts.get("expires").unwrap_or(&0),
                avg_ttl: *parts.get("avg_ttl").unwrap_or(&0),
                alias: c.alias.get(&i64::from(index)).cloned().unwrap_or_default(),
            });
        }
        Ok(out)
    }
    pub fn scan(
        &self,
        c: &Connection,
        db: u32,
        pattern: &str,
        kind: &str,
        exact: bool,
        mut state: ScanState,
        count: usize,
    ) -> Result<ScanPage> {
        if state.end {
            return Ok(ScanPage {
                state,
                ..Default::default()
            });
        }
        let pattern = if pattern.is_empty() { "*" } else { pattern };
        if exact {
            let mut key = pattern.as_bytes().to_vec();
            let mut typ =
                KeyType::parse(&self.run(c, db, vec![b"TYPE".to_vec(), key.clone()])?.text());
            if typ == KeyType::None
                && let Ok(binary) = parse_bytes(pattern)
                && binary != key
            {
                key = binary;
                typ = KeyType::parse(&self.run(c, db, vec![b"TYPE".to_vec(), key.clone()])?.text());
            }
            let keys = if typ != KeyType::None
                && (kind.is_empty() || typ.name().eq_ignore_ascii_case(kind))
            {
                vec![KeyEntry { key, kind: typ }]
            } else {
                vec![]
            };
            state.end = true;
            return Ok(ScanPage {
                total: keys.len() as u64,
                keys,
                state,
            });
        }
        if state.nodes.is_empty() {
            state.nodes = self.client(c, db)?.masters()?;
        }
        let mut keys = vec![];
        let mut seen = HashSet::new();
        let size = count.max(1);
        let mut rounds = 0;
        while state.node < state.nodes.len() && keys.len() < size && rounds < 64 {
            let (host, port) = state.nodes[state.node].clone();
            let mut args = command(&[
                "SCAN",
                &state.cursor.to_string(),
                "MATCH",
                pattern,
                "COUNT",
                &(size - keys.len()).to_string(),
            ]);
            if !kind.is_empty() {
                args.extend(command(&[
                    "TYPE",
                    if kind.eq_ignore_ascii_case("JSON") {
                        "ReJSON-RL"
                    } else {
                        kind
                    },
                ]));
            }
            let value = self.run_at(c, db, &host, port, args)?;
            let a = value.array()?;
            if a.len() != 2 {
                bail!("SCAN 响应异常");
            }
            state.cursor = a[0].integer()? as u64;
            state.started = true;
            let found = a[1]
                .array()?
                .iter()
                .map(Value::bytes)
                .filter(|key| seen.insert(key.clone()))
                .collect::<Vec<_>>();
            let typ = if kind.is_empty() { KeyType::Unknown } else { KeyType::parse(kind) };
            keys.extend(found.into_iter().map(|key| KeyEntry { key, kind: typ.clone() }));
            if state.cursor == 0 {
                state.node += 1;
            }
            rounds += 1;
        }
        state.end = state.node >= state.nodes.len();
        let total = if c.cluster.enable {
            let mut sum = 0;
            for (h, p) in &state.nodes {
                sum += self.run_at(c, db, h, *p, command(&["DBSIZE"]))?.integer()? as u64;
            }
            sum
        } else {
            self.run(c, db, command(&["DBSIZE"]))?.integer()? as u64
        };
        Ok(ScanPage { keys, state, total })
    }
    pub fn detail(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        cursor: u64,
        pattern: &str,
        page: usize,
        stream_after: &str,
        reverse: bool,
    ) -> Result<KeyDetail> {
        let kind = KeyType::parse(
            &self
                .run(c, db, vec![b"TYPE".to_vec(), key.to_vec()])?
                .text(),
        );
        if kind == KeyType::None {
            return Err(KeyNotFound.into());
        }
        let ttl = self
            .run(c, db, vec![b"TTL".to_vec(), key.to_vec()])?
            .integer()?;
        let memory = self
            .run(
                c,
                db,
                vec![b"MEMORY".to_vec(), b"USAGE".to_vec(), key.to_vec()],
            )
            .ok()
            .and_then(|v| v.integer().ok())
            .unwrap_or(0)
            .max(0) as u64;
        let encoding = self
            .run(
                c,
                db,
                vec![b"OBJECT".to_vec(), b"ENCODING".to_vec(), key.to_vec()],
            )
            .map(|v| v.text())
            .unwrap_or_default();
        let mut d = KeyDetail {
            key: key.to_vec(),
            kind: kind.clone(),
            ttl,
            memory,
            encoding,
            ..Default::default()
        };
        let page = page.max(1);
        let pattern = if pattern.is_empty() { "*" } else { pattern };
        match kind {
            KeyType::String | KeyType::Json => {
                let cmd = if kind == KeyType::String {
                    "GET"
                } else {
                    "JSON.GET"
                };
                d.value = self
                    .run(c, db, vec![cmd.as_bytes().to_vec(), key.to_vec()])?
                    .bytes();
                d.length = if kind == KeyType::Json {
                    0
                } else {
                    d.value.len() as u64
                };
                d.end = true;
            }
            KeyType::Hash | KeyType::Set | KeyType::ZSet => {
                let (scan, len) = match kind {
                    KeyType::Hash => ("HSCAN", "HLEN"),
                    KeyType::Set => ("SSCAN", "SCARD"),
                    _ => ("ZSCAN", "ZCARD"),
                };
                d.length = self
                    .run(c, db, vec![len.as_bytes().to_vec(), key.to_vec()])?
                    .integer()? as u64;
                if kind == KeyType::ZSet && pattern == "*" {
                    let mut args = vec![
                        if reverse {
                            b"ZREVRANGE".to_vec()
                        } else {
                            b"ZRANGE".to_vec()
                        },
                        key.to_vec(),
                    ];
                    args.extend(command(&[
                        &cursor.to_string(),
                        &(cursor + page as u64 - 1).to_string(),
                        "WITHSCORES",
                    ]));
                    let value = self.run(c, db, args)?;
                    for r in value.array()?.chunks_exact(2) {
                        d.rows.push(Entry {
                            id: r[0].bytes(),
                            value: r[0].bytes(),
                            score: r[1].text(),
                            ..Default::default()
                        });
                    }
                    d.cursor = (cursor + d.rows.len() as u64).min(d.length);
                    d.end = d.cursor >= d.length;
                } else {
                    let full = pattern != "*";
                    let pattern = if full {
                        format!(
                            "{}{}{}",
                            if pattern.starts_with('*') { "" } else { "*" },
                            pattern,
                            if pattern.ends_with('*') { "" } else { "*" }
                        )
                    } else {
                        "*".into()
                    };
                    let mut current = cursor;
                    {
                        let mut args = vec![scan.as_bytes().to_vec(), key.to_vec()];
                        args.extend(command(&[
                            &current.to_string(),
                            "MATCH",
                            &pattern,
                            "COUNT",
                            &page.to_string(),
                        ]));
                        let v = self.run(c, db, args)?;
                        let a = v.array()?;
                        current = a[0].integer()? as u64;
                        let rows = a[1].array()?;
                        if kind == KeyType::Set {
                            d.rows.extend(rows.iter().map(|v| Entry {
                                id: v.bytes(),
                                value: v.bytes(),
                                ..Default::default()
                            }));
                        } else {
                            for r in rows.chunks_exact(2) {
                                d.rows.push(if kind == KeyType::Hash {
                                    Entry {
                                        id: r[0].bytes(),
                                        value: r[1].bytes(),
                                        ..Default::default()
                                    }
                                } else {
                                    Entry {
                                        id: r[0].bytes(),
                                        value: r[0].bytes(),
                                        score: r[1].text(),
                                        ..Default::default()
                                    }
                                });
                            }
                        }
                    }
                    d.cursor = current;
                    d.end = current == 0;
                }
                if kind == KeyType::ZSet {
                    d.rows.sort_by(|a, b| {
                        a.score
                            .parse::<f64>()
                            .unwrap_or_default()
                            .total_cmp(&b.score.parse::<f64>().unwrap_or_default())
                            .then_with(|| a.id.cmp(&b.id))
                    });
                    if reverse {
                        d.rows.reverse();
                    }
                }
            }
            KeyType::List => {
                d.length = self
                    .run(c, db, vec![b"LLEN".to_vec(), key.to_vec()])?
                    .integer()? as u64;
                let filter = pattern != "*";
                let start = cursor;
                let last = cursor.saturating_add(page as u64).saturating_sub(1).to_string();
                let mut args = vec![b"LRANGE".to_vec(), key.to_vec()];
                args.extend(command(&[&start.to_string(), &last]));
                let v = self.run(c, db, args)?;
                for (i, v) in v.array()?.iter().enumerate() {
                    if !filter || contains_bytes(&v.bytes(), pattern.as_bytes()) {
                        d.rows.push(Entry {
                            id: (start + i as u64).to_string().into_bytes(),
                            value: v.bytes(),
                            ..Default::default()
                        });
                    }
                }
                d.cursor = cursor + v.array()?.len() as u64;
                d.end = v.array()?.len() < page || d.cursor >= d.length;
            }
            KeyType::Stream => {
                d.length = self
                    .run(c, db, vec![b"XLEN".to_vec(), key.to_vec()])?
                    .integer()? as u64;
                let filter = pattern != "*";
                let start = if stream_after.is_empty() {
                    "+".into()
                } else {
                    format!("({stream_after}")
                };
                let mut args = vec![b"XREVRANGE".to_vec(), key.to_vec()];
                args.extend(command(&[&start, "-"]));
                args.extend(command(&["COUNT", &page.to_string()]));
                let v = self.run(c, db, args)?;
                for r in v.array()? {
                    let a = r.array()?;
                    d.stream_cursor = a[0].text();
                    let fields = a[1]
                        .array()?
                        .chunks_exact(2)
                        .map(|p| (p[0].bytes(), p[1].bytes()))
                        .collect::<Vec<_>>();
                    let value = serde_json::to_vec(
                        &fields
                            .iter()
                            .map(|(k, v)| (display_bytes(k), display_bytes(v)))
                            .collect::<std::collections::BTreeMap<_, _>>(),
                    )?;
                    let mut display = Vec::new();
                    for (field, item) in &fields {
                        if !display.is_empty() {
                            display.extend_from_slice(b", ");
                        }
                        display.push(b'"');
                        display.extend_from_slice(field);
                        display.extend_from_slice(b"\":");
                        display.extend_from_slice(item);
                    }
                    if !filter || contains_bytes(&display, pattern.as_bytes()) {
                        d.rows.push(Entry {
                            id: a[0].bytes(),
                            value,
                            fields,
                            ..Default::default()
                        });
                    }
                }
                d.cursor = cursor + v.array()?.len() as u64;
                d.end = v.array()?.len() < page || d.cursor >= d.length;
            }
            _ => bail!("不支持的数据类型：{}", kind.name()),
        }
        Ok(d)
    }
    pub fn set_string(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        value: Vec<u8>,
        json: bool,
    ) -> Result<()> {
        let mut args = if json {
            vec![b"JSON.SET".to_vec(), key.to_vec(), b"$".to_vec(), value]
        } else {
            vec![b"SET".to_vec(), key.to_vec(), value]
        };
        if !json {
            args.push(b"KEEPTTL".to_vec());
        }
        self.run(c, db, args)?;
        Ok(())
    }
    pub fn create(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        kind: KeyType,
        value: &[u8],
        rows: &[Entry],
        ttl: i64,
    ) -> Result<()> {
        if key.is_empty() {
            bail!("键名不能为空");
        }
        let mut args = match kind {
            KeyType::String => vec![b"SET".to_vec(), key.to_vec(), value.to_vec()],
            KeyType::Json => {
                let _: serde_json::Value = serde_json::from_slice(value)?;
                vec![
                    b"JSON.SET".to_vec(),
                    key.to_vec(),
                    b"$".to_vec(),
                    value.to_vec(),
                ]
            }
            KeyType::Hash => {
                let mut a = vec![b"HSET".to_vec(), key.to_vec()];
                for r in rows {
                    a.push(r.id.clone());
                    a.push(r.value.clone());
                }
                a
            }
            KeyType::List | KeyType::Set => {
                let mut a = vec![
                    if kind == KeyType::List {
                        b"LPUSH".to_vec()
                    } else {
                        b"SADD".to_vec()
                    },
                    key.to_vec(),
                ];
                a.extend(rows.iter().map(|r| r.value.clone()));
                a
            }
            KeyType::ZSet => {
                let mut a = vec![b"ZADD".to_vec(), key.to_vec()];
                for r in rows {
                    let _: f64 = r.score.parse()?;
                    a.push(r.score.as_bytes().to_vec());
                    a.push(r.value.clone());
                }
                a
            }
            KeyType::Stream => {
                let mut a = vec![
                    b"XADD".to_vec(),
                    key.to_vec(),
                    if value.is_empty() {
                        b"*".to_vec()
                    } else {
                        value.to_vec()
                    },
                ];
                for r in rows {
                    a.push(r.id.clone());
                    a.push(r.value.clone());
                }
                a
            }
            _ => bail!("无效数据类型"),
        };
        if args.len() < 3 {
            bail!("请添加至少一项数据");
        }
        if kind == KeyType::String && ttl > 0 {
            args.extend(command(&["EX", &ttl.to_string()]));
        }
        self.run(c, db, args)?;
        if kind != KeyType::String && ttl > 0 {
            self.ttl(c, db, key, ttl)?;
        }
        Ok(())
    }
    /// NewKeyDialog's SetKeyValue contract, including empty collection no-ops
    /// and a negative TTL reusing the key's remaining lifetime.
    pub fn create_key(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        kind: KeyType,
        value: &serde_json::Value,
        ttl: i64,
    ) -> Result<()> {
        let expiration = if ttl < 0 {
            let pttl = self.run(c, db, vec![b"PTTL".to_vec(), key.to_vec()])
                .and_then(|value| value.integer()).unwrap_or(-1);
            if pttl < 0 { pttl } else { pttl.wrapping_mul(1_000_000) }
        } else { ttl.wrapping_mul(1_000_000_000) };
        // go-redis formats positive subsecond EXPIRE durations as one second.
        let expire = || -> Result<()> {
            if expiration > 0 { self.ttl(c, db, key, (expiration / 1_000_000_000).max(1))?; }
            Ok(())
        };
        if matches!(kind, KeyType::String | KeyType::Json) {
            let text = value.as_str().ok_or_else(|| anyhow::anyhow!("invalid string value"))?;
            let args = if kind == KeyType::String {
                vec![b"SET".to_vec(), key.to_vec(), text.as_bytes().to_vec()]
            } else {
                vec![b"JSON.SET".to_vec(), key.to_vec(), b".".to_vec(), text.as_bytes().to_vec()]
            };
            self.run(c, db, args)?;
            let _ = expire(); // The original ignores this separate command's error.
            return Ok(());
        }
        let items = value.as_array().ok_or_else(|| anyhow::anyhow!("invalid collection value"))?;
        let bytes = |item: &serde_json::Value| item.as_str().unwrap_or_default().as_bytes().to_vec();
        let mut args = vec![];
        match kind {
            KeyType::Hash => {
                let mut failure = None;
                for pair in items.chunks_exact(2) {
                    if let Err(error) = self.run(c, db, vec![b"HSET".to_vec(), key.to_vec(), bytes(&pair[0]), bytes(&pair[1])]) {
                        failure.get_or_insert(error);
                    }
                }
                if !items.is_empty() && let Err(error) = expire() { failure.get_or_insert(error); }
                if let Some(error) = failure { return Err(error); }
                return Ok(());
            }
            KeyType::List => args.extend([b"LPUSH".to_vec(), key.to_vec()]),
            KeyType::Set => {
                if items.is_empty() { bail!("invalid set value"); }
                args.extend([b"SADD".to_vec(), key.to_vec()]);
            }
            KeyType::ZSet => {
                if items.is_empty() { bail!("invalid zset value"); }
                args.extend([b"ZADD".to_vec(), key.to_vec()]);
                for pair in items.chunks_exact(2) {
                    let score = pair[1].as_str().unwrap_or_default().parse::<f64>().unwrap_or(0.);
                    args.push(if score.is_infinite() { if score.is_sign_positive() { b"+inf".to_vec() } else { b"-inf".to_vec() } } else { score.to_string().into_bytes() });
                    args.push(bytes(&pair[0]));
                }
            }
            KeyType::Stream => {
                if items.len() <= 2 { return Ok(()); }
                args.extend([b"XADD".to_vec(), key.to_vec()]);
            }
            _ => bail!("无效数据类型"),
        }
        if kind != KeyType::ZSet { args.extend(items.iter().map(bytes)); }
        self.run(c, db, args)?;
        let _ = expire();
        Ok(())
    }
    pub fn ttl(&self, c: &Connection, db: u32, key: &[u8], ttl: i64) -> Result<()> {
        let mut args = vec![
            if ttl < 0 {
                b"PERSIST".to_vec()
            } else {
                b"EXPIRE".to_vec()
            },
            key.to_vec(),
        ];
        if ttl >= 0 {
            args.push(ttl.to_string().into_bytes());
        }
        self.run(c, db, args)?;
        Ok(())
    }
    pub fn rename(&self, c: &Connection, db: u32, key: &[u8], new: &[u8]) -> Result<()> {
        self.run(c, db, vec![b"RENAME".to_vec(), key.to_vec(), new.to_vec()])?;
        Ok(())
    }
    pub fn delete(&self, c: &Connection, db: u32, key: &[u8], asynchronous: bool) -> Result<()> {
        self.run(
            c,
            db,
            vec![
                if asynchronous {
                    b"UNLINK".to_vec()
                } else {
                    b"DEL".to_vec()
                },
                key.to_vec(),
            ],
        )?;
        Ok(())
    }
    pub fn edit_entry(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        kind: &KeyType,
        old: &Entry,
        new: &Entry,
        remove: bool,
    ) -> Result<()> {
        let mut args = match kind {
            KeyType::Hash => {
                if remove {
                    vec![b"HDEL".to_vec(), key.to_vec(), old.id.clone()]
                } else {
                    vec![
                        b"HSET".to_vec(),
                        key.to_vec(),
                        new.id.clone(),
                        new.value.clone(),
                    ]
                }
            }
            KeyType::List => {
                if remove {
                    let script=b"local v=redis.call('LINDEX',KEYS[1],ARGV[1]); if not v then return 0 end; local mark=ARGV[2]; redis.call('LSET',KEYS[1],ARGV[1],mark); return redis.call('LREM',KEYS[1],1,mark)";
                    let marker = format!(
                        "\0redisdock-delete-{}-{}",
                        std::process::id(),
                        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
                    )
                    .into_bytes();
                    vec![
                        b"EVAL".to_vec(),
                        script.to_vec(),
                        b"1".to_vec(),
                        key.to_vec(),
                        old.id.clone(),
                        marker,
                    ]
                } else {
                    vec![
                        b"LSET".to_vec(),
                        key.to_vec(),
                        old.id.clone(),
                        new.value.clone(),
                    ]
                }
            }
            KeyType::Set => {
                if remove {
                    vec![b"SREM".to_vec(), key.to_vec(), old.value.clone()]
                } else {
                    vec![b"SADD".to_vec(), key.to_vec(), new.value.clone()]
                }
            }
            KeyType::ZSet => {
                if remove {
                    vec![b"ZREM".to_vec(), key.to_vec(), old.value.clone()]
                } else {
                    let _: f64 = new.score.parse()?;
                    vec![
                        b"ZADD".to_vec(),
                        key.to_vec(),
                        new.score.as_bytes().to_vec(),
                        new.value.clone(),
                    ]
                }
            }
            KeyType::Stream if remove => vec![b"XDEL".to_vec(), key.to_vec(), old.id.clone()],
            _ => bail!("此类型不支持编辑条目"),
        };
        self.run(c, db, std::mem::take(&mut args))?;
        if !remove {
            match kind {
                KeyType::Hash if old.id != new.id => {
                    self.run(c, db, vec![b"HDEL".to_vec(), key.to_vec(), old.id.clone()])?;
                }
                KeyType::Set | KeyType::ZSet if old.value != new.value => {
                    self.run(
                        c,
                        db,
                        vec![
                            if *kind == KeyType::Set {
                                b"SREM".to_vec()
                            } else {
                                b"ZREM".to_vec()
                            },
                            key.to_vec(),
                            old.value.clone(),
                        ],
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn add_entries(
        &self,
        c: &Connection,
        db: u32,
        key: &[u8],
        kind: KeyType,
        rows: &[Entry],
        head: bool,
        only_new: bool,
        id: &str,
    ) -> Result<()> {
        if kind == KeyType::List && head {
            let mut a = vec![b"LPUSH".to_vec(), key.to_vec()];
            a.extend(rows.iter().rev().map(|r| r.value.clone()));
            self.run(c, db, a)?;
        } else if kind == KeyType::List {
            let mut a = vec![b"RPUSH".to_vec(), key.to_vec()];
            a.extend(rows.iter().map(|r| r.value.clone()));
            self.run(c, db, a)?;
        } else if kind == KeyType::Hash && only_new {
            for r in rows {
                self.run(
                    c,
                    db,
                    vec![
                        b"HSETNX".to_vec(),
                        key.to_vec(),
                        r.id.clone(),
                        r.value.clone(),
                    ],
                )?;
            }
        } else if kind == KeyType::ZSet && only_new {
            let mut a = vec![b"ZADD".to_vec(), key.to_vec(), b"NX".to_vec()];
            for r in rows {
                let _: f64 = r.score.parse()?;
                a.push(r.score.as_bytes().to_vec());
                a.push(r.value.clone());
            }
            self.run(c, db, a)?;
        } else {
            self.create(c, db, key, kind, id.as_bytes(), rows, -1)?;
        }
        Ok(())
    }
    /// AddFieldsDialog uses these raw values and consumes the original
    /// added/updated/left/right response before optionally reloading the key.
    pub fn add_key_entries(&self, c:&Connection, db:u32, key:&[u8], kind:KeyType, action:i32, value:&serde_json::Value) -> Result<serde_json::Value> {
        use serde_json::{Value as Json, json};
        if kind == KeyType::ZSet && !value.is_object() {
            bail!("json: cannot unmarshal array into Go value of type map[string]float64");
        }
        let _ = self.client(c,db)?;
        let mut data=serde_json::Map::new();
        let bytes=|value:&Json|value.as_str().unwrap_or_default().as_bytes().to_vec();
        let items=value.as_array().cloned().unwrap_or_default();
        let mut added=vec![];let mut updated=vec![];
        match kind {
            KeyType::Hash => {
                for pair in items.chunks_exact(2) {
                    let count=self.run(c,db,vec![if action==1{b"HSETNX".to_vec()}else{b"HSET".to_vec()},key.to_vec(),bytes(&pair[0]),bytes(&pair[1])]).and_then(|value|value.integer()).unwrap_or(0);
                    let entry=json!({"k":pair[0],"v":pair[1]});
                    if count>0{added.push(entry)}else if action!=1{updated.push(entry)}
                }
            }
            KeyType::List => {
                let mut args=vec![if action==0{b"LPUSH".to_vec()}else{b"RPUSH".to_vec()},key.to_vec()];
                if action==0{args.extend(items.iter().rev().map(bytes))}else{args.extend(items.iter().map(bytes))}
                self.run(c,db,args)?;
                if !items.is_empty(){data.insert(if action==0{"left"}else{"right"}.into(),Json::Array(items.iter().map(|v|json!({"index":0,"v":v})).collect()));}
            }
            KeyType::Set => {
                let mut affected=0;
                for item in &items {
                    affected=self.run(c,db,vec![b"SADD".to_vec(),key.to_vec(),bytes(item)]).and_then(|value|value.integer()).unwrap_or(0);
                    if affected>0{added.push(json!({"v":item}));}
                }
                data.insert("affected".into(),json!(affected));
            }
            KeyType::ZSet => {
                for (member,score) in value.as_object().unwrap() {
                    let score=score.as_f64().unwrap_or(0.);
                    let mut args=vec![b"ZADD".to_vec(),key.to_vec()];
                    if action==1{args.push(b"NX".to_vec());}
                    args.extend([score.to_string().into_bytes(),member.as_bytes().to_vec()]);
                    let count=self.run(c,db,args).and_then(|value|value.integer()).unwrap_or(0);
                    let entry=json!({"v":member,"s":score});
                    if count>0{added.push(entry)}else if action!=1{updated.push(entry)}
                }
            }
            KeyType::Stream => {
                let mut args=vec![b"XADD".to_vec(),key.to_vec()];
                args.extend(items.iter().map(bytes));
                let id=self.run(c,db,args)?.text();
                let fields=&items[items.len().min(1)..];
                let mut values=serde_json::Map::new();
                // Preserve the original response loop: it returns only the
                // first ceil(n/2) fields although Redis receives all n fields.
                for index in (0..fields.len()/2).step_by(2) {
                    values.insert(fields[index].as_str().unwrap_or_default().into(),fields[index+1].clone());
                }
                let display=serde_json::to_string_pretty(&values)?;
                added.push(json!({"id":id,"v":values,"dv":display}));
            }
            _=>bail!("此类型不支持添加元素"),
        }
        if !added.is_empty(){data.insert("added".into(),Json::Array(added));}
        if !updated.is_empty(){data.insert("updated".into(),Json::Array(updated));}
        Ok(Json::Object(data))
    }

    pub fn key_summary(&self,c:&Connection,db:u32,key:&[u8])->Result<KeyDetail>{
        let kind=KeyType::parse(&self.run(c,db,vec![b"TYPE".to_vec(),key.to_vec()])?.text());
        if kind==KeyType::None{return Err(KeyNotFound.into());}
        let ttl=self.run(c,db,vec![b"TTL".to_vec(),key.to_vec()])?.integer()?;
        let memory=self.run(c,db,vec![b"MEMORY".to_vec(),b"USAGE".to_vec(),key.to_vec()]).and_then(|value|value.integer()).unwrap_or(0).max(0) as u64;
        let encoding=self.run(c,db,vec![b"OBJECT".to_vec(),b"ENCODING".to_vec(),key.to_vec()]).map(|value|value.text()).unwrap_or_default();
        let length=match kind {KeyType::Hash=>"HLEN",KeyType::List=>"LLEN",KeyType::Set=>"SCARD",KeyType::ZSet=>"ZCARD",KeyType::Stream=>"XLEN",KeyType::String=>"STRLEN",_=>""};
        let length=if length.is_empty(){0}else{self.run(c,db,vec![length.as_bytes().to_vec(),key.to_vec()])?.integer()?.max(0) as u64};
        Ok(KeyDetail{key:key.to_vec(),kind,ttl,memory,encoding,length,..Default::default()})
    }
    pub fn flush(&self, c: &Connection, db: u32, asynchronous: bool) -> Result<()> {
        let args = command(&["FLUSHDB", if asynchronous { "ASYNC" } else { "SYNC" }]);
        let masters = self.client(c, db)?.masters()?;
        for (h, p) in masters {
            self.run_at(c, db, &h, p, args.clone())?;
        }
        Ok(())
    }
    pub fn slow_logs(&self, c: &Connection, count: usize) -> Result<Vec<SlowLog>> {
        let count = count.max(1);
        let nodes = self.client(c, 0)?.masters()?;
        let mut logs = Vec::new();
        for (host, port) in nodes {
            let v = self.run_at(
                c,
                0,
                &host,
                port,
                command(&["SLOWLOG", "GET", &count.to_string()]),
            )?;
            let mut page = v
                .array()?
                .iter()
                .map(|row| {
                    let a = row.array()?;
                    if a.len() < 4 {
                        bail!("慢日志响应异常");
                    }
                    Ok(SlowLog {
                        id: a[0].integer()?,
                        time: a[1].integer()?,
                        duration: a[2].integer()?,
                        args: a[3]
                            .array()?
                            .iter()
                            .map(Value::text)
                            .collect::<Vec<_>>()
                            .join(" "),
                        address: a.get(4).map(Value::text).unwrap_or_default(),
                        name: a.get(5).map(Value::text).unwrap_or_default(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            logs.append(&mut page);
        }
        logs.sort_by(|a, b| b.time.cmp(&a.time).then(b.id.cmp(&a.id)));
        logs.truncate(count);
        Ok(logs)
    }
    pub fn sentinel_masters(&self, c: &Connection) -> Result<Value> {
        let (h, p) = redis::split_endpoint(c.addr.split(',').next().unwrap_or(&c.addr), c.port)?;
        Wire::open(c, &h, p, 0, true)?
            .query(&command(&["SENTINEL", "MASTERS"]))?
            .checked()
    }
    pub fn export_csv(
        &self,
        c: &Connection,
        db: u32,
        keys: &[Vec<u8>],
        path: &Path,
        include_ttl: bool,
        cancel: &Cancellation,
        mut progress: impl FnMut(TransferProgress),
    ) -> Result<TransferProgress> {
        let temporary = crate::storage::staging_file(path)?;
        let mut w = csv::Writer::from_writer(temporary.as_file());
        let mut report = TransferProgress {
            total: keys.len(),
            ..Default::default()
        };
        let mut emitted = std::time::Instant::now() - std::time::Duration::from_secs(10);
        for (index, key) in keys.iter().enumerate() {
            report.current = index + 1;
            if index + 1 == keys.len() || emitted.elapsed() > std::time::Duration::from_millis(100) {
                emitted = std::time::Instant::now();
                progress(report.clone());
            }
            if cancel.canceled() {
                report.canceled = true;
                break;
            }
            // The original exporter still writes an empty DUMP field for a
            // missing key or non-cancellation Redis error, and counts the CSV write.
            let dump = match self.run(c, db, vec![b"DUMP".to_vec(), key.clone()]) {
                Ok(Value::Bytes(dump)) => dump,
                _ => Vec::new(),
            };
            if cancel.canceled() {report.canceled = true;break;}
            let mut record = vec![hex::encode(key), hex::encode(dump)];
            if include_ttl {
                let ttl = self.run(c, db, vec![b"PTTL".to_vec(), key.clone()])
                    .and_then(|value| value.integer()).unwrap_or(-1);
                record.push(if ttl > 0 {(now_ms() + ttl).to_string()} else {"-1".into()});
            }
            w.write_record(record)?;
            report.done += 1;
        }
        if cancel.canceled() {
            report.canceled = true;
            return Ok(report);
        }
        w.flush()?;
        w.get_ref().sync_all()?;
        drop(w);
        if cancel.canceled() {
            report.canceled = true;
            return Ok(report);
        }
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(report)
    }
    pub fn import_csv(
        &self,
        c: &Connection,
        db: u32,
        path: &Path,
        replace: bool,
        ttl: i64,
        cancel: &Cancellation,
        mut progress: impl FnMut(TransferProgress),
    ) -> Result<TransferProgress> {
        let mut reader = crate::csv_import::Reader::new(std::io::BufReader::new(
            std::fs::File::open(path)?,
        ));
        let mut report = TransferProgress::default();
        let mut emitted = std::time::Instant::now() - std::time::Duration::from_secs(10);
        loop {
            let row = match reader.read_record() {
                Ok(Some(row)) => row,
                Ok(None) => break,
                // encoding/csv stops at the first parse/field-count error.
                Err(_) => {
                    report.total += 1;
                    break;
                }
            };
            report.total += 1;
            if row.len() < 2 {
                continue;
            }
            let (Ok(key), Ok(value)) = (hex::decode(&row[0]), hex::decode(&row[1])) else {
                // The original ignores undecodable records without counting them.
                continue;
            };
            // Source reads and decodes before the canceled Redis call. Its next
            // valid record is counted as ignored; EOF alone is not cancellation.
            if cancel.canceled() {
                report.failed += 1;
                report.canceled = true;
                break;
            }
            let expire = crate::csv_import::restore_ttl_ms(
                ttl,
                row.get(2).map(Vec::as_slice),
                || match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(elapsed) => elapsed.as_nanos() as i128,
                    Err(before_epoch) => -(before_epoch.duration().as_nanos() as i128),
                },
            );
            if !replace && self.run(c, db, vec![b"EXISTS".to_vec(), key.clone()])
                .and_then(|value| value.integer()).unwrap_or(0) > 0 {
                report.skipped += 1;
            } else {
            let mut args = vec![
                b"RESTORE".to_vec(),
                key,
                expire.to_string().into_bytes(),
                value,
            ];
            if replace {
                args.push(b"REPLACE".to_vec());
            }
            match self.run(c, db, args) {
                Ok(_) => report.done += 1,
                Err(_) if expire < 0 => report.skipped += 1,
                Err(e) if e.to_string().contains("BUSYKEY") => report.skipped += 1,
                Err(_) => report.failed += 1,
            }
            }
            if cancel.canceled() {report.canceled = true;break;}
            if emitted.elapsed() > std::time::Duration::from_millis(100) {
                emitted = std::time::Instant::now();
                progress(report.clone());
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        Ok(report)
    }
    pub fn all_keys(
        &self,
        c: &Connection,
        db: u32,
        pattern: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<Vec<u8>>> {
        let mut state = ScanState::default();
        let mut keys = HashSet::new();
        while !state.end && !cancel.canceled() {
            let page = self.scan(c, db, pattern, "", false, state, 3000)?;
            keys.extend(page.keys.into_iter().map(|k| k.key));
            state = page.state;
        }
        Ok(keys.into_iter().collect())
    }
    pub fn batch(
        &self,
        c: &Connection,
        db: u32,
        keys: &[Vec<u8>],
        ttl: Option<i64>,
        cancel: &Cancellation,
        mut progress: impl FnMut(TransferProgress),
    ) -> Result<TransferProgress> {
        let mut r = TransferProgress {
            total: keys.len(),
            ..Default::default()
        };
        let expires_at = ttl
            .filter(|ttl| *ttl >= 0)
            .map(|ttl| (now_ms() / 1000).saturating_add(ttl));
        for k in keys {
            if cancel.canceled() {
                r.canceled = true;
                break;
            }
            let result = if let Some(ttl) = ttl {
                if let Some(expires_at) = expires_at {
                    self.run(
                        c,
                        db,
                        vec![
                            b"EXPIREAT".to_vec(),
                            k.clone(),
                            expires_at.to_string().into_bytes(),
                        ],
                    )
                    .map(|_| ())
                } else {
                    self.ttl(c, db, k, ttl)
                }
            } else {
                self.delete(c, db, k, true)
            };
            match result {
                Ok(_) => r.done += 1,
                Err(_) => r.failed += 1,
            }
            progress(r.clone());
        }
        Ok(r)
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn parse_info(text: &str) -> Info {
    let mut info = Info::new();
    let mut section = "Server".to_owned();
    for line in text.lines() {
        let line = line.trim();
        if let Some(s) = line.strip_prefix("# ") {
            section = s.into();
        } else if let Some((k, v)) = line.split_once(':') {
            info.entry(section.clone())
                .or_default()
                .insert(k.into(), v.into());
        }
    }
    info
}
pub fn info_value(info: &Info, section: &str, key: &str) -> String {
    info.get(section)
        .and_then(|s| s.get(key))
        .cloned()
        .unwrap_or_default()
}
fn contains_bytes(value: &[u8], pattern: &[u8]) -> bool {
    pattern.is_empty() || value.windows(pattern.len()).any(|w| w == pattern)
}

pub fn glob_match(pattern: &[u8], value: &[u8]) -> bool {
    fn run(p: &[u8], v: &[u8], memo: &mut HashMap<(usize, usize), bool>) -> bool {
        let key = (p.len(), v.len());
        if let Some(r) = memo.get(&key) {
            return *r;
        }
        let r = match p.first() {
            None => v.is_empty(),
            Some(b'*') => run(&p[1..], v, memo) || !v.is_empty() && run(p, &v[1..], memo),
            Some(b'?') => !v.is_empty() && run(&p[1..], &v[1..], memo),
            Some(b'\\') if p.len() > 1 => {
                v.first() == p.get(1) && !v.is_empty() && run(&p[2..], &v[1..], memo)
            }
            Some(b'[') => {
                if let Some(end) = p.iter().position(|b| *b == b']') {
                    if v.is_empty() {
                        false
                    } else {
                        let class = &p[1..end];
                        let negate = class.first() == Some(&b'^');
                        let class = if negate { &class[1..] } else { class };
                        let mut matched = false;
                        let mut i = 0;
                        while i < class.len() {
                            if i + 2 < class.len() && class[i + 1] == b'-' {
                                matched |= (class[i]..=class[i + 2]).contains(&v[0]);
                                i += 3;
                            } else {
                                matched |= class[i] == v[0];
                                i += 1;
                            }
                        }
                        matched != negate && run(&p[end + 1..], &v[1..], memo)
                    }
                } else {
                    v.first() == p.first() && !v.is_empty() && run(&p[1..], &v[1..], memo)
                }
            }
            Some(c) => v.first() == Some(c) && !v.is_empty() && run(&p[1..], &v[1..], memo),
        };
        memo.insert(key, r);
        r
    }
    run(pattern, value, &mut HashMap::new())
}
