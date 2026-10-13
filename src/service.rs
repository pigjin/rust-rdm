use crate::{model::*, redis::{self, Cancellation, Client, Value, Wire, command}, storage::{Store, atomic_write}};
use anyhow::{Result, bail};
use std::{collections::{HashMap, HashSet, VecDeque}, path::Path, time::{Instant, SystemTime, UNIX_EPOCH}};

#[derive(Clone,Debug)]
pub struct CommandLog { pub time:String, pub server:String, pub db:u32, pub command:String, pub elapsed_ms:f64, pub error:String }
#[derive(Clone,Debug,Default)]
pub struct ScanState { pub nodes:Vec<(String,u16)>, pub node:usize, pub cursor:u64, pub started:bool, pub end:bool }
#[derive(Clone,Debug,Default)]
pub struct ScanPage { pub keys:Vec<KeyEntry>, pub state:ScanState, pub total:u64 }
#[derive(Clone,Debug,Default)]
pub struct TransferProgress { pub done:usize, pub failed:usize, pub skipped:usize, pub total:usize, pub canceled:bool }
#[derive(Clone,Debug)]
pub struct SlowLog { pub id:i64, pub time:i64, pub duration:i64, pub args:String, pub address:String, pub name:String }

pub struct Service {
    clients:HashMap<(String,u32),Client>,
    pub history:VecDeque<CommandLog>,
    pub store:Store,
}
impl Service {
    pub fn new(store:Store) -> Self { Self { clients:HashMap::new(),history:VecDeque::new(),store } }
    pub fn connect(&mut self,c:&Connection,db:u32) -> Result<()> { self.clients.insert((c.name.clone(),db),Client::connect(c,db)?); Ok(()) }
    pub fn close(&mut self,name:&str) { self.clients.retain(|(n,_),_|n!=name); }
    pub fn client(&mut self,c:&Connection,db:u32) -> Result<&mut Client> {
        let id=(c.name.clone(),db);
        if !self.clients.contains_key(&id) { self.connect(c,db)?; }
        Ok(self.clients.get_mut(&id).unwrap())
    }
    pub fn run(&mut self,c:&Connection,db:u32,args:Vec<Vec<u8>>) -> Result<Value> {
        let start=Instant::now(); let result=self.client(c,db)?.query(&args);
        let is_auth=args.first().is_some_and(|a|a.eq_ignore_ascii_case(b"AUTH"));
        self.history.push_back(CommandLog {
            time:chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),server:c.name.clone(),db,
            command:if is_auth { "AUTH ***".into() } else { args.iter().map(|a|display_bytes(a)).collect::<Vec<_>>().join(" ") },
            elapsed_ms:start.elapsed().as_secs_f64()*1000., error:result.as_ref().err().map(|e|e.to_string()).unwrap_or_default(),
        });
        while self.history.len()>5000 { self.history.pop_front(); }
        result
    }
    pub fn info(&mut self,c:&Connection) -> Result<Info> { Ok(parse_info(&self.run(c,0,command(&["INFO","ALL"]))?.text())) }
    pub fn databases(&mut self,c:&Connection) -> Result<Vec<Database>> {
        let info=self.info(c)?;
        let count=if c.cluster.enable { 1 } else {
            self.run(c,0,command(&["CONFIG","GET","databases"])).ok().and_then(|v|v.array().ok().and_then(|a|a.get(1).and_then(|v|v.integer().ok()))).unwrap_or(16).clamp(1,65536) as u32
        };
        let mut out=vec![];
        for index in 0..count {
            if c.db_filter_type=="show" && !c.db_filter_list.contains(&index) || c.db_filter_type=="hide" && c.db_filter_list.contains(&index) { continue; }
            let text=info.get("Keyspace").and_then(|s|s.get(&format!("db{index}"))).cloned().unwrap_or_default();
            let parts=text.split(',').filter_map(|p|p.split_once('=')).map(|(k,v)|(k,v.parse::<u64>().unwrap_or(0))).collect::<HashMap<_,_>>();
            out.push(Database { index,keys:*parts.get("keys").unwrap_or(&0),expires:*parts.get("expires").unwrap_or(&0),avg_ttl:*parts.get("avg_ttl").unwrap_or(&0),alias:c.alias.get(&index).cloned().unwrap_or_default() });
        }
        Ok(out)
    }
    pub fn scan(&mut self,c:&Connection,db:u32,pattern:&str,kind:&str,exact:bool,mut state:ScanState,count:usize) -> Result<ScanPage> {
        if state.end { return Ok(ScanPage { state,..Default::default() }); }
        let pattern=if pattern.is_empty() { "*" } else { pattern };
        if exact {
            let key=parse_bytes(pattern)?;
            let typ=KeyType::parse(&self.run(c,db,vec![b"TYPE".to_vec(),key.clone()])?.text());
            let keys=if typ!=KeyType::None && (kind.is_empty() || typ.name().eq_ignore_ascii_case(kind)) { vec![KeyEntry{key,kind:typ}] } else { vec![] };
            state.end=true; return Ok(ScanPage { total:keys.len() as u64,keys,state });
        }
        if state.nodes.is_empty() { state.nodes=self.client(c,db)?.masters()?; }
        let mut keys=vec![]; let mut seen=HashSet::new(); let size=count.max(1); let mut rounds=0;
        while state.node<state.nodes.len() && keys.len()<size && rounds<64 {
            let (host,port)=state.nodes[state.node].clone();
            let mut args=command(&["SCAN",&state.cursor.to_string(),"MATCH",pattern,"COUNT",&size.to_string()]);
            if !kind.is_empty() { args.extend(command(&["TYPE",if kind.eq_ignore_ascii_case("JSON") { "ReJSON-RL" } else { kind }])); }
            let value=self.client(c,db)?.query_at(&host,port,&args)?;
            let a=value.array()?; if a.len()!=2 { bail!("SCAN 响应异常"); }
            state.cursor=a[0].integer()? as u64; state.started=true;
            for value in a[1].array()? {
                let key=value.bytes(); if !seen.insert(key.clone()) { continue; }
                let typ=KeyType::parse(&self.client(c,db)?.query_at(&host,port,&vec![b"TYPE".to_vec(),key.clone()])?.text());
                if typ!=KeyType::None { keys.push(KeyEntry{key,kind:typ}); }
            }
            if state.cursor==0 { state.node+=1; }
            rounds+=1;
        }
        state.end=state.node>=state.nodes.len();
        let total=if c.cluster.enable { let mut sum=0; for (h,p) in &state.nodes { sum+=self.client(c,db)?.query_at(h,*p,&command(&["DBSIZE"]))?.integer()? as u64; } sum }
            else { self.run(c,db,command(&["DBSIZE"]))?.integer()? as u64 };
        Ok(ScanPage{keys,state,total})
    }
    pub fn detail(&mut self,c:&Connection,db:u32,key:&[u8],cursor:u64,pattern:&str,page:usize,stream_after:&str,reverse:bool) -> Result<KeyDetail> {
        let kind=KeyType::parse(&self.run(c,db,vec![b"TYPE".to_vec(),key.to_vec()])?.text());
        if kind==KeyType::None { bail!("键不存在"); }
        let ttl=self.run(c,db,vec![b"TTL".to_vec(),key.to_vec()])?.integer()?;
        let memory=self.run(c,db,vec![b"MEMORY".to_vec(),b"USAGE".to_vec(),key.to_vec()]).ok().and_then(|v|v.integer().ok()).unwrap_or(0).max(0) as u64;
        let encoding=self.run(c,db,vec![b"OBJECT".to_vec(),b"ENCODING".to_vec(),key.to_vec()]).map(|v|v.text()).unwrap_or_default();
        let mut d=KeyDetail { key:key.to_vec(),kind:kind.clone(),ttl,memory,encoding,..Default::default() };
        let page=page.max(1); let pattern=if pattern.is_empty() { "*" } else { pattern };
        match kind {
            KeyType::String|KeyType::Json=>{
                let cmd=if kind==KeyType::String { "GET" } else { "JSON.GET" };
                d.value=self.run(c,db,vec![cmd.as_bytes().to_vec(),key.to_vec()])?.bytes(); d.length=d.value.len() as u64; d.end=true;
            },
            KeyType::Hash|KeyType::Set|KeyType::ZSet=>{
                let (scan,len)=match kind { KeyType::Hash=>("HSCAN","HLEN"),KeyType::Set=>("SSCAN","SCARD"),_=>("ZSCAN","ZCARD") };
                d.length=self.run(c,db,vec![len.as_bytes().to_vec(),key.to_vec()])?.integer()? as u64;
                let mut args=vec![scan.as_bytes().to_vec(),key.to_vec()]; args.extend(command(&[&cursor.to_string(),"MATCH",pattern,"COUNT",&page.to_string()]));
                let v=self.run(c,db,args)?; let a=v.array()?; d.cursor=a[0].integer()? as u64; d.end=d.cursor==0;
                let rows=a[1].array()?;
                if kind==KeyType::Set { d.rows=rows.iter().map(|v|Entry { id:v.bytes(),value:v.bytes(),..Default::default() }).collect(); }
                else { for r in rows.chunks_exact(2) { d.rows.push(if kind==KeyType::Hash { Entry { id:r[0].bytes(),value:r[1].bytes(),..Default::default() } } else { Entry { id:r[0].bytes(),value:r[0].bytes(),score:r[1].text(),..Default::default() } }); } }
                if kind==KeyType::ZSet { d.rows.sort_by(|a,b|a.score.parse::<f64>().unwrap_or_default().total_cmp(&b.score.parse::<f64>().unwrap_or_default())); if reverse { d.rows.reverse(); } }
            },
            KeyType::List=>{
                d.length=self.run(c,db,vec![b"LLEN".to_vec(),key.to_vec()])?.integer()? as u64;
                let mut args=vec![b"LRANGE".to_vec(),key.to_vec()]; args.extend(command(&[&cursor.to_string(),&(cursor+page as u64-1).to_string()]));
                let v=self.run(c,db,args)?;
                for (i,v) in v.array()?.iter().enumerate() { if glob_match(pattern.as_bytes(),&v.bytes()) { d.rows.push(Entry{id:(cursor+i as u64).to_string().into_bytes(),value:v.bytes(),..Default::default()}); } }
                d.cursor=(cursor+page as u64).min(d.length); d.end=d.cursor>=d.length;
            },
            KeyType::Stream=>{
                d.length=self.run(c,db,vec![b"XLEN".to_vec(),key.to_vec()])?.integer()? as u64;
                let start=if stream_after.is_empty() { "-".into() } else { format!("({stream_after}") };
                let mut args=vec![b"XRANGE".to_vec(),key.to_vec()]; args.extend(command(&[&start,"+","COUNT",&page.to_string()]));
                let v=self.run(c,db,args)?;
                for r in v.array()? { let a=r.array()?; let fields=a[1].array()?.chunks_exact(2).map(|p|(p[0].bytes(),p[1].bytes())).collect::<Vec<_>>();
                    d.rows.push(Entry { id:a[0].bytes(),value:serde_json::to_vec(&fields.iter().map(|(k,v)|(display_bytes(k),display_bytes(v))).collect::<std::collections::BTreeMap<_,_>>())?,fields,..Default::default() }); }
                d.cursor=cursor+d.rows.len() as u64; d.end=d.rows.len()<page;
            },
            _=>bail!("不支持的数据类型：{}",kind.name()),
        }
        Ok(d)
    }
    pub fn set_string(&mut self,c:&Connection,db:u32,key:&[u8],value:Vec<u8>,json:bool) -> Result<()> {
        let mut args=if json { vec![b"JSON.SET".to_vec(),key.to_vec(),b"$".to_vec(),value] } else { vec![b"SET".to_vec(),key.to_vec(),value] };
        if !json { args.push(b"KEEPTTL".to_vec()); }
        self.run(c,db,args)?; Ok(())
    }
    pub fn create(&mut self,c:&Connection,db:u32,key:&[u8],kind:KeyType,value:&[u8],rows:&[Entry],ttl:i64) -> Result<()> {
        if key.is_empty() { bail!("键名不能为空"); }
        let mut args=match kind {
            KeyType::String=>vec![b"SET".to_vec(),key.to_vec(),value.to_vec()],
            KeyType::Json=>{ let _:serde_json::Value=serde_json::from_slice(value)?; vec![b"JSON.SET".to_vec(),key.to_vec(),b"$".to_vec(),value.to_vec()] },
            KeyType::Hash=>{ let mut a=vec![b"HSET".to_vec(),key.to_vec()]; for r in rows { a.push(r.id.clone()); a.push(r.value.clone()); } a },
            KeyType::List|KeyType::Set=>{ let mut a=vec![if kind==KeyType::List { b"RPUSH".to_vec() } else { b"SADD".to_vec() },key.to_vec()]; a.extend(rows.iter().map(|r|r.value.clone())); a },
            KeyType::ZSet=>{ let mut a=vec![b"ZADD".to_vec(),key.to_vec()]; for r in rows { let _:f64=r.score.parse()?; a.push(r.score.as_bytes().to_vec()); a.push(r.value.clone()); } a },
            KeyType::Stream=>{ let mut a=vec![b"XADD".to_vec(),key.to_vec(),if value.is_empty() { b"*".to_vec() } else { value.to_vec() }]; for r in rows { a.push(r.id.clone()); a.push(r.value.clone()); } a },
            _=>bail!("无效数据类型"),
        };
        if args.len()<3 { bail!("请添加至少一项数据"); }
        if kind==KeyType::String && ttl>0 { args.extend(command(&["EX",&ttl.to_string()])); }
        self.run(c,db,args)?;
        if kind!=KeyType::String && ttl>0 { self.ttl(c,db,key,ttl)?; }
        Ok(())
    }
    pub fn ttl(&mut self,c:&Connection,db:u32,key:&[u8],ttl:i64) -> Result<()> {
        let mut args=vec![if ttl<0 { b"PERSIST".to_vec() } else { b"EXPIRE".to_vec() },key.to_vec()]; if ttl>=0 { args.push(ttl.to_string().into_bytes()); } self.run(c,db,args)?; Ok(())
    }
    pub fn rename(&mut self,c:&Connection,db:u32,key:&[u8],new:&[u8]) -> Result<()> { self.run(c,db,vec![b"RENAME".to_vec(),key.to_vec(),new.to_vec()])?; Ok(()) }
    pub fn delete(&mut self,c:&Connection,db:u32,key:&[u8],asynchronous:bool) -> Result<()> { self.run(c,db,vec![if asynchronous { b"UNLINK".to_vec() } else { b"DEL".to_vec() },key.to_vec()])?; Ok(()) }
    pub fn edit_entry(&mut self,c:&Connection,db:u32,key:&[u8],kind:&KeyType,old:&Entry,new:&Entry,remove:bool) -> Result<()> {
        let mut args=match kind {
            KeyType::Hash=>if remove { vec![b"HDEL".to_vec(),key.to_vec(),old.id.clone()] } else { vec![b"HSET".to_vec(),key.to_vec(),new.id.clone(),new.value.clone()] },
            KeyType::List=>{
                if remove {
                    let script=b"local v=redis.call('LINDEX',KEYS[1],ARGV[1]); if not v then return 0 end; local mark=ARGV[2]; redis.call('LSET',KEYS[1],ARGV[1],mark); return redis.call('LREM',KEYS[1],1,mark)";
                    let marker=format!("\0rust-rdm-delete-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()).into_bytes();
                    vec![b"EVAL".to_vec(),script.to_vec(),b"1".to_vec(),key.to_vec(),old.id.clone(),marker]
                } else { vec![b"LSET".to_vec(),key.to_vec(),old.id.clone(),new.value.clone()] }
            },
            KeyType::Set=>if remove { vec![b"SREM".to_vec(),key.to_vec(),old.value.clone()] } else { vec![b"SADD".to_vec(),key.to_vec(),new.value.clone()] },
            KeyType::ZSet=>if remove { vec![b"ZREM".to_vec(),key.to_vec(),old.value.clone()] } else { let _:f64=new.score.parse()?; vec![b"ZADD".to_vec(),key.to_vec(),new.score.as_bytes().to_vec(),new.value.clone()] },
            KeyType::Stream if remove=>vec![b"XDEL".to_vec(),key.to_vec(),old.id.clone()],
            _=>bail!("此类型不支持编辑条目"),
        };
        self.run(c,db,std::mem::take(&mut args))?;
        if !remove { match kind {
            KeyType::Hash if old.id!=new.id=>{ self.run(c,db,vec![b"HDEL".to_vec(),key.to_vec(),old.id.clone()])?; },
            KeyType::Set|KeyType::ZSet if old.value!=new.value=>{ self.run(c,db,vec![if *kind==KeyType::Set { b"SREM".to_vec() } else { b"ZREM".to_vec() },key.to_vec(),old.value.clone()])?; }, _=>{},
        } }
        Ok(())
    }
    pub fn add_entries(&mut self,c:&Connection,db:u32,key:&[u8],kind:KeyType,rows:&[Entry],head:bool,only_new:bool,id:&str) -> Result<()> {
        if kind==KeyType::List && head {
            let mut a=vec![b"LPUSH".to_vec(),key.to_vec()]; a.extend(rows.iter().map(|r|r.value.clone())); self.run(c,db,a)?;
        } else if kind==KeyType::Hash && only_new {
            for r in rows { self.run(c,db,vec![b"HSETNX".to_vec(),key.to_vec(),r.id.clone(),r.value.clone()])?; }
        } else if kind==KeyType::ZSet && only_new {
            let mut a=vec![b"ZADD".to_vec(),key.to_vec(),b"NX".to_vec()]; for r in rows { let _:f64=r.score.parse()?; a.push(r.score.as_bytes().to_vec()); a.push(r.value.clone()); } self.run(c,db,a)?;
        } else { self.create(c,db,key,kind,id.as_bytes(),rows,-1)?; }
        Ok(())
    }
    pub fn flush(&mut self,c:&Connection,db:u32,asynchronous:bool) -> Result<()> {
        let args=command(&["FLUSHDB",if asynchronous { "ASYNC" } else { "SYNC" }]);
        let masters=self.client(c,db)?.masters()?; for (h,p) in masters { self.client(c,db)?.query_at(&h,p,&args)?; } Ok(())
    }
    pub fn slow_logs(&mut self,c:&Connection,count:usize) -> Result<Vec<SlowLog>> {
        let v=self.run(c,0,command(&["SLOWLOG","GET",&count.to_string()]))?;
        v.array()?.iter().map(|row|{
            let a=row.array()?; if a.len()<4 { bail!("慢日志响应异常"); }
            Ok(SlowLog{id:a[0].integer()?,time:a[1].integer()?,duration:a[2].integer()?,args:a[3].array()?.iter().map(Value::text).collect::<Vec<_>>().join(" "),address:a.get(4).map(Value::text).unwrap_or_default(),name:a.get(5).map(Value::text).unwrap_or_default()})
        }).collect()
    }
    pub fn sentinel_masters(&mut self,c:&Connection) -> Result<Value> {
        let (h,p)=redis::split_endpoint(c.addr.split(',').next().unwrap_or(&c.addr),c.port)?;
        Wire::open(c,&h,p,0,true)?.query(&command(&["SENTINEL","MASTERS"]))?.checked()
    }
    pub fn export_csv(&mut self,c:&Connection,db:u32,keys:&[Vec<u8>],path:&Path,include_ttl:bool,cancel:&Cancellation,mut progress:impl FnMut(TransferProgress)) -> Result<TransferProgress> {
        let mut bytes=vec![]; let mut w=csv::Writer::from_writer(&mut bytes);
        let mut report=TransferProgress{total:keys.len(),..Default::default()};
        for key in keys {
            if cancel.canceled() { report.canceled=true; break; }
            match self.run(c,db,vec![b"DUMP".to_vec(),key.clone()]) {
                Ok(Value::Bytes(dump))=>{
                    let mut record=vec![hex::encode(key),hex::encode(dump)];
                    if include_ttl { let ttl=self.run(c,db,vec![b"PTTL".to_vec(),key.clone()])?.integer()?; record.push(if ttl>0 { (now_ms()+ttl).to_string() } else { "-1".into() }); }
                    w.write_record(record)?; report.done+=1;
                },_=>report.failed+=1,
            }
            progress(report.clone());
        }
        w.flush()?; drop(w); atomic_write(path,&bytes)?; Ok(report)
    }
    pub fn import_csv(&mut self,c:&Connection,db:u32,path:&Path,replace:bool,ttl:i64,cancel:&Cancellation,mut progress:impl FnMut(TransferProgress)) -> Result<TransferProgress> {
        let mut reader=csv::ReaderBuilder::new().has_headers(false).flexible(true).from_path(path)?;
        let mut report=TransferProgress::default();
        for row in reader.records() {
            if cancel.canceled() { report.canceled=true; break; }
            report.total+=1;
            let row=match row { Ok(r)=>r,Err(_)=>{report.failed+=1;continue;} };
            if row.len()<2 { report.failed+=1; continue; }
            let (Ok(key),Ok(value))=(hex::decode(&row[0]),hex::decode(&row[1])) else { report.failed+=1; continue; };
            let expire=if ttl>0 { ttl.saturating_mul(1000) } else if ttl<0 { row.get(2).and_then(|s|s.parse::<i64>().ok()).filter(|n|*n>0).map(|n|n-now_ms()).unwrap_or(0) } else { 0 };
            if expire<0 { report.skipped+=1; continue; }
            let mut args=vec![b"RESTORE".to_vec(),key,expire.to_string().into_bytes(),value]; if replace { args.push(b"REPLACE".to_vec()); }
            match self.run(c,db,args) { Ok(_)=>report.done+=1, Err(e) if e.to_string().contains("BUSYKEY")=>report.skipped+=1, Err(_)=>report.failed+=1 }
            progress(report.clone());
        }
        Ok(report)
    }
    pub fn all_keys(&mut self,c:&Connection,db:u32,pattern:&str,cancel:&Cancellation) -> Result<Vec<Vec<u8>>> {
        let mut state=ScanState::default(); let mut keys=HashSet::new();
        while !state.end && !cancel.canceled() { let page=self.scan(c,db,pattern,"",false,state,3000)?; keys.extend(page.keys.into_iter().map(|k|k.key)); state=page.state; }
        Ok(keys.into_iter().collect())
    }
    pub fn batch(&mut self,c:&Connection,db:u32,keys:&[Vec<u8>],ttl:Option<i64>,cancel:&Cancellation,mut progress:impl FnMut(TransferProgress)) -> Result<TransferProgress> {
        let mut r=TransferProgress{total:keys.len(),..Default::default()};
        for k in keys { if cancel.canceled() { r.canceled=true;break; } let result=if let Some(ttl)=ttl { self.ttl(c,db,k,ttl) } else { self.delete(c,db,k,true) };
            match result { Ok(_)=>r.done+=1,Err(_)=>r.failed+=1 } progress(r.clone()); }
        Ok(r)
    }
}

pub fn now_ms() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }
pub fn parse_info(text:&str) -> Info {
    let mut info=Info::new(); let mut section="Server".to_owned();
    for line in text.lines() { let line=line.trim(); if let Some(s)=line.strip_prefix("# ") { section=s.into(); }
        else if let Some((k,v))=line.split_once(':') { info.entry(section.clone()).or_default().insert(k.into(),v.into()); } }
    info
}
pub fn info_value(info:&Info,section:&str,key:&str) -> String { info.get(section).and_then(|s|s.get(key)).cloned().unwrap_or_default() }

pub fn glob_match(pattern:&[u8],value:&[u8]) -> bool {
    fn run(p:&[u8],v:&[u8],memo:&mut HashMap<(usize,usize),bool>) -> bool {
        let key=(p.len(),v.len()); if let Some(r)=memo.get(&key) {return *r;}
        let r=match p.first() {
            None=>v.is_empty(), Some(b'*')=>run(&p[1..],v,memo)||!v.is_empty()&&run(p,&v[1..],memo),
            Some(b'?')=>!v.is_empty()&&run(&p[1..],&v[1..],memo),
            Some(b'\\') if p.len()>1=>v.first()==p.get(1)&&!v.is_empty()&&run(&p[2..],&v[1..],memo),
            Some(b'[')=>{
                if let Some(end)=p.iter().position(|b|*b==b']') {
                    if v.is_empty() { false } else {
                        let class=&p[1..end]; let negate=class.first()==Some(&b'^'); let class=if negate {&class[1..]} else {class}; let mut matched=false; let mut i=0;
                        while i<class.len() { if i+2<class.len() && class[i+1]==b'-' { matched|=(class[i]..=class[i+2]).contains(&v[0]); i+=3; } else {matched|=class[i]==v[0];i+=1;} }
                        matched!=negate&&run(&p[end+1..],&v[1..],memo)
                    }
                } else { v.first()==p.first()&&!v.is_empty()&&run(&p[1..],&v[1..],memo) }
            },
            Some(c)=>v.first()==Some(c)&&!v.is_empty()&&run(&p[1..],&v[1..],memo),
        }; memo.insert(key,r);r
    }
    run(pattern,value,&mut HashMap::new())
}
