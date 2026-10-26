//! Binary-safe RESP transport. Each Redis connection belongs to one database.
use crate::model::{Connection, Proxy, display_bytes};
use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    collections::HashMap,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::{IpAddr, TcpStream, ToSocketAddrs},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

mod frame;

pub trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}
impl std::fmt::Debug for dyn ReadWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RedisTransport")
    }
}

struct SshTransport(ssh2::Channel);
impl Read for SshTransport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}
impl Write for SshTransport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        // Channel writes are unbuffered; libssh2's flush discards received replies.
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Error(String),
}
impl Value {
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Bytes(b) => b.clone(),
            Self::Int(n) => n.to_string().into_bytes(),
            Self::Null => vec![],
            _ => self.text().into_bytes(),
        }
    }
    pub fn text(&self) -> String {
        match self {
            Self::Bytes(b) => display_bytes(b),
            Self::Int(n) => n.to_string(),
            Self::Null => "(nil)".into(),
            Self::Error(s) => format!("(error) {s}"),
            Self::Array(a) => a
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{}) {}", i + 1, v.text()))
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
    pub fn integer(&self) -> Result<i64> {
        match self {
            Self::Int(n) => Ok(*n),
            Self::Bytes(b) => Ok(std::str::from_utf8(b)?.parse()?),
            _ => bail!("预期整数响应"),
        }
    }
    pub fn array(&self) -> Result<&[Value]> {
        match self {
            Self::Array(a) => Ok(a),
            Self::Null => Ok(&[]),
            _ => bail!("预期数组响应：{}", self.text()),
        }
    }
    pub fn checked(self) -> Result<Self> {
        if let Self::Error(s) = self {
            bail!("{s}")
        } else {
            Ok(self)
        }
    }
}

pub fn encode_command(args: &[Vec<u8>]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for a in args {
        out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
        out.extend_from_slice(a);
        out.extend_from_slice(b"\r\n");
    }
    out
}
pub fn command(args: &[&str]) -> Vec<Vec<u8>> {
    args.iter().map(|s| s.as_bytes().to_vec()).collect()
}

pub fn read_value(reader: &mut impl BufRead) -> Result<Value> {
    read_nested(reader, 0, false)
}
fn read_nested(reader: &mut impl BufRead, depth: usize, buffered: bool) -> Result<Value> {
    if depth > 128 {
        bail!("RESP 响应嵌套过深");
    }
    let mut prefix = [0u8];
    reader.read_exact(&mut prefix)?;
    let mut line = vec![];
    reader.read_until(b'\n', &mut line)?;
    if !line.ends_with(b"\r\n") {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "RESP 响应行未正确结束").into());
    }
    line.truncate(line.len() - 2);
    match prefix[0] {
        b'+' | b',' | b'(' => Ok(Value::Bytes(line)),
        b'-' => Ok(Value::Error(String::from_utf8_lossy(&line).into_owned())),
        b':' => Ok(Value::Int(std::str::from_utf8(&line)?.parse()?)),
        b'_' => Ok(Value::Null),
        b'#' => Ok(Value::Int(i64::from(line == b"t"))),
        b'$' | b'!' | b'=' => {
            let n: i64 = std::str::from_utf8(&line)?.parse()?;
            if n == -1 {
                return Ok(Value::Null);
            }
            if !(0..=512 * 1024 * 1024).contains(&n) {
                bail!("RESP 字符串长度异常");
            }
            if buffered && reader.fill_buf()?.len() < n as usize + 2 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "等待 RESP 数据").into());
            }
            let mut bytes = vec![0; n as usize];
            reader.read_exact(&mut bytes)?;
            let mut end = [0; 2];
            reader.read_exact(&mut end)?;
            if end != *b"\r\n" {
                bail!("RESP 数据未正确结束");
            }
            if prefix[0] == b'!' {
                Ok(Value::Error(String::from_utf8_lossy(&bytes).into_owned()))
            } else {
                Ok(Value::Bytes(bytes))
            }
        }
        b'*' | b'~' | b'>' | b'%' | b'|' => {
            let mut n: i64 = std::str::from_utf8(&line)?.parse()?;
            if n == -1 {
                return Ok(Value::Null);
            }
            if prefix[0] == b'%' || prefix[0] == b'|' {
                n = n.checked_mul(2).ok_or_else(|| anyhow!("RESP 数组长度异常"))?;
            }
            if !(0..=10_000_000).contains(&n) {
                bail!("RESP 数组长度异常");
            }
            let mut out = Vec::with_capacity((n as usize).min(4096));
            for _ in 0..n {
                out.push(read_nested(reader, depth + 1, buffered)?);
            }
            if prefix[0] == b'|' {
                read_nested(reader, depth + 1, buffered)
            } else {
                Ok(Value::Array(out))
            }
        }
        _ => bail!("未知 RESP 类型：{}", prefix[0]),
    }
}

pub struct Wire {
    reader: BufReader<Box<dyn ReadWrite>>,
    pending: Vec<u8>,
    frame: frame::Frame,
}
impl Wire {
    pub fn open(
        config: &Connection,
        host: &str,
        port: u16,
        db: u32,
        sentinel: bool,
    ) -> Result<Self> {
        let mut wire = Self {
            reader: BufReader::new(transport(config, host, port)?),
            pending: vec![],
            frame: frame::Frame::default(),
        };
        let (user, password) = if config.sentinel.enable && !sentinel {
            (&config.sentinel.username, &config.sentinel.password)
        } else {
            (&config.username, &config.password)
        };
        if !password.is_empty() {
            let args = if user.is_empty() {
                command(&["AUTH", password])
            } else {
                command(&["AUTH", user, password])
            };
            wire.query(&args)?.checked()?;
        }
        if db > 0 && !sentinel {
            wire.query(&command(&["SELECT", &db.to_string()]))?
                .checked()?;
        }
        Ok(wire)
    }
    pub fn query(&mut self, args: &[Vec<u8>]) -> Result<Value> {
        self.send(args)?;
        self.read()
    }
    pub fn send(&mut self, args: &[Vec<u8>]) -> Result<()> {
        let s = self.reader.get_mut();
        s.write_all(&encode_command(args))?;
        s.flush()?;
        Ok(())
    }
    pub fn read(&mut self) -> Result<Value> {
        loop {
            if let Some(used) = self.frame.advance(&self.pending)? {
                let value = read_value(&mut io::Cursor::new(&self.pending[..used]))?;
                self.pending.drain(..used);
                self.frame = frame::Frame::default();
                return Ok(value);
            }
            let mut chunk = [0; 16384];
            let n = self.reader.read(&mut chunk)?;
            if n == 0 {
                return Err(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "Redis 连接已关闭").into(),
                );
            }
            self.pending.extend_from_slice(&chunk[..n]);
        }
    }
    pub fn pipeline(&mut self, commands: &[Vec<Vec<u8>>]) -> Result<Vec<Value>> {
        if commands.is_empty() {
            return Ok(vec![]);
        }
        let s = self.reader.get_mut();
        const CAPACITY: usize = 64 * 1024;
        let mut pending = Vec::with_capacity(CAPACITY);
        for args in commands {
            let encoded = encode_command(args);
            let mut remaining = encoded.as_slice();
            while !remaining.is_empty() {
                let n = remaining.len().min(CAPACITY - pending.len());
                pending.extend_from_slice(&remaining[..n]);
                remaining = &remaining[n..];
                if pending.len() == CAPACITY {
                    s.write_all(&pending)?;
                    pending.clear();
                }
            }
        }
        if !pending.is_empty() {
            s.write_all(&pending)?;
        }
        s.flush()?;
        commands.iter().map(|_| self.read()).collect()
    }
}

pub struct Client {
    pub config: Connection,
    pub db: u32,
    pub primary: (String, u16),
    wires: HashMap<(String, u16), Wire>,
    slots: HashMap<u16, (String, u16)>,
    sentinel_checked: std::time::Instant,
}
fn resolve_sentinel(config: &Connection) -> Result<(String, u16)> {
    let mut resolved = None;
    let mut last_error = anyhow!("未发现哨兵主节点");
    for addr in config.addr.split(',') {
        let result = (|| -> Result<(String, u16)> {
            let (h, p) = split_endpoint(addr.trim(), config.port)?;
            let mut wire = Wire::open(config, &h, p, 0, true)?;
            let value = wire
                .query(&command(&[
                    "SENTINEL",
                    "get-master-addr-by-name",
                    &config.sentinel.master,
                ]))?
                .checked()?;
            let a = value.array()?;
            if a.len() != 2 {
                bail!("哨兵未找到主节点 {}", config.sentinel.master);
            }
            Ok((a[0].text(), a[1].integer()? as u16))
        })();
        match result {
            Ok(addr) => {
                resolved = Some(addr);
                break;
            }
            Err(e) => last_error = e,
        }
    }
    resolved.ok_or(last_error)
}
impl Client {
    pub fn connect(config: &Connection, db: u32) -> Result<Self> {
        if config.cluster.enable && db != 0 {
            bail!("Redis 集群只支持 db0");
        }
        let seed = split_endpoint(
            config.addr.split(',').next().unwrap_or(&config.addr).trim(),
            config.port,
        )?;
        let target = if config.sentinel.enable {
            resolve_sentinel(config)?
        } else {
            seed
        };
        let wire = Wire::open(config, &target.0, target.1, db, false)?;
        let mut wires = HashMap::new();
        wires.insert(target.clone(), wire);
        let mut client = Self {
            config: config.clone(),
            db,
            primary: target,
            wires,
            slots: HashMap::new(),
            sentinel_checked: std::time::Instant::now(),
        };
        client.query(&command(&["PING"]))?;
        if config.cluster.enable {
            client.masters()?;
        }
        Ok(client)
    }
    pub fn query(&mut self, args: &[Vec<u8>]) -> Result<Value> {
        if args.is_empty() {
            bail!("命令不能为空");
        }
        if self.config.sentinel.enable && self.sentinel_checked.elapsed() >= Duration::from_secs(1)
        {
            let _ = self.rediscover_master();
        }
        let op = String::from_utf8_lossy(&args[0]).to_ascii_uppercase();
        let key = match op.as_str() {
            "EVAL" | "EVALSHA" => args.get(3),
            "PING" | "INFO" | "CLIENT" | "COMMAND" | "CONFIG" | "CLUSTER" | "SENTINEL" | "SCAN"
            | "DBSIZE" | "FLUSHDB" | "FLUSHALL" | "AUTH" | "SELECT" | "SLOWLOG" | "MONITOR" => None,
            _ => args.get(1),
        };
        let slot = key.map(|key| hash_slot(key));
        let mut target = slot
            .and_then(|s| self.slots.get(&s).cloned())
            .unwrap_or_else(|| self.primary.clone());
        let mut asking = false;
        for _ in 0..8 {
            if !self.wires.contains_key(&target) {
                match Wire::open(&self.config, &target.0, target.1, self.db, false) {
                    Ok(wire) => {
                        self.wires.insert(target.clone(), wire);
                    }
                    Err(error) => {
                        if self.rediscover_master().unwrap_or(false) {
                            target = self.primary.clone();
                            asking = false;
                            continue;
                        }
                        return Err(error);
                    }
                }
            }
            let wire = self.wires.get_mut(&target).unwrap();
            if asking {
                wire.query(&command(&["ASKING"]))?.checked()?;
            }
            let value = match wire.query(args) {
                Ok(v) => v,
                Err(e) => {
                    self.wires.remove(&target);
                    // A lost reply can follow a successful write. Rediscover for
                    // subsequent commands without replaying an uncertain write.
                    let _ = self.rediscover_master();
                    return Err(e);
                }
            };
            if self.config.sentinel.enable
                && matches!(&value, Value::Error(e) if e.starts_with("READONLY"))
                && self.rediscover_master()?
            {
                target = self.primary.clone();
                asking = false;
                continue;
            }
            if self.config.cluster.enable
                && let Value::Error(ref e) = value
            {
                let parts = e.split_whitespace().collect::<Vec<_>>();
                if parts.len() == 3 && (parts[0] == "MOVED" || parts[0] == "ASK") {
                    target = split_endpoint(parts[2], self.config.port)?;
                    asking = parts[0] == "ASK";
                    if !asking {
                        self.slots.insert(parts[1].parse()?, target.clone());
                    }
                    continue;
                }
            }
            let value = value.checked()?;
            if op == "SELECT"
                && let Some(db) = args
                    .get(1)
                    .and_then(|v| std::str::from_utf8(v).ok())
                    .and_then(|v| v.parse().ok())
            {
                self.db = db;
            }
            return Ok(value);
        }
        bail!("Redis 集群重定向次数过多")
    }
    fn rediscover_master(&mut self) -> Result<bool> {
        if !self.config.sentinel.enable {
            return Ok(false);
        }
        self.sentinel_checked = std::time::Instant::now();
        let target = resolve_sentinel(&self.config)?;
        if target == self.primary {
            return Ok(false);
        }
        self.primary = target;
        self.wires.clear();
        Ok(true)
    }
    pub fn query_at(&mut self, host: &str, port: u16, args: &[Vec<u8>]) -> Result<Value> {
        if self.config.sentinel.enable {
            return self.query(args);
        }
        let addr = (host.to_owned(), port);
        if !self.wires.contains_key(&addr) {
            self.wires.insert(
                addr.clone(),
                Wire::open(&self.config, host, port, self.db, false)?,
            );
        }
        match self.wires.get_mut(&addr).unwrap().query(args) {
            Ok(value) => value.checked(),
            Err(error) => {
                self.wires.remove(&addr);
                Err(error)
            }
        }
    }
    pub fn pipeline_at(
        &mut self,
        host: &str,
        port: u16,
        args: &[Vec<Vec<u8>>],
    ) -> Result<Vec<Value>> {
        let addr = (host.to_owned(), port);
        if !self.wires.contains_key(&addr) {
            self.wires.insert(
                addr.clone(),
                Wire::open(&self.config, host, port, self.db, false)?,
            );
        }
        let values = match self.wires.get_mut(&addr).unwrap().pipeline(args) {
            Ok(values) => values,
            Err(error) => {
                self.wires.remove(&addr);
                return Err(error);
            }
        };
        values
            .into_iter()
            .zip(args)
            .map(|(v, args)| {
                if matches!(&v,Value::Error(s)if s.starts_with("MOVED")||s.starts_with("ASK")) {
                    self.query(args)
                } else {
                    v.checked()
                }
            })
            .collect()
    }
    pub fn key_types(&mut self, keys: &[Vec<u8>], cancel: &Cancellation) -> Result<Vec<Value>> {
        if cancel.canceled() {
            bail!("operation canceled");
        }
        if self.config.sentinel.enable && self.sentinel_checked.elapsed() >= Duration::from_secs(1) {
            let _ = self.rediscover_master();
        }
        let mut groups = HashMap::<_, Vec<_>>::new();
        for (index, key) in keys.iter().enumerate() {
            let target = self.slots.get(&hash_slot(key)).unwrap_or(&self.primary).clone();
            groups.entry(target).or_default().push((index, vec![b"TYPE".to_vec(), key.clone()]));
        }
        let mut result = vec![Value::Null; keys.len()];
        for ((host, port), group) in groups {
            if cancel.canceled() {
                bail!("operation canceled");
            }
            let (indices, commands): (Vec<_>, Vec<_>) = group.into_iter().unzip();
            for (index, value) in indices.into_iter().zip(self.pipeline_at(&host, port, &commands)?) {
                result[index] = value;
            }
        }
        Ok(result)
    }
    pub fn masters(&mut self) -> Result<Vec<(String, u16)>> {
        if !self.config.cluster.enable {
            return Ok(vec![self.primary.clone()]);
        }
        let value = self.query(&command(&["CLUSTER", "SLOTS"]))?;
        let mut masters = vec![];
        self.slots.clear();
        for row in value.array()? {
            let a = row.array()?;
            if a.len() < 3 {
                continue;
            }
            let m = a[2].array()?;
            if m.len() < 2 {
                continue;
            }
            let host = m[0].text();
            let host = if host.is_empty() {
                self.primary.0.clone()
            } else {
                host
            };
            let addr = (host, m[1].integer()? as u16);
            for slot in a[0].integer()?.max(0) as u16..=a[1].integer()?.min(16383) as u16 {
                self.slots.insert(slot, addr.clone());
            }
            if !masters.contains(&addr) {
                masters.push(addr);
            }
        }
        Ok(masters)
    }
    pub fn dedicated(&self) -> Result<Wire> {
        Wire::open(
            &self.config,
            &self.primary.0,
            self.primary.1,
            self.db,
            false,
        )
    }
}

pub fn split_endpoint(text: &str, default_port: u16) -> Result<(String, u16)> {
    if text.starts_with('[')
        && let Some((host, port)) = text.rsplit_once("]:")
    {
        return Ok((host[1..].into(), port.parse()?));
    }
    if text.matches(':').count() == 1
        && let Some((host, port)) = text.rsplit_once(':')
    {
        return Ok((host.into(), port.parse()?));
    }
    Ok((text.trim_matches(['[', ']']).into(), default_port))
}
pub fn hash_slot(key: &[u8]) -> u16 {
    let mut bytes = key;
    if let Some(open) = key.iter().position(|b| *b == b'{')
        && let Some(close) = key[open + 1..].iter().position(|b| *b == b'}')
        && close > 0
    {
        bytes = &key[open + 1..open + 1 + close];
    }
    let mut crc = 0u16;
    for b in bytes {
        crc ^= (*b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc % 16384
}

fn connect_tcp(host: &str, port: u16, connect_secs: u64, exec_secs: u64) -> Result<TcpStream> {
    let host = host.trim_matches(['[', ']']);
    let mut last = None;
    for addr in (host, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(connect_secs.max(1))) {
            Ok(stream) => {
                stream.set_nodelay(true)?;
                stream.set_read_timeout(Some(Duration::from_secs(exec_secs.max(1))))?;
                stream.set_write_timeout(Some(Duration::from_secs(exec_secs.max(1))))?;
                return Ok(stream);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow!("地址无法解析：{host}:{port}")))
}
#[derive(Default)]
struct DialProxy {
    config: Proxy,
    address: String,
    auth: bool,
}
fn proxy_authority(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}
fn effective_proxy(config: &Connection, host: &str) -> DialProxy {
    match config.proxy.kind {
        1 => environment_proxy(host, |name| std::env::var(name).ok()),
        2 => {
            let mut proxy = config.proxy.clone();
            if !matches!(
                proxy.schema.as_str(),
                "http" | "https" | "socks5" | "socks5h"
            ) {
                proxy.schema = "http".into();
            }
            DialProxy {
                address: proxy_authority(&proxy.addr, proxy.port),
                auth: !proxy.username.is_empty(),
                config: proxy,
            }
        }
        _ => DialProxy::default(),
    }
}
fn normalized_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        ip => ip,
    }
}
fn bypass_proxy(host: &str, rules: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let ip = host.parse::<IpAddr>().ok().map(normalized_ip);
    rules
        .split(',')
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .any(|rule| {
            if let Some((address, prefix)) = rule.split_once('/') {
                let (Ok(network), Ok(mut prefix)) =
                    (address.parse::<IpAddr>(), prefix.parse::<u32>())
                else {
                    return false;
                };
                let network = match network {
                    IpAddr::V6(address) if address.to_ipv4_mapped().is_some() && prefix >= 96 => {
                        prefix -= 96;
                        IpAddr::V4(address.to_ipv4_mapped().unwrap())
                    }
                    network => network,
                };
                return match (ip, network) {
                    (Some(IpAddr::V4(ip)), IpAddr::V4(network)) if prefix <= 32 => {
                        let mask = if prefix == 0 {
                            0
                        } else {
                            u32::MAX << (32 - prefix)
                        };
                        u32::from(ip) & mask == u32::from(network) & mask
                    }
                    (Some(IpAddr::V6(ip)), IpAddr::V6(network)) if prefix <= 128 => {
                        let mask = if prefix == 0 {
                            0
                        } else {
                            u128::MAX << (128 - prefix)
                        };
                        u128::from(ip) & mask == u128::from(network) & mask
                    }
                    _ => false,
                };
            }
            if let Ok(address) = rule.parse::<IpAddr>() {
                return ip == Some(normalized_ip(address));
            }
            if ip.is_some() {
                return false;
            }
            if let Some(zone) = rule.strip_prefix("*.") {
                let zone = zone.strip_suffix('.').unwrap_or(zone);
                host == zone || host.ends_with(&format!(".{zone}"))
            } else {
                host == rule.strip_suffix('.').unwrap_or(rule)
            }
        })
}
fn environment_proxy(host: &str, env: impl Fn(&str) -> Option<String>) -> DialProxy {
    let value = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| env(name).filter(|value| !value.is_empty()))
    };
    let Some(raw) = value(&["ALL_PROXY", "all_proxy"]) else {
        return DialProxy::default();
    };
    let Some((scheme, rest)) = raw.split_once(':') else {
        return DialProxy::default();
    };
    let scheme = scheme.to_ascii_lowercase();
    let scheme = scheme.as_str();
    if !matches!(scheme, "http" | "https" | "socks5" | "socks5h") {
        return DialProxy::default();
    }
    // Go's URL parser accepts non-ASCII host escapes and keeps missing HTTP ports.
    // Validate percent escapes before decoding the authority, rather than applying
    // the web URL parser's lowercasing, IDNA and implicit HTTP port defaults.
    let bytes = raw.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit())
        {
            return DialProxy::default();
        }
    }
    let authority = rest
        .strip_prefix("//")
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let (userinfo, address, auth) = if let Some((user, address)) = authority.rsplit_once('@') {
        (user, address, true)
    } else {
        ("", authority, false)
    };
    for (index, byte) in address.as_bytes().iter().enumerate() {
        if *byte == b'%' {
            let decoded = u8::from_str_radix(&address[index + 1..index + 3], 16).unwrap();
            if decoded < 128 && decoded != b'%' {
                return DialProxy::default();
            }
        }
    }
    let address = crate::model::percent_decode(address);
    let (addr, port_text) = if let Some(ip) = address.strip_prefix('[') {
        let Some((ip, tail)) = ip.split_once(']') else {
            return DialProxy::default();
        };
        (
            ip.to_string(),
            tail.strip_prefix(':').unwrap_or("").to_string(),
        )
    } else if let Some((addr, port)) = address.rsplit_once(':') {
        (addr.to_string(), port.to_string())
    } else {
        (address.clone(), String::new())
    };
    if !port_text.is_empty() && !port_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return DialProxy::default();
    }
    let socks = matches!(scheme, "socks5" | "socks5h");
    let port = if port_text.is_empty() && socks {
        1080
    } else {
        port_text.parse().unwrap_or(0)
    };
    let (username, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let rules = value(&["NO_PROXY", "no_proxy"]).unwrap_or_default();
    if bypass_proxy(host, &rules) {
        return DialProxy::default();
    }
    DialProxy {
        address: if socks && port_text.is_empty() {
            proxy_authority(&addr, port)
        } else {
            address
        },
        auth,
        config: Proxy {
            kind: 2,
            schema: scheme.into(),
            addr,
            port,
            username: crate::model::percent_decode(username),
            password: crate::model::percent_decode(password),
        },
    }
}
fn proxy_tcp(config: &Connection, host: &str, port: u16, p: &DialProxy) -> Result<TcpStream> {
    if p.config.kind == 0 {
        return connect_tcp(host, port, config.conn_timeout, config.exec_timeout);
    }
    if !p
        .address
        .rsplit_once(':')
        .is_some_and(|(_, port)| !port.contains(']') && port.parse::<u16>().is_ok())
    {
        bail!(
            "dial tcp: address {}: missing or invalid port in address",
            p.address
        );
    }
    let mut stream = connect_tcp(
        &p.config.addr,
        p.config.port,
        config.conn_timeout,
        config.exec_timeout,
    )?;
    if matches!(p.config.schema.as_str(), "socks5" | "socks5h") {
        socks_connect(&mut stream, host, port, p)?;
    } else {
        http_connect(&mut stream, host, port, p)?;
    }
    Ok(stream)
}
fn socks_connect(s: &mut impl ReadWrite, host: &str, port: u16, proxy: &DialProxy) -> Result<()> {
    let p = &proxy.config;
    s.write_all(if proxy.auth {
        &[5, 2, 0, 2]
    } else {
        &[5, 1, 0]
    })?;
    let mut answer = [0; 2];
    s.read_exact(&mut answer)?;
    if answer[0] != 5 || answer[1] == 255 {
        bail!("SOCKS5 代理验证失败");
    }
    if answer[1] == 2 {
        if p.username.is_empty() || p.username.len() > 255 || p.password.len() > 255 {
            bail!("SOCKS5 用户名或密码过长");
        }
        let mut auth = vec![1, p.username.len() as u8];
        auth.extend_from_slice(p.username.as_bytes());
        auth.push(p.password.len() as u8);
        auth.extend_from_slice(p.password.as_bytes());
        s.write_all(&auth)?;
        s.read_exact(&mut answer)?;
        if answer != [1, 0] {
            bail!("SOCKS5 代理用户名或密码错误");
        }
    } else if answer[1] != 0 {
        bail!("SOCKS5 验证方式不支持");
    }
    let host = host.trim_matches(['[', ']']);
    let mut req = vec![5, 1, 0];
    match host.parse::<IpAddr>().ok().map(normalized_ip) {
        Some(IpAddr::V4(ip)) => {
            req.push(1);
            req.extend_from_slice(&ip.octets());
        }
        Some(IpAddr::V6(ip)) => {
            req.push(4);
            req.extend_from_slice(&ip.octets());
        }
        None => {
            if host.len() > 255 {
                bail!("SOCKS5 地址过长");
            }
            req.extend_from_slice(&[3, host.len() as u8]);
            req.extend_from_slice(host.as_bytes());
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req)?;
    let mut h = [0; 4];
    s.read_exact(&mut h)?;
    if h[0] != 5 || h[2] != 0 {
        bail!("SOCKS5 响应格式异常");
    }
    if h[1] != 0 {
        bail!("SOCKS5 连接失败：{}", h[1]);
    }
    let length = match h[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut n = [0];
            s.read_exact(&mut n)?;
            n[0] as usize
        }
        _ => bail!("SOCKS5 响应地址异常"),
    };
    let mut rest = vec![0; length + 2];
    s.read_exact(&mut rest)?;
    Ok(())
}
fn http_connect(
    stream: &mut impl ReadWrite,
    host: &str,
    port: u16,
    proxy: &DialProxy,
) -> Result<()> {
    let authority = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let p = &proxy.config;
    let auth = if !proxy.auth {
        String::new()
    } else {
        format!(
            "Authorization: Basic {value}\r\nProxy-Authorization: Basic {value}\r\n",
            value = STANDARD.encode(format!("{}:{}", p.username, p.password))
        )
    };
    stream.write_all(
        format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nUser-Agent: Go-http-client/1.1\r\n{auth}\r\n").as_bytes(),
    )?;
    stream.flush()?;
    let mut header = vec![];
    while !header.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b)?;
        header.push(b[0]);
        if header.len() > 16384 {
            bail!("代理响应头过大");
        }
    }
    let first = String::from_utf8_lossy(&header);
    if first.split_whitespace().nth(1) != Some("200") {
        bail!(
            "HTTP 代理连接失败：{}",
            first.lines().next().unwrap_or_default()
        );
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct SshHostKeyError {
    pub observed: crate::model::SshHostKey,
    pub expected: Option<crate::model::SshHostKey>,
}
impl std::fmt::Display for SshHostKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = &self.observed;
        if let Some(old) = &self.expected {
            write!(f, "SSH host key changed for {}:{} ({}). Connection blocked.\nExpected: {}\nReceived: {}", key.host, key.port, key.algorithm, old.fingerprint, key.fingerprint)
        } else {
            write!(f, "Unknown SSH host {}:{} ({}) {}", key.host, key.port, key.algorithm, key.fingerprint)
        }
    }
}
impl std::error::Error for SshHostKeyError {}

fn transport(config: &Connection, host: &str, port: u16) -> Result<Box<dyn ReadWrite>> {
    let proxy = effective_proxy(
        config,
        if config.ssh.enable {
            &config.ssh.addr
        } else {
            host
        },
    );
    let mut stream: Box<dyn ReadWrite> = if config.ssh.enable {
        let ssh = &config.ssh;
        let tcp = proxy_tcp(config, &ssh.addr, ssh.port, &proxy)?;
        tcp.set_read_timeout(None)?;
        tcp.set_write_timeout(None)?;
        let mut session = ssh2::Session::new()?;
        session.set_tcp_stream(tcp);
        session.set_timeout(config.conn_timeout.max(1).saturating_mul(1000).min(u32::MAX as u64) as u32);
        session.handshake()?;
        let (_, kind) = session.host_key().ok_or_else(|| anyhow!("SSH server did not provide a host key"))?;
        let hash = session.host_key_hash(ssh2::HashType::Sha256).ok_or_else(|| anyhow!("SSH host key fingerprint unavailable"))?;
        let observed = crate::model::SshHostKey {
            host: ssh.addr.clone(), port: ssh.port, algorithm: format!("{kind:?}"),
            fingerprint: format!("SHA256:{}", base64::engine::general_purpose::STANDARD_NO_PAD.encode(hash)),
        };
        let expected = ssh.trusted_host.as_ref().filter(|key| key.host == ssh.addr && key.port == ssh.port);
        if expected != Some(&observed) {
            return Err(SshHostKeyError { observed, expected: expected.cloned() }.into());
        }
        match ssh.login_type.as_str() {
            "agent" => session.userauth_agent(&ssh.username)?,
            "pkfile" => session.userauth_pubkey_file(
                &ssh.username,
                None,
                Path::new(&ssh.pk_file),
                if ssh.passphrase.is_empty() {
                    None
                } else {
                    Some(&ssh.passphrase)
                },
            )?,
            _ => session.userauth_password(&ssh.username, &ssh.password)?,
        }
        if !session.authenticated() {
            bail!("SSH 验证失败");
        }
        session.set_timeout(config.exec_timeout.max(1).saturating_mul(1000).min(u32::MAX as u64) as u32);
        if config.network == "unix" {
            let socket = if config.sock.is_empty() {
                "/tmp/redis.sock"
            } else {
                &config.sock
            };
            Box::new(SshTransport(session.channel_direct_streamlocal(socket, None)?))
        } else {
            Box::new(SshTransport(session.channel_direct_tcpip(host, port, None)?))
        }
    } else if config.network == "unix" {
        #[cfg(unix)]
        {
            if proxy.config.kind != 0 {
                // The original forwards the requested network to its proxy dialer.
                // A TCP proxy address therefore remains a Unix socket address here.
                let _ = std::os::unix::net::UnixStream::connect(&proxy.address)
                    .map_err(|error| anyhow!("dial unix {}: {error}", proxy.address))?;
                bail!("proxy cannot forward a Unix socket without an SSH tunnel");
            }
            let socket = if config.sock.is_empty() {
                "/tmp/redis.sock"
            } else {
                &config.sock
            };
            let stream = std::os::unix::net::UnixStream::connect(socket)?;
            stream.set_read_timeout(Some(Duration::from_secs(config.exec_timeout.max(1))))?;
            stream.set_write_timeout(Some(Duration::from_secs(config.exec_timeout.max(1))))?;
            Box::new(stream)
        }
        #[cfg(not(unix))]
        {
            bail!("此系统不支持 UNIX 套接字");
        }
    } else {
        let tcp = proxy_tcp(config, host, port, &proxy)?;
        Box::new(tcp)
    };
    if config.ssl.enable {
        let mut b = native_tls::TlsConnector::builder();
        b.min_protocol_version(Some(native_tls::Protocol::Tlsv12));
        b.danger_accept_invalid_certs(config.ssl.allow_insecure)
            .danger_accept_invalid_hostnames(config.ssl.allow_insecure);
        if !config.ssl.ca_file.is_empty() {
            b.disable_built_in_roots(true);
            let pem = fs::read_to_string(&config.ssl.ca_file)?;
            for certificate in pem
                .split_inclusive("-----END CERTIFICATE-----")
                .filter(|s| s.contains("-----BEGIN CERTIFICATE-----"))
            {
                b.add_root_certificate(native_tls::Certificate::from_pem(
                    certificate.trim().as_bytes(),
                )?);
            }
        }
        if !config.ssl.cert_file.is_empty() && !config.ssl.key_file.is_empty() {
            let mut key = fs::read(&config.ssl.key_file)?;
            if !key.starts_with(b"-----BEGIN PRIVATE KEY-----") {
                use std::process::{Command, Stdio};
                let mut child = Command::new("openssl")
                    .args(["pkcs8", "-topk8", "-nocrypt"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?;
                child.stdin.take().unwrap().write_all(&key)?;
                let output = child.wait_with_output()?;
                if !output.status.success() {
                    bail!("无法将 TLS 私钥转换为 PKCS#8 格式");
                }
                key = output.stdout;
            }
            b.identity(native_tls::Identity::from_pkcs8(
                &fs::read(&config.ssl.cert_file)?,
                &key,
            )?);
        }
        let sni = if config.ssl.sni.trim().is_empty() {
            host
        } else {
            config.ssl.sni.trim()
        };
        stream = Box::new(
            b.build()?
                .connect(sni, stream)
                .map_err(|e| anyhow!("TLS 连接失败：{e}"))?,
        );
    }
    Ok(stream)
}
#[derive(Clone, Default)]
pub struct Cancellation(pub Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn canceled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

pub fn parse_command(line: &str) -> Result<Vec<Vec<u8>>> {
    let mut args = vec![];
    let mut part = vec![];
    let mut quote = None;
    let mut started = false;
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\\' && quote != Some(b'\'') {
            i += 1;
            if i >= b.len() {
                bail!("命令以无效转义结束");
            }
            let v = match b[i] {
                b'n' => b'\n',
                b'r' => b'\r',
                b't' => b'\t',
                b'x' if i + 2 < b.len() => {
                    let v = u8::from_str_radix(&line[i + 1..i + 3], 16)?;
                    i += 2;
                    v
                }
                other => other,
            };
            part.push(v);
            started = true;
        } else if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                part.push(c);
            }
        } else if c == b'\'' || c == b'"' {
            quote = Some(c);
            started = true;
        } else if c.is_ascii_whitespace() {
            if started {
                args.push(std::mem::take(&mut part));
                started = false;
            }
        } else {
            part.push(c);
            started = true;
        }
        i += 1;
    }
    if quote.is_some() {
        bail!("引号未闭合");
    }
    if started {
        args.push(part);
    }
    Ok(args)
}

pub fn is_timeout(error: &anyhow::Error) -> bool {
    error.downcast_ref::<io::Error>().is_some_and(|e| {
        matches!(
            e.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) || (e.kind() == io::ErrorKind::Other && e.to_string() == "Timed out waiting on socket")
        // ssh2 maps LIBSSH2_ERROR_SOCKET_TIMEOUT to Other and retains only its message.
    })
}

#[cfg(test)]
mod pipeline_tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Record { bytes: Vec<u8>, calls: Vec<usize>, reads: usize, flushes: usize }
    struct Transport {
        record: Arc<Mutex<Record>>,
        response: io::Cursor<Vec<u8>>,
        short: usize,
        fail_after: usize,
    }
    impl Read for Transport {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.record.lock().unwrap().reads += 1;
            self.response.read(b)
        }
    }
    impl Write for Transport {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut record = self.record.lock().unwrap();
            record.calls.push(bytes.len());
            if record.bytes.len() >= self.fail_after { return Err(io::ErrorKind::BrokenPipe.into()); }
            let n = bytes.len().min(self.short).min(self.fail_after - record.bytes.len());
            record.bytes.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> { self.record.lock().unwrap().flushes += 1; Ok(()) }
    }
    fn wire(short: usize, fail_after: usize, response: Vec<u8>) -> (Wire, Arc<Mutex<Record>>) {
        let record = Arc::new(Mutex::new(Record::default()));
        (Wire { reader: BufReader::new(Box::new(Transport { record: record.clone(), response: io::Cursor::new(response), short, fail_after })), pending: vec![], frame: frame::Frame::default() }, record)
    }
    #[test]
    fn pipeline_coalesces_bounded_writes_and_handles_short_writes() {
        let commands = (0..3000).map(|i| command(&["ECHO", &format!("binary\0:{i}")])).collect::<Vec<_>>();
        let expected = commands.iter().flat_map(|args| encode_command(args)).collect::<Vec<_>>();
        let replies = (0..3000).map(|i| format!(":{i}\r\n")).collect::<String>().into_bytes();
        for short in [usize::MAX, 7] {
            let (mut wire, record) = wire(short, usize::MAX, replies.clone());
            assert_eq!(wire.pipeline(&commands).unwrap(), (0..3000).map(Value::Int).collect::<Vec<_>>());
            let r = record.lock().unwrap();
            assert_eq!(r.bytes, expected);
            assert_eq!(r.flushes, 1);
            assert!(r.calls.iter().all(|&n| n <= 65536));
            if short == usize::MAX { assert_eq!(r.calls.len(), expected.len().div_ceil(65536)); }
        }
    }
    #[test]
    fn failed_pipeline_writes_are_not_retried_or_read() {
        let (mut wire, record) = wire(7, 13, vec![]);
        assert!(wire.pipeline(&[command(&["INCR", "counter"])]).is_err());
        drop(wire);
        let r = record.lock().unwrap();
        assert_eq!(r.bytes.len(), 13);
        assert_eq!(r.calls.len(), 3);
        assert_eq!(r.reads, 0);
        assert_eq!(r.flushes, 0);
    }
}

#[cfg(test)]
mod proxy_tests {
    use super::*;
    use serde_json::Value as Json;
    struct Duplex {
        response: io::Cursor<Vec<u8>>,
        written: Vec<u8>,
    }
    impl Read for Duplex {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.response.read(b)
        }
    }
    impl Write for Duplex {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    fn proxy(case: &Json, host: &str) -> DialProxy {
        if case["mode"] == "system" {
            environment_proxy(host, |name| case["env"][name].as_str().map(str::to_string))
        } else {
            effective_proxy(
                &Connection {
                    proxy: Proxy {
                        kind: 2,
                        schema: case["schema"].as_str().unwrap().into(),
                        addr: "proxy.example".into(),
                        port: 3128,
                        username: case["username"].as_str().unwrap().into(),
                        password: case["password"].as_str().unwrap().into(),
                    },
                    ..Default::default()
                },
                host,
            )
        }
    }
    fn target(case: &Json) -> (&str, u16) {
        let (host, port) = case["target"].as_str().unwrap().rsplit_once(':').unwrap();
        (host.trim_matches(['[', ']']), port.parse().unwrap())
    }
    fn oracle() -> Json {
        serde_json::from_str(include_str!("../tests/fixtures/proxy-source.json")).unwrap()
    }
    #[test]
    fn original_go_proxy_environment_routes() {
        let oracle = oracle();
        for record in oracle["records"].as_array().unwrap() {
            let case = &record["case"];
            let (host, _) = target(case);
            let proxy = proxy(case, host);
            let forward = if proxy.config.kind == 0 {
                case["target"].as_str().unwrap()
            } else {
                &proxy.address
            };
            assert_eq!(
                forward,
                record["forward"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn original_go_http_and_socks_wire_messages() {
        let oracle = oracle();
        for record in oracle["records"].as_array().unwrap() {
            let case = &record["case"];
            let (host, port) = target(case);
            let proxy = proxy(case, host);
            if proxy.config.kind == 0 {
                continue;
            }
            let mut wire = Duplex {
                response: io::Cursor::new(vec![]),
                written: vec![],
            };
            if let Some(expected) = record["http"].as_str() {
                wire.response = io::Cursor::new(
                    b"HTTP/1.1 200 Connection Established\r\nContent-Length: 0\r\n\r\n".to_vec(),
                );
                http_connect(&mut wire, host, port, &proxy).unwrap();
                assert_eq!(
                    String::from_utf8(wire.written).unwrap(),
                    expected,
                    "{}",
                    case["name"]
                );
            } else if record["greeting"].is_string() {
                let method = case["method"].as_u64().unwrap() as u8;
                let mut response = vec![5, method];
                if method == 2 {
                    response.extend_from_slice(&[1, 0]);
                }
                response.extend_from_slice(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]);
                wire.response = io::Cursor::new(response);
                socks_connect(&mut wire, host, port, &proxy).unwrap();
                let expected = format!(
                    "{}{}{}",
                    record["greeting"].as_str().unwrap(),
                    record["auth"].as_str().unwrap_or(""),
                    record["request"].as_str().unwrap()
                );
                assert_eq!(hex::encode(wire.written), expected, "{}", case["name"]);
            }
        }
    }
}
