//! Creates disposable data only in the explicitly supplied local Unix socket.
use redisdock::{model::*, redis::command, service::Service, storage::Store};
fn main() -> anyhow::Result<()> {
    let render_qa = std::env::args().nth(2).as_deref() == Some("--render-qa");
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("usage: seed_demo /absolute/disposable/directory"))?,
    );
    let store = Store::new(&root);
    let config = Connection {
        name: "localhost".into(),
        network: "unix".into(),
        sock: root.join("redis.sock").to_string_lossy().into_owned(),
        ..Default::default()
    };
    let s = Service::new(store.clone());
    s.create(
        &config,
        0,
        b"account:1",
        KeyType::String,
        br#"{"id":1,"name":"Tiny RDM","enabled":true}"#,
        &[],
        -1,
    )?;
    s.create(
        &config,
        0,
        b"aastring",
        KeyType::String,
        b"Hello Rust + GPUI",
        &[],
        -1,
    )?;
    s.create(
        &config,
        0,
        b"test:expiring",
        KeyType::String,
        b"TTL 600",
        &[],
        600,
    )?;
    let rows = vec![
        Entry {
            id: b"name".to_vec(),
            value: b"Tiny RDM".to_vec(),
            score: "1".into(),
            ..Default::default()
        },
        Entry {
            id: b"language".to_vec(),
            value: b"Rust".to_vec(),
            score: "2".into(),
            ..Default::default()
        },
    ];
    for (key, kind) in [
        ("aahash", KeyType::Hash),
        ("aalist", KeyType::List),
        ("aaset", KeyType::Set),
        ("aazset", KeyType::ZSet),
        ("stream:events", KeyType::Stream),
    ] {
        if s.run(&config, 0, command(&["EXISTS", key]))?.integer()? == 0 {
            s.create(&config, 0, key.as_bytes(), kind, b"*", &rows, -1)?;
        }
    }
    let gzip = redisdock::codec::encode("{\"compressed\":true}", "GZip", "JSON", &[])?;
    s.create(&config, 0, b"gzip_test", KeyType::String, &gzip, &[], -1)?;
    s.create(
        &config,
        0,
        &[0xff, 0, 65],
        KeyType::String,
        &[0, 255, 10],
        &[],
        -1,
    )?;
    for i in 0..10 {
        s.create(
            &config,
            0,
            format!("test:{i}").as_bytes(),
            KeyType::String,
            format!("value {i}").as_bytes(),
            &[],
            -1,
        )?;
    }
    let _ = s.create(
        &config,
        0,
        b"json:document",
        KeyType::Json,
        br#"{"app":"Tiny RDM"}"#,
        &[],
        -1,
    );
    if render_qa {
        // The comparison scenes use the same 19 non-expiring keys in both servers.
        s.run(&config, 0, command(&["DEL", "test:expiring", "json:document"]))?;
    }
    store.save("connections.yaml", &vec![config])?;
    store.save(
        "preferences.yaml",
        &Preferences {
            general: General {
                language: "zh-cn".into(),
                theme: "light".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    std::fs::write(root.join("RDM-QA-FIXTURE"), "rust-rdm-render-fixture-v1\n")?;
    println!("Disposable demo data prepared at {}", root.display());
    Ok(())
}
