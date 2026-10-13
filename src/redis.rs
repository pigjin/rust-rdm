//! Binary-safe RESP transport. Each Redis connection belongs to one database.
use crate::model::{Connection, Proxy, display_bytes};
use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{collections::HashMap, fs, io::{self, BufRead, BufReader, Read, Write}, net::{TcpStream, ToSocketAddrs}, path::Path, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};

pub trait ReadWrite: Read + Write + Send {}
impl<T: Read + Write + Send> ReadWrite for T {}
impl std::fmt::Debug for dyn ReadWrite { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str("RedisTransport") } }

#[derive(Clone, Debug, PartialEq)]
pub enum Value { Null, Int(i64), Bytes(Vec<u8>), Array(Vec<Value>), Error(String) }
impl Value {
    pub fn bytes(&self) -> Vec<u8> { match self { Self::Bytes(b)=>b.clone(), Self::Int(n)=>n.to_string().into_bytes(), Self::Null=>vec![], _=>self.text().into_bytes() } }
    pub fn text(&self) -> String { match self { Self::Bytes(b)=>display_bytes(b), Self::Int(n)=>n.to_string(), Self::Null=>"(nil)".into(), Self::Error(s)=>format!("(error) {s}"), Self::Array(a)=>a.iter().enumerate().map(|(i,v)|format!("{}) {}",i+1,v.text())).collect::<Vec<_>>().join("\n") } }
    pub fn integer(&self) -> Result<i64> { match self { Self::Int(n)=>Ok(*n), Self::Bytes(b)=>Ok(std::str::from_utf8(b)?.parse()?), _=>bail!("预期整数响应") } }
    pub fn array(&self) -> Result<&[Value]> { match self { Self::Array(a)=>Ok(a), Self::Null=>Ok(&[]), _=>bail!("预期数组响应：{}",self.text()) } }
    pub fn checked(self) -> Result<Self> { if let Self::Error(s)=self { bail!("{s}") } else { Ok(self) } }
}

pub fn encode_command(args: &[Vec<u8>]) -> Vec<u8> {
    let mut out=format!("*{}\r\n",args.len()).into_bytes();
    for a in args { out.extend_from_slice(format!("${}\r\n",a.len()).as_bytes()); out.extend_from_slice(a); out.extend_from_slice(b"\r\n"); }
    out
}
pub fn command(args: &[&str]) -> Vec<Vec<u8>> { args.iter().map(|s|s.as_bytes().to_vec()).collect() }

pub fn read_value(reader: &mut impl BufRead) -> Result<Value> { read_nested(reader,0) }
fn read_nested(reader: &mut impl BufRead, depth: usize) -> Result<Value> {
    if depth>128 { bail!("RESP 响应嵌套过深"); }
    let mut prefix=[0u8]; reader.read_exact(&mut prefix)?;
    let mut line=vec![];
    reader.read_until(b'\n', &mut line)?;
    if !line.ends_with(b"\r\n") { bail!("RESP 响应行未正确结束"); }
    line.truncate(line.len()-2);
    match prefix[0] {
        b'+'|b','|b'(' => Ok(Value::Bytes(line)),
        b'-'=>Ok(Value::Error(String::from_utf8_lossy(&line).into_owned())),
        b':'=>Ok(Value::Int(std::str::from_utf8(&line)?.parse()?)),
        b'_'=>Ok(Value::Null), b'#'=>Ok(Value::Int(i64::from(line==b"t"))),
        b'$'|b'!'|b'='=>{
            let n:i64=std::str::from_utf8(&line)?.parse()?;
            if n == -1 { return Ok(Value::Null); }
            if !(0..=512*1024*1024).contains(&n) { bail!("RESP 字符串长度异常"); }
            let mut bytes=vec![0;n as usize]; reader.read_exact(&mut bytes)?;
            let mut end=[0;2]; reader.read_exact(&mut end)?; if end!=*b"\r\n" { bail!("RESP 数据未正确结束"); }
            if prefix[0]==b'!' { Ok(Value::Error(String::from_utf8_lossy(&bytes).into_owned())) } else { Ok(Value::Bytes(bytes)) }
        },
        b'*'|b'~'|b'>'|b'%'|b'|'=>{
            let mut n:i64=std::str::from_utf8(&line)?.parse()?;
            if n == -1 { return Ok(Value::Null); }
            if prefix[0]==b'%' || prefix[0]==b'|' { n*=2; }
            if !(0..=10_000_000).contains(&n) { bail!("RESP 数组长度异常"); }
            let mut out=Vec::with_capacity(n as usize);
            for _ in 0..n { out.push(read_nested(reader,depth+1)?); }
            if prefix[0]==b'|' { read_nested(reader,depth+1) } else { Ok(Value::Array(out)) }
        },
        _=>bail!("未知 RESP 类型：{}",prefix[0]),
    }
}

pub struct Wire { reader: BufReader<Box<dyn ReadWrite>> }
impl Wire {
    pub fn open(config: &Connection, host: &str, port: u16, db: u32, sentinel: bool) -> Result<Self> {
        let mut wire=Self { reader:BufReader::new(transport(config,host,port)?) };
        let (user,password)=if sentinel { (&config.sentinel.username,&config.sentinel.password) } else { (&config.username,&config.password) };
        if !password.is_empty() {
            let args=if user.is_empty() { command(&["AUTH",password]) } else { command(&["AUTH",user,password]) };
            wire.query(&args)?.checked()?;
        }
        if db>0 && !sentinel { wire.query(&command(&["SELECT",&db.to_string()]))?.checked()?; }
        Ok(wire)
    }
    pub fn query(&mut self,args:&[Vec<u8>]) -> Result<Value> { self.send(args)?; self.read() }
    pub fn send(&mut self,args:&[Vec<u8>]) -> Result<()> { let s=self.reader.get_mut(); s.write_all(&encode_command(args))?; s.flush()?; Ok(()) }
    pub fn read(&mut self) -> Result<Value> { read_value(&mut self.reader) }
}

pub struct Client {
    pub config: Connection,
    pub db: u32,
    pub primary: (String,u16),
    wires: HashMap<(String,u16),Wire>,
    slots: HashMap<u16,(String,u16)>,
}
impl Client {
    pub fn connect(config: &Connection,db:u32) -> Result<Self> {
        if config.cluster.enable && db!=0 { bail!("Redis 集群只支持 db0"); }
        let seed=split_endpoint(config.addr.split(',').next().unwrap_or(&config.addr).trim(),config.port)?;
        let target=if config.sentinel.enable {
            let mut resolved=None; let mut last_error=anyhow!("未发现哨兵主节点");
            for addr in config.addr.split(',') {
                let result=(||->Result<(String,u16)>{
                    let (h,p)=split_endpoint(addr.trim(),config.port)?;
                    let mut wire=Wire::open(config,&h,p,0,true)?;
                    let value=wire.query(&command(&["SENTINEL","get-master-addr-by-name",&config.sentinel.master]))?.checked()?;
                    let a=value.array()?; if a.len()!=2 { bail!("哨兵未找到主节点 {}",config.sentinel.master); }
                    Ok((a[0].text(),a[1].integer()? as u16))
                })();
                match result { Ok(addr)=>{ resolved=Some(addr); break; }, Err(e)=>last_error=e }
            }
            resolved.ok_or(last_error)?
        } else { seed };
        let wire=Wire::open(config,&target.0,target.1,db,false)?;
        let mut wires=HashMap::new(); wires.insert(target.clone(),wire);
        let mut client=Self { config:config.clone(),db,primary:target,wires,slots:HashMap::new() };
        client.query(&command(&["PING"]))?;
        Ok(client)
    }
    pub fn query(&mut self,args:&[Vec<u8>]) -> Result<Value> {
        if args.is_empty() { bail!("命令不能为空"); }
        let slot=args.get(1).map(|key|hash_slot(key));
        let mut target=slot.and_then(|s|self.slots.get(&s).cloned()).unwrap_or_else(||self.primary.clone());
        let mut asking=false;
        for _ in 0..8 {
            if !self.wires.contains_key(&target) { self.wires.insert(target.clone(),Wire::open(&self.config,&target.0,target.1,self.db,false)?); }
            let wire=self.wires.get_mut(&target).unwrap();
            if asking { wire.query(&command(&["ASKING"]))?.checked()?; }
            let value=match wire.query(args) { Ok(v)=>v,Err(e)=>{ self.wires.remove(&target); return Err(e); } };
            if self.config.cluster.enable && let Value::Error(ref e)=value {
                let parts=e.split_whitespace().collect::<Vec<_>>();
                if parts.len()==3 && (parts[0]=="MOVED" || parts[0]=="ASK") {
                    target=split_endpoint(parts[2],self.config.port)?;
                    asking=parts[0]=="ASK";
                    if !asking { self.slots.insert(parts[1].parse()?,target.clone()); }
                    continue;
                }
            }
            return value.checked();
        }
        bail!("Redis 集群重定向次数过多")
    }
    pub fn query_at(&mut self,host:&str,port:u16,args:&[Vec<u8>]) -> Result<Value> {
        let addr=(host.to_owned(),port);
        if !self.wires.contains_key(&addr) { self.wires.insert(addr.clone(),Wire::open(&self.config,host,port,self.db,false)?); }
        self.wires.get_mut(&addr).unwrap().query(args)?.checked()
    }
    pub fn masters(&mut self) -> Result<Vec<(String,u16)>> {
        if !self.config.cluster.enable { return Ok(vec![self.primary.clone()]); }
        let value=self.query(&command(&["CLUSTER","SLOTS"]))?;
        let mut masters=vec![];
        for row in value.array()? {
            let a=row.array()?; if a.len()<3 { continue; }
            let m=a[2].array()?; if m.len()<2 { continue; }
            let host=m[0].text(); let host=if host.is_empty() { self.primary.0.clone() } else { host };
            let addr=(host,m[1].integer()? as u16);
            if !masters.contains(&addr) { masters.push(addr); }
        }
        Ok(masters)
    }
    pub fn dedicated(&self) -> Result<Wire> { Wire::open(&self.config,&self.primary.0,self.primary.1,self.db,false) }
}

pub fn split_endpoint(text:&str,default_port:u16) -> Result<(String,u16)> {
    if text.starts_with('[') && let Some((host,port))=text.rsplit_once("]:") { return Ok((host[1..].into(),port.parse()?)); }
    if text.matches(':').count()==1 && let Some((host,port))=text.rsplit_once(':') { return Ok((host.into(),port.parse()?)); }
    Ok((text.trim_matches(['[',']']).into(),default_port))
}
pub fn hash_slot(key:&[u8]) -> u16 {
    let mut bytes=key;
    if let Some(open)=key.iter().position(|b|*b==b'{')
        && let Some(close)=key[open+1..].iter().position(|b|*b==b'}') && close>0 { bytes=&key[open+1..open+1+close]; }
    let mut crc=0u16;
    for b in bytes { crc^=(*b as u16)<<8; for _ in 0..8 { crc=if crc & 0x8000!=0 { (crc<<1)^0x1021 } else { crc<<1 }; } }
    crc % 16384
}

fn connect_tcp(host:&str,port:u16,connect_secs:u64,exec_secs:u64) -> Result<TcpStream> {
    let mut last=None;
    for addr in (host,port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr,Duration::from_secs(connect_secs.max(1))) {
            Ok(stream)=>{ stream.set_nodelay(true)?; stream.set_read_timeout(Some(Duration::from_secs(exec_secs.max(1))))?; stream.set_write_timeout(Some(Duration::from_secs(exec_secs.max(1))))?; return Ok(stream); },
            Err(e)=>last=Some(e),
        }
    }
    Err(last.map(anyhow::Error::from).unwrap_or_else(||anyhow!("地址无法解析：{host}:{port}")))
}
fn effective_proxy(config:&Connection) -> Result<Proxy> {
    if config.proxy.kind!=1 { return Ok(config.proxy.clone()); }
    for name in ["ALL_PROXY","all_proxy","HTTPS_PROXY","https_proxy","HTTP_PROXY","http_proxy"] {
        if let Ok(s)=std::env::var(name) && !s.is_empty() {
            let url=url::Url::parse(&s)?;
            return Ok(Proxy { kind:2,schema:url.scheme().into(),addr:url.host_str().unwrap_or_default().into(),port:url.port_or_known_default().unwrap_or(1080),username:url.username().into(),password:url.password().unwrap_or_default().into() });
        }
    }
    Ok(Proxy::default())
}
fn proxy_tcp(config:&Connection,host:&str,port:u16,p:&Proxy) -> Result<TcpStream> {
    if p.kind==0 { return connect_tcp(host,port,config.conn_timeout,config.exec_timeout); }
    let mut stream=connect_tcp(&p.addr,p.port,config.conn_timeout,config.exec_timeout)?;
    if p.schema.starts_with("socks5") {
        socks_connect(&mut stream,host,port,p)?;
    } else if p.schema!="https" { http_connect(&mut stream,host,port,p)?; }
    Ok(stream)
}
fn socks_connect(s:&mut TcpStream,host:&str,port:u16,p:&Proxy)->Result<()> {
    s.write_all(if p.username.is_empty(){&[5,1,0]}else{&[5,1,2]})?;
    let mut answer=[0;2];s.read_exact(&mut answer)?;
    if answer[0]!=5||answer[1]==255 {bail!("SOCKS5 代理验证失败");}
    if answer[1]==2 {
        if p.username.len()>255||p.password.len()>255 {bail!("SOCKS5 用户名或密码过长");}
        let mut auth=vec![1,p.username.len() as u8];auth.extend_from_slice(p.username.as_bytes());auth.push(p.password.len() as u8);auth.extend_from_slice(p.password.as_bytes());s.write_all(&auth)?;s.read_exact(&mut answer)?;
        if answer[1]!=0 {bail!("SOCKS5 代理用户名或密码错误");}
    } else if answer[1]!=0 {bail!("SOCKS5 验证方式不支持");}
    if host.len()>255 {bail!("SOCKS5 地址过长");}
    let mut req=vec![5,1,0,3,host.len() as u8];req.extend_from_slice(host.as_bytes());req.extend_from_slice(&port.to_be_bytes());s.write_all(&req)?;
    let mut h=[0;4];s.read_exact(&mut h)?;if h[1]!=0 {bail!("SOCKS5 连接失败：{}",h[1]);}
    let length=match h[3] {1=>4,4=>16,3=>{let mut n=[0];s.read_exact(&mut n)?;n[0] as usize},_=>bail!("SOCKS5 响应地址异常")};let mut rest=vec![0;length+2];s.read_exact(&mut rest)?;Ok(())
}
fn http_connect(stream:&mut impl ReadWrite,host:&str,port:u16,p:&Proxy) -> Result<()> {
    let auth=if p.username.is_empty() { String::new() } else { format!("Proxy-Authorization: Basic {}\r\n",STANDARD.encode(format!("{}:{}",p.username,p.password))) };
    stream.write_all(format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n{auth}\r\n").as_bytes())?; stream.flush()?;
    let mut header=vec![];
    while !header.ends_with(b"\r\n\r\n") { let mut b=[0]; stream.read_exact(&mut b)?; header.push(b[0]); if header.len()>16384 { bail!("代理响应头过大"); } }
    let first=String::from_utf8_lossy(&header); if first.split_whitespace().nth(1)!=Some("200") { bail!("HTTP 代理连接失败：{}",first.lines().next().unwrap_or_default()); }
    Ok(())
}
fn transport(config:&Connection,host:&str,port:u16) -> Result<Box<dyn ReadWrite>> {
    let proxy=effective_proxy(config)?;
    let mut stream:Box<dyn ReadWrite>=if config.network=="unix" {
        #[cfg(unix)] { let s=std::os::unix::net::UnixStream::connect(&config.sock)?; s.set_read_timeout(Some(Duration::from_secs(config.exec_timeout.max(1))))?; s.set_write_timeout(Some(Duration::from_secs(config.exec_timeout.max(1))))?; Box::new(s) }
        #[cfg(not(unix))] { bail!("此系统不支持 UNIX 套接字"); }
    } else if config.ssh.enable {
        let ssh=&config.ssh;
        if proxy.schema=="https" { bail!("SSH 通过 HTTPS 代理的组合暂不支持"); }
        let tcp=proxy_tcp(config,&ssh.addr,ssh.port,&proxy)?;
        let mut session=ssh2::Session::new()?; session.set_tcp_stream(tcp); session.set_timeout((config.conn_timeout*1000).min(u32::MAX as u64) as u32); session.handshake()?;
        match ssh.login_type.as_str() {
            "agent"=>session.userauth_agent(&ssh.username)?,
            "pkfile"=>session.userauth_pubkey_file(&ssh.username,None,Path::new(&ssh.pk_file),if ssh.passphrase.is_empty() { None } else { Some(&ssh.passphrase) })?,
            _=>session.userauth_password(&ssh.username,&ssh.password)?,
        }
        if !session.authenticated() { bail!("SSH 验证失败"); }
        session.set_timeout((config.exec_timeout*1000).min(u32::MAX as u64) as u32);
        Box::new(session.channel_direct_tcpip(host,port,None)?)
    } else {
        let tcp=proxy_tcp(config,host,port,&proxy)?;
        if proxy.kind!=0 && proxy.schema=="https" {
            let mut tls=native_tls::TlsConnector::new()?.connect(&proxy.addr,tcp)?;
            http_connect(&mut tls,host,port,&proxy)?; Box::new(tls)
        } else { Box::new(tcp) }
    };
    if config.ssl.enable {
        let mut b=native_tls::TlsConnector::builder();
        b.danger_accept_invalid_certs(config.ssl.allow_insecure).danger_accept_invalid_hostnames(config.ssl.allow_insecure);
        if !config.ssl.ca_file.is_empty() { b.add_root_certificate(native_tls::Certificate::from_pem(&fs::read(&config.ssl.ca_file)?)?); }
        if !config.ssl.cert_file.is_empty() && !config.ssl.key_file.is_empty() { b.identity(native_tls::Identity::from_pkcs8(&fs::read(&config.ssl.cert_file)?,&fs::read(&config.ssl.key_file)?)?); }
        let sni=if config.ssl.sni.is_empty() { host } else { &config.ssl.sni };
        stream=Box::new(b.build()?.connect(sni,stream).map_err(|e|anyhow!("TLS 连接失败：{e}"))?);
    }
    Ok(stream)
}

#[derive(Clone,Default)]
pub struct Cancellation(pub Arc<AtomicBool>);
impl Cancellation { pub fn cancel(&self) { self.0.store(true,Ordering::Relaxed); } pub fn canceled(&self)->bool { self.0.load(Ordering::Relaxed) } }

pub fn parse_command(line:&str) -> Result<Vec<Vec<u8>>> {
    let mut args=vec![]; let mut part=vec![]; let mut quote=None; let mut started=false;
    let b=line.as_bytes(); let mut i=0;
    while i<b.len() {
        let c=b[i];
        if c==b'\\' && quote!=Some(b'\'') {
            i+=1; if i>=b.len() { bail!("命令以无效转义结束"); }
            let v=match b[i] { b'n'=>b'\n',b'r'=>b'\r',b't'=>b'\t',b'x' if i+2<b.len()=>{ let v=u8::from_str_radix(&line[i+1..i+3],16)?; i+=2; v },other=>other };
            part.push(v); started=true;
        } else if let Some(q)=quote {
            if c==q { quote=None; } else { part.push(c); }
        } else if c==b'\'' || c==b'"' { quote=Some(c); started=true; }
        else if c.is_ascii_whitespace() { if started { args.push(std::mem::take(&mut part)); started=false; } }
        else { part.push(c); started=true; }
        i+=1;
    }
    if quote.is_some() { bail!("引号未闭合"); }
    if started { args.push(part); }
    Ok(args)
}

pub fn is_timeout(error:&anyhow::Error) -> bool { error.downcast_ref::<io::Error>().is_some_and(|e|matches!(e.kind(),io::ErrorKind::TimedOut|io::ErrorKind::WouldBlock)) }
