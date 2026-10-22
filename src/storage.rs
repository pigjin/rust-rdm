use crate::model::{Connection, Preferences};
use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn system() -> Self {
        if let Some(path) =
            std::env::var_os("REDISDOCK_DATA_DIR").or_else(|| std::env::var_os("RUST_RDM_DATA_DIR"))
        {
            return Self::new(path);
        }
        let dirs = directories::BaseDirs::new();
        Self::new(Self::default_root(dirs.as_ref().map(|d| d.config_dir())))
    }
    fn default_root(config_dir: Option<&Path>) -> PathBuf {
        let (root, legacy) = match config_dir {
            Some(base) => (base.join("RedisDock"), base.join("rust-rdm")),
            None => (PathBuf::from(".redisdock-data"), PathBuf::from(".rdm-data")),
        };
        // Reuse existing connections and preferences without moving or copying user data.
        if !root.exists() && legacy.is_dir() {
            legacy
        } else {
            root
        }
    }
    pub fn load<T: DeserializeOwned + Default>(&self, filename: &str) -> Result<T> {
        let path = self.root.join(filename);
        if !path.exists() {
            return Ok(T::default());
        }
        serde_yaml::from_slice(&fs::read(&path)?)
            .with_context(|| format!("无法读取 {}", path.display()))
    }
    pub fn save<T: Serialize>(&self, filename: &str, value: &T) -> Result<()> {
        self.save_bytes(filename, serde_yaml::to_string(value)?.as_bytes())
    }
    fn save_bytes(&self, filename: &str, bytes: &[u8]) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700))?;
        }
        atomic_write(&self.root.join(filename), bytes)
    }
    pub fn connections(&self) -> Result<Vec<Connection>> {
        self.load("connections.yaml")
    }
    pub fn preferences(&self) -> Result<Preferences> {
        // The original getter falls back to defaults on unreadable YAML and
        // repairs these layout and scan values without rewriting the file.
        let mut pref: Preferences = self.load("preferences.yaml").unwrap_or_default();
        if pref.general.scan_size == 0 {
            pref.general.scan_size = 3000;
        }
        pref.behavior.aside_width = pref.behavior.aside_width.max(300.);
        pref.behavior.window_width = pref.behavior.window_width.max(960.);
        pref.behavior.window_height = pref.behavior.window_height.max(640.);
        Ok(pref)
    }
    pub fn import_connections(&self, path: &Path) -> Result<Vec<Connection>> {
        let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
        let mut candidate = None;
        // The desktop importer takes the first matching entry, or the last entry
        // if the archive has no connections.yaml. It copies bytes without form validation.
        for index in 0..archive.len() {
            candidate = Some(index);
            if archive.by_index(index)?.name_raw() == b"connections.yaml" {
                break;
            }
        }
        if let Some(index) = candidate {
            let mut bytes = Vec::new();
            archive.by_index(index)?.read_to_end(&mut bytes)?;
            self.save_bytes("connections.yaml", &bytes)?;
        }
        // The original connection list is empty when the copied YAML is invalid.
        Ok(self.connections().unwrap_or_default())
    }
    pub fn export_connections(&self, path: &Path) -> Result<()> {
        let bytes = fs::read(self.root.join("connections.yaml"))?;
        let mut file = staging_file(path)?;
        {
            let mut archive = zip::ZipWriter::new(file.as_file_mut());
            archive.start_file(
                "connections.yaml",
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )?;
            archive.write_all(&bytes)?;
            archive.finish()?;
        }
        file.as_file().sync_all()?;
        file.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = staging_file(path)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
pub fn staging_file(path: &Path) -> Result<tempfile::NamedTempFile> {
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(tempfile::Builder::new()
        .prefix(".redisdock-")
        .tempfile_in(directory)?)
}
pub fn flatten(conns: &[Connection]) -> Vec<Connection> {
    let mut out = vec![];
    for c in conns {
        if c.category == "group" {
            for mut child in flatten(&c.connections) {
                child.group = c.name.clone();
                out.push(child);
            }
        } else {
            out.push(c.clone());
        }
    }
    out
}
pub fn remove_connection(conns: &mut Vec<Connection>, name: &str) -> bool {
    if let Some(i) = conns
        .iter()
        .position(|c| c.category != "group" && c.name == name)
    {
        conns.remove(i);
        return true;
    }
    conns
        .iter_mut()
        .any(|c| remove_connection(&mut c.connections, name))
}
pub fn save_connection(
    conns: &mut Vec<Connection>,
    original: Option<&str>,
    mut item: Connection,
) -> Result<()> {
    // SaveConnection validates the name; incomplete transport settings can be saved.
    if item.name.contains('/') {
        bail!("connection name contains illegal characters");
    }
    if flatten(conns)
        .iter()
        .any(|c| c.name == item.name && Some(c.name.as_str()) != original)
    {
        bail!("同名连接已存在：{}", item.name);
    }
    let group_name = std::mem::take(&mut item.group);
    if let Some(name) = original {
        let old = flatten(conns)
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| anyhow::anyhow!("连接不存在：{name}"))?;
        if old.group == group_name {
            fn replace(conns: &mut [Connection], name: &str, item: &Connection) -> bool {
                for c in conns {
                    if c.category != "group" && c.name == name {
                        *c = item.clone();
                        return true;
                    }
                    if replace(&mut c.connections, name, item) {
                        return true;
                    }
                }
                false
            }
            replace(conns, name, &item);
            return Ok(());
        }
        remove_connection(conns, name);
    }
    if group_name.is_empty() {
        conns.push(item);
    } else if let Some(g) = conns
        .iter_mut()
        .find(|c| c.category == "group" && c.name == group_name)
    {
        g.connections.push(item);
    } else {
        conns.push(Connection {
            name: group_name,
            category: "group".into(),
            connections: vec![item],
            ..Default::default()
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redisdock_config_reuses_legacy_data_until_a_new_directory_exists() {
        let directory = tempfile::tempdir().unwrap();
        let current = directory.path().join("RedisDock");
        let legacy = directory.path().join("rust-rdm");
        assert_eq!(Store::default_root(Some(directory.path())), current);

        fs::create_dir(&legacy).unwrap();
        let connections = b"- name: existing-connection\n";
        fs::write(legacy.join("connections.yaml"), connections).unwrap();
        let store = Store::new(Store::default_root(Some(directory.path())));
        assert_eq!(store.root, legacy);
        assert_eq!(store.connections().unwrap()[0].name, "existing-connection");
        assert!(!current.exists());

        fs::create_dir(&current).unwrap();
        assert_eq!(Store::default_root(Some(directory.path())), current);
        assert_eq!(
            fs::read(legacy.join("connections.yaml")).unwrap(),
            connections
        );
    }
}
