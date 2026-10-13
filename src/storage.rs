use crate::model::{Connection, Preferences};
use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs::{self, OpenOptions}, io::Write, path::{Path, PathBuf}};

#[derive(Clone)]
pub struct Store { pub root: PathBuf }
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self { Self { root: root.into() } }
    pub fn system() -> Self {
        if let Some(path) = std::env::var_os("RUST_RDM_DATA_DIR") { return Self::new(path); }
        Self::new(directories::BaseDirs::new().map(|d|d.config_dir().join("rust-rdm")).unwrap_or_else(|| PathBuf::from(".rdm-data")))
    }
    pub fn load<T: DeserializeOwned + Default>(&self, filename: &str) -> Result<T> {
        let path = self.root.join(filename);
        if !path.exists() { return Ok(T::default()); }
        serde_yaml::from_slice(&fs::read(&path)?).with_context(|| format!("无法读取 {}", path.display()))
    }
    pub fn save<T: Serialize>(&self, filename: &str, value: &T) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?; }
        atomic_write(&self.root.join(filename), serde_yaml::to_string(value)?.as_bytes())
    }
    pub fn connections(&self) -> Result<Vec<Connection>> { self.load("connections.yaml") }
    pub fn preferences(&self) -> Result<Preferences> { self.load("preferences.yaml") }
    pub fn import_connections(&self, path: &Path, replace: bool) -> Result<Vec<Connection>> {
        let mut imported: Vec<Connection> = serde_yaml::from_slice(&fs::read(path)?)?;
        validate_tree(&imported)?;
        if !replace {
            let mut existing = self.connections()?;
            let names = flatten(&existing).into_iter().map(|c|c.name).collect::<Vec<_>>();
            for conn in flatten(&imported) {
                if names.contains(&conn.name) { bail!("同名连接已存在：{}", conn.name); }
            }
            for item in imported {
                if item.category == "group" && let Some(group) = existing.iter_mut().find(|c| c.category == "group" && c.name == item.name) {
                    group.connections.extend(item.connections);
                } else { existing.push(item); }
            }
            imported = existing;
        }
        validate_tree(&imported)?; self.save("connections.yaml", &imported)?; Ok(imported)
    }
    pub fn export_connections(&self, path: &Path) -> Result<()> { atomic_write(path, serde_yaml::to_string(&self.connections()?)?.as_bytes()) }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}",std::process::id()));
    let mut options = OpenOptions::new(); options.write(true).create(true).truncate(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(&tmp)?; file.write_all(bytes)?; file.sync_all()?;
    fs::rename(&tmp,path)?; Ok(())
}
pub fn flatten(conns: &[Connection]) -> Vec<Connection> {
    let mut out=vec![];
    for c in conns {
        if c.category == "group" { for mut child in flatten(&c.connections) { child.group = c.name.clone(); out.push(child); } }
        else { out.push(c.clone()); }
    }
    out
}
pub fn validate_tree(conns: &[Connection]) -> Result<()> {
    let mut names = std::collections::HashSet::new();
    for c in flatten(conns) { c.validate()?; if !names.insert(c.name.clone()) { bail!("同名连接已存在：{}",c.name); } }
    let mut groups = std::collections::HashSet::new();
    for c in conns.iter().filter(|c|c.category == "group") { c.validate()?; if !groups.insert(&c.name) { bail!("同名分组已存在：{}",c.name); } }
    Ok(())
}
pub fn remove_connection(conns: &mut Vec<Connection>, name: &str) -> bool {
    if let Some(i) = conns.iter().position(|c|c.category != "group" && c.name == name) { conns.remove(i); return true; }
    conns.iter_mut().any(|c|remove_connection(&mut c.connections,name))
}
pub fn save_connection(conns: &mut Vec<Connection>, original: Option<&str>, mut item: Connection) -> Result<()> {
    item.validate()?;
    if flatten(conns).iter().any(|c|c.name==item.name && Some(c.name.as_str())!=original) { bail!("同名连接已存在：{}",item.name); }
    if let Some(name) = original { remove_connection(conns,name); }
    let group_name=std::mem::take(&mut item.group);
    if group_name.is_empty() { conns.push(item); }
    else if let Some(g) = conns.iter_mut().find(|c|c.category=="group" && c.name==group_name) { g.connections.push(item); }
    else { conns.push(Connection { name:group_name, category:"group".into(), connections:vec![item], ..Default::default() }); }
    Ok(())
}
