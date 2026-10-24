use redisdock::{
    codec,
    model::*,
    redis::{self, Client, Value, command},
    service::{ScanState, Service, glob_match},
    storage::{Store, flatten, save_connection},
};
use std::{
    io::Cursor,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn connection_form_validation_matches_original_general_then_advanced_rules() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/connection-form-source.json")).unwrap();
    let defaults = Connection::default();
    for case in corpus["records"].as_array().unwrap().iter().take(10) {
        let patch = &case["input"];
        let values = [
            ("name", "name", defaults.name.as_str()),
            ("addr", "addr", defaults.addr.as_str()),
            (
                "default_filter",
                "defaultFilter",
                defaults.default_filter.as_str(),
            ),
            (
                "key_separator",
                "keySeparator",
                defaults.key_separator.as_str(),
            ),
        ];
        let mut errors = Vec::new();
        let mut tab = "常规配置";
        for stage in [0..2, 2..4] {
            for i in stage {
                let (key, source_key, fallback) = values[i];
                let value = patch[source_key].as_str().unwrap_or(fallback);
                if let Some(error) = connection_form_error(key, value) {
                    errors.push(match error {
                        ConnectionFieldError::Required => "此项不能为空",
                        ConnectionFieldError::IllegalCharacters => "包含非法字符",
                    });
                }
            }
            if !errors.is_empty() {
                break;
            } else {
                tab = "高级配置";
            }
        }
        assert_eq!(
            errors.is_empty(),
            case["saved"].as_bool().unwrap(),
            "source save gate: {}",
            case["name"]
        );
        if !errors.is_empty() {
            assert_eq!(
                serde_json::json!(errors),
                case["errors"],
                "source feedback: {}",
                case["name"]
            );
            assert_eq!(
                tab,
                case["tab"].as_str().unwrap(),
                "source tab: {}",
                case["name"]
            );
        }
    }
}

#[test]
fn saving_connection_preserves_source_incomplete_transport_and_name_rules() {
    let mut connections = vec![];
    let profile = Connection {
        name: "  ".into(),
        network: "unix".into(),
        addr: String::new(),
        port: 0,
        sock: String::new(),
        ssh: Ssh {
            enable: true,
            addr: String::new(),
            username: String::new(),
            ..Default::default()
        },
        ..Default::default()
    };
    save_connection(&mut connections, None, profile.clone()).unwrap();
    let saved = flatten(&connections).pop().unwrap();
    assert_eq!(saved.name, profile.name);
    assert_eq!(saved.sock, "");
    assert!(saved.ssh.enable);
    assert_eq!(saved.port, 0);
    assert!(
        save_connection(
            &mut connections,
            None,
            Connection {
                name: "bad/name".into(),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        save_connection(
            &mut connections,
            None,
            Connection {
                name: "bad\\name".into(),
                ..Default::default()
            }
        )
        .is_ok(),
        "the backend only checks slash; the form checks both characters"
    );
}

struct RedisFixture {
    child: Child,
    dir: tempfile::TempDir,
    config: Connection,
}
impl RedisFixture {
    fn new() -> Self {
        Self::with_module(None)
    }
    fn with_module(module: Option<&std::path::Path>) -> Self {
        let dir = tempfile::Builder::new()
            .prefix("redisdock-test-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let socket = dir.path().join("redis.sock");
        let mut server = Command::new("redis-server");
        if let Some(module) = module { server.arg("--loadmodule").arg(module); }
        let child = server.args([
                "--port",
                "0",
                "--save",
                "",
                "--appendonly",
                "no",
                "--unixsocket",
            ])
            .arg(&socket)
            .arg("--logfile")
            .arg(dir.path().join("redis.log"))
            .current_dir(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("redis-server must be installed for integration tests");
        let config = Connection {
            name: "fixture".into(),
            network: "unix".into(),
            sock: socket.to_string_lossy().into_owned(),
            conn_timeout: 1,
            exec_timeout: 1,
            ..Default::default()
        };
        let mut fixture = Self { child, dir, config };
        let start = Instant::now();
        while Client::connect(&fixture.config, 0).is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(4),
                "Redis fixture did not start: {:?} {}",
                fixture.child.try_wait(),
                std::fs::read_to_string(fixture.dir.path().join("redis.log")).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(30));
        }
        fixture
    }
    fn service(&self) -> Service {
        Service::new(Store::new(self.dir.path().join("config")))
    }
}
impl Drop for RedisFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn resp_is_binary_safe_and_nested() {
    let v = redis::read_value(&mut Cursor::new(
        b"*4\r\n:42\r\n$4\r\n\0\xff\r\n\r\n$-1\r\n*2\r\n+OK\r\n-ERR nope\r\n",
    ))
    .unwrap();
    assert_eq!(
        v,
        Value::Array(vec![
            Value::Int(42),
            Value::Bytes(vec![0, 255, 13, 10]),
            Value::Null,
            Value::Array(vec![
                Value::Bytes(b"OK".to_vec()),
                Value::Error("ERR nope".into())
            ])
        ])
    );
    let frame = redis::encode_command(&vec![b"SET".to_vec(), vec![0, 255], b"a\r\nb".to_vec()]);
    assert!(frame.windows(7).any(|b| b == b"$4\r\na\r\n"));
}
#[test]
fn resp_rejects_malformed_lengths_and_truncation() {
    for b in [
        b"$-3\r\n".as_slice(),
        b"$3\r\na\r\n",
        b"*9999999999999\r\n",
        b"+OK\n",
    ] {
        assert!(redis::read_value(&mut Cursor::new(b)).is_err());
    }
}
#[test]
fn cli_parser_preserves_quoted_and_binary_args() {
    assert_eq!(
        redis::parse_command("SET 'key with space' \"a\\n\\xFF\" ").unwrap(),
        vec![
            b"SET".to_vec(),
            b"key with space".to_vec(),
            vec![b'a', 10, 255]
        ]
    );
    assert!(redis::parse_command("SET k ''").unwrap()[2].is_empty());
    assert!(redis::parse_command("SET 'unterminated").is_err());
}
#[test]
fn cluster_slots_respect_hash_tags() {
    assert_eq!(redis::hash_slot(b"123456789"), 12739);
    assert_eq!(
        redis::hash_slot(b"{account:1}:a"),
        redis::hash_slot(b"{account:1}:b")
    );
    assert_ne!(
        redis::hash_slot(b"{account:1}:a"),
        redis::hash_slot(b"{account:2}:a")
    );
}
#[test]
fn formats_round_trip_arbitrary_binary() {
    let b = [0, 255, 128, 10, 13, 65, 92];
    for f in ["Hex", "Binary", "BitSet"] {
        let text = codec::format_bytes(&b, f).unwrap();
        let mut got = codec::encode(&text, "None", f, &[]).unwrap();
        if f == "BitSet" {
            while got.len() > b.len() && got.last() == Some(&0) {
                got.pop();
            }
        }
        assert_eq!(got, b, "{f}");
    }
    assert_eq!(codec::format_bytes(&[255, 0], "Hex").unwrap(), "\\xFF\\x00");
}

#[test]
fn connection_form_save_clears_inactive_options_and_preserves_selected_transport() {
    let mut connection = Connection::default();
    connection.network = "unix".into();
    connection.sock = " /tmp/example.sock ".into();
    connection.ssl.cert_file = "unused-cert.pem".into();
    connection.ssh.password = "unused-password".into();
    connection.sentinel.password = "unused-sentinel-password".into();
    connection.db_filter_list = vec![1, 2];
    connection.proxy.kind = 1;
    connection.proxy.username = "unused-proxy-user".into();
    connection.proxy.password = "unused-proxy-password".into();
    connection.prepare_form_save(false);
    assert_eq!(connection.sock, "/tmp/example.sock");
    assert_eq!((connection.addr.as_str(), connection.port), ("", 0));
    assert!(connection.ssl.cert_file.is_empty());
    assert!(connection.ssh.password.is_empty());
    assert!(connection.sentinel.password.is_empty());
    assert!(connection.db_filter_list.is_empty());
    assert_eq!(connection.proxy.kind, 1);
    assert!(connection.proxy.username.is_empty());
    assert!(connection.proxy.password.is_empty());

    connection.network = "tcp".into();
    connection.addr = " localhost ".into();
    connection.ssh.enable = true;
    connection.ssh.login_type = "pkfile".into();
    connection.ssh.pk_file = "/tmp/key".into();
    connection.ssh.passphrase = "fixture-passphrase".into();
    connection.ssh.password = "old-password".into();
    connection.proxy.kind = 2;
    connection.proxy.username = "fixture-user".into();
    connection.proxy.password = "fixture-password".into();
    connection.prepare_form_save(true);
    assert_eq!(connection.addr, "localhost");
    assert!(connection.sock.is_empty());
    assert!(connection.ssh.password.is_empty());
    assert_eq!(connection.ssh.pk_file, "/tmp/key");
    assert_eq!(connection.ssh.passphrase, "fixture-passphrase");
    assert_eq!(connection.proxy.username, "fixture-user");
    connection.ssh.login_type = "agent".into();
    connection.prepare_form_save(false);
    assert!(connection.ssh.pk_file.is_empty());
    assert!(connection.ssh.passphrase.is_empty());
    assert!(connection.proxy.username.is_empty());
    assert!(connection.proxy.password.is_empty());
}

#[test]
fn unicode_binary_classification_matches_original_keys_and_auto_format() {
    for text in [
        "中文🙂",
        "e\u{301}",
        "a\t\n\r\u{b}\u{c}b",
        "\u{85}\u{2028}\u{2029}",
    ] {
        assert!(!contains_binary(text.as_bytes()), "{text:?}");
        assert_eq!(display_bytes(text.as_bytes()), text);
    }
    for text in [
        "\u{fffd}", "\u{7f}", "\u{1b}", "\u{200b}", "\u{e000}", "\u{fdd0}",
    ] {
        assert!(contains_binary(text.as_bytes()), "{text:?}");
        let converted = codec::convert(text.as_bytes(), "None", "Auto", &[]).unwrap();
        assert_eq!(converted.format, "Hex", "{text:?}");
        assert_eq!(
            codec::encode(&converted.text, "None", "Hex", &[]).unwrap(),
            text.as_bytes()
        );
    }
    assert!(contains_binary(b"\xff"));
    assert_eq!(
        display_bytes("key\u{200b}".as_bytes()),
        "key\\xE2\\x80\\x8B"
    );
    assert_eq!(
        codec::format_bytes(b"hello\0world", "Raw").unwrap(),
        "hello\0world"
    );
}

#[test]
fn automatic_decoders_skip_binary_base64_and_brotli_like_original() {
    let encoded = codec::encode("hello\0world", "Base64", "Raw", &[]).unwrap();
    assert_eq!(
        codec::convert(&encoded, "Auto", "Auto", &[])
            .unwrap()
            .decode,
        "None"
    );
    let encoded = codec::encode(
        "a repeated text a repeated text a repeated text",
        "Brotli",
        "Raw",
        &[],
    )
    .unwrap();
    assert_eq!(
        codec::convert(&encoded, "Auto", "Auto", &[])
            .unwrap()
            .decode,
        "None"
    );
    assert_eq!(
        codec::convert(b"  <root><value>1</value></root>  ", "None", "Auto", &[])
            .unwrap()
            .format,
        "XML"
    );
    assert_eq!(
        codec::convert(b"<root>", "None", "Auto", &[])
            .unwrap()
            .format,
        "Raw"
    );
}
#[test]
fn compression_round_trips_unicode_and_nuls() {
    let text = "你好 Rust + GPUI\0\n";
    for d in ["Base64", "GZip", "Deflate", "ZStd", "LZ4", "Brotli"] {
        let compressed = codec::encode(text, d, "Raw", &[]).unwrap();
        assert_eq!(
            codec::decode_bytes(&compressed, d, &[]).unwrap(),
            text.as_bytes(),
            "{d}"
        );
    }
}

#[test]
fn editing_json_preserves_number_lexemes_order_and_unicode_escapes() {
    let source =
        r#"{"z":9007199254740993,"tiny":1.234567890123456789e-42,"name":"\u4F60\u597D","a":[]}"#;
    let shown = codec::format_bytes(source.as_bytes(), "JSON").unwrap();
    assert_eq!(
        codec::encode(&shown, "None", "JSON", &[]).unwrap(),
        source.as_bytes()
    );
    let unicode = codec::format_bytes(source.as_bytes(), "Unicode JSON").unwrap();
    assert!(unicode.contains("你好"));
    assert!(unicode.contains("9007199254740993"));
    assert!(unicode.find('z') < unicode.find('a'));
    let yaml = "# keep the note\nname: '你好'\nids: [1, 2]\n";
    assert_eq!(codec::format_bytes(yaml.as_bytes(), "YAML").unwrap(), yaml);
    assert_eq!(
        codec::encode(yaml, "None", "YAML", &[]).unwrap(),
        yaml.as_bytes()
    );
}

#[test]
fn updating_connections_preserves_group_and_sibling_order() {
    let mut connections = vec![];
    for name in ["first", "middle", "last"] {
        save_connection(
            &mut connections,
            None,
            Connection {
                name: name.into(),
                group: "Servers".into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let mut middle = flatten(&connections)[1].clone();
    middle.name = "renamed".into();
    middle.port = 6380;
    save_connection(&mut connections, Some("middle"), middle).unwrap();
    assert_eq!(
        flatten(&connections)
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "renamed", "last"]
    );
    assert_eq!(flatten(&connections)[1].group, "Servers");
    assert!(
        save_connection(
            &mut connections,
            Some("missing"),
            Connection {
                name: "new".into(),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn dragging_connections_preserves_order_and_group_membership() {
    use redisdock::order::{Destination, move_item};
    let mut connections = vec![];
    for name in ["first", "middle", "last"] {
        save_connection(
            &mut connections,
            None,
            Connection {
                name: name.into(),
                group: "Servers".into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    move_item(
        &mut connections,
        "last",
        false,
        Destination::Before {
            name: "first".into(),
            group: false,
        },
    )
    .unwrap();
    assert_eq!(
        flatten(&connections)
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["last", "first", "middle"]
    );
    move_item(&mut connections, "first", false, Destination::Root).unwrap();
    assert!(
        flatten(&connections)
            .iter()
            .find(|item| item.name == "first")
            .unwrap()
            .group
            .is_empty()
    );
    move_item(
        &mut connections,
        "first",
        false,
        Destination::Group("Servers".into()),
    )
    .unwrap();
    assert_eq!(flatten(&connections).last().unwrap().name, "first");
    let before = connections.clone();
    assert!(
        move_item(
            &mut connections,
            "Servers",
            true,
            Destination::Group("Servers".into())
        )
        .is_err()
    );
    assert_eq!(connections, before);
}

#[test]
fn collection_order_and_substring_search_follow_original_behavior() {
    let f = RedisFixture::new();
    let s = f.service();
    let c = &f.config;
    let entries = ["first", "middle", "last"].map(|v| Entry {
        value: v.as_bytes().to_vec(),
        ..Default::default()
    });
    s.create(c, 0, b"ordered", KeyType::List, b"", &entries, -1)
        .unwrap();
    assert_eq!(
        s.detail(c, 0, b"ordered", 0, "", 100, "", false)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.value.clone())
            .collect::<Vec<_>>(),
        vec![b"last".to_vec(), b"middle".to_vec(), b"first".to_vec()]
    );
    s.add_entries(c, 0, b"ordered", KeyType::List, &entries, true, false, "*")
        .unwrap();
    let first = s.detail(c, 0, b"ordered", 0, "", 3, "", false).unwrap();
    assert_eq!(
        first
            .rows
            .iter()
            .map(|r| r.value.clone())
            .collect::<Vec<_>>(),
        entries.iter().map(|r| r.value.clone()).collect::<Vec<_>>()
    );
    let mut filtered = s.detail(c, 0, b"ordered", 0, "idd", 1, "", false).unwrap();
    assert!(!filtered.end, "a search batch does not pretend the full list was scanned");
    while !filtered.end {
        let next = s.detail(c, 0, b"ordered", filtered.cursor, "idd", 1, "", false).unwrap();
        filtered.rows.extend(next.rows); filtered.cursor=next.cursor; filtered.end=next.end;
    }
    assert_eq!(filtered.rows.len(), 2);
    assert_eq!(filtered.rows[0].id, b"1");
    s.create(
        c,
        0,
        b"hash",
        KeyType::Hash,
        b"",
        &[Entry {
            id: b"prefix-field-suffix".to_vec(),
            value: b"found".to_vec(),
            ..Default::default()
        }],
        -1,
    )
    .unwrap();
    assert_eq!(
        s.detail(c, 0, b"hash", 0, "field", 1, "", false)
            .unwrap()
            .rows
            .len(),
        1
    );
}

#[test]
fn exact_key_search_keeps_literal_backslashes_and_binary_fallback() {
    let f = RedisFixture::new();
    let s = f.service();
    let c = &f.config;
    s.create(c, 0, b"\\xFF:key", KeyType::String, b"literal", &[], -1)
        .unwrap();
    s.create(c, 0, b"\xff:key", KeyType::String, b"binary", &[], -1)
        .unwrap();
    assert_eq!(
        s.scan(c, 0, "\\xFF:key", "", true, ScanState::default(), 100)
            .unwrap()
            .keys[0]
            .key,
        b"\\xFF:key"
    );
    s.delete(c, 0, b"\\xFF:key", false).unwrap();
    assert_eq!(
        s.scan(c, 0, "\\xFF:key", "", true, ScanState::default(), 100)
            .unwrap()
            .keys[0]
            .key,
        b"\xff:key"
    );
}
#[test]
fn object_decoders_round_trip() {
    let text = "{\"name\":\"你好\",\"items\":[1,2],\"enabled\":true}";
    for d in ["Msgpack", "Pickle"] {
        let b = codec::encode(text, d, "JSON", &[]).unwrap();
        let decoded = codec::decode_bytes(&b, d, &[]).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&decoded).unwrap(),
            serde_json::from_str::<serde_json::Value>(text).unwrap(),
            "{d}"
        );
    }
}

#[test]
fn pickle_preserves_datetime_values_and_refuses_executable_globals() {
    let text = r#"{"created":"2026-10-13T09:00:00+08:00","name":"你好"}"#;
    let encoded = codec::encode(text, "Pickle", "JSON", &[]).unwrap();
    let decoded = codec::decode_bytes(&encoded, "Pickle", &[]).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&decoded).unwrap(),
        serde_json::from_str::<serde_json::Value>(text).unwrap()
    );
    let probe=codec::custom_command(&encoded,"python3",&["-c".into(),"import base64,pickle,datetime,sys; obj=pickle.loads(base64.b64decode(sys.argv[1])); print(base64.b64encode(str(isinstance(obj['created'],datetime.datetime)).encode()).decode())".into()]).unwrap();
    assert_eq!(probe, b"True");
    assert!(codec::decode_bytes(b"cos\nsystem\n(S'false'\ntR.", "Pickle", &[]).is_err());
}

#[test]
fn cli_selected_database_is_retained_by_dedicated_connections() {
    let f = RedisFixture::new();
    let mut cli = Client::connect(&f.config, 0).unwrap();
    cli.query(&command(&["SELECT", "2"])).unwrap();
    assert_eq!(cli.db, 2);
    assert!(cli.query(&command(&["SELECT", "-1"])).is_err());
    assert_eq!(cli.db, 2);
    cli.query(&command(&["SET", "cli-only", "value"])).unwrap();
    assert_eq!(
        cli.dedicated()
            .unwrap()
            .query(&command(&["GET", "cli-only"]))
            .unwrap()
            .bytes(),
        b"value"
    );
    assert_eq!(
        Client::connect(&f.config, 0)
            .unwrap()
            .query(&command(&["EXISTS", "cli-only"]))
            .unwrap()
            .integer()
            .unwrap(),
        0
    );
}
#[test]
fn client_list_reports_named_clients_and_their_selected_database() {
    let f = RedisFixture::new();
    let s = f.service();
    s.run(
        &f.config,
        3,
        command(&["CLIENT", "SETNAME", "rdm-client-list-test"]),
    )
    .unwrap();
    let clients = s.client_list(&f.config).unwrap();
    let client = clients
        .iter()
        .find(|client| {
            client
                .get("name")
                .is_some_and(|name| name == "rdm-client-list-test")
        })
        .unwrap();
    assert_eq!(client.get("db").unwrap(), "3");
    assert!(client.get("age").unwrap().parse::<u64>().is_ok());
    assert!(client.get("idle").unwrap().parse::<u64>().is_ok());
}
#[test]
fn blocked_command_does_not_block_another_connection_or_the_command_log() {
    let f = RedisFixture::new();
    let service = std::sync::Arc::new(f.service());
    let slow = Connection {
        name: "slow".into(),
        exec_timeout: 4,
        ..f.config.clone()
    };
    let fast = Connection {
        name: "fast".into(),
        ..f.config.clone()
    };
    service.connect(&slow, 0).unwrap();
    service.connect(&fast, 0).unwrap();
    let worker = service.clone();
    let thread = std::thread::spawn(move || {
        worker.run(&slow, 0, command(&["BLPOP", "never-produced", "2"]))
    });
    let start = Instant::now();
    while !service
        .run(&fast, 0, command(&["CLIENT", "LIST"]))
        .unwrap()
        .text()
        .contains("cmd=blpop")
    {
        assert!(start.elapsed() < Duration::from_secs(1));
        std::thread::sleep(Duration::from_millis(10));
    }
    let start = Instant::now();
    assert_eq!(
        service.run(&fast, 0, command(&["PING"])).unwrap().bytes(),
        b"PONG"
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(service.history.try_lock().is_some());
    assert_eq!(thread.join().unwrap().unwrap(), Value::Null);
}
#[test]
fn concurrent_atomic_writes_never_share_a_staging_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.txt");
    let contents = (0..4)
        .map(|index| format!("writer {index}\n").repeat(4096))
        .collect::<Vec<_>>();
    let threads = contents
        .iter()
        .map(|content| {
            let path = path.clone();
            let content = content.clone();
            std::thread::spawn(move || {
                redisdock::storage::atomic_write(&path, content.as_bytes()).unwrap()
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    let result = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains(&result));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
#[test]
fn raw_nuls_and_large_json_numbers_preserve_data() {
    let bytes = b"before\0after";
    let raw = codec::format_bytes(bytes, "Raw").unwrap();
    assert_eq!(codec::encode(&raw, "None", "Raw", &[]).unwrap(), bytes);
    assert_eq!(
        codec::convert(bytes, "None", "Auto", &[]).unwrap().format,
        "Hex"
    );
    let json = r#"{"very_large":1e999,"integer":184467440737095516160}"#;
    let formatted = codec::format_bytes(json.as_bytes(), "JSON").unwrap();
    assert_eq!(
        codec::encode(&formatted, "None", "JSON", &[]).unwrap(),
        json.as_bytes()
    );
}
#[test]
fn auto_format_detects_compressed_json() {
    let b = codec::encode("{\"a\":1}", "GZip", "JSON", &[]).unwrap();
    let c = codec::convert(&b, "Auto", "Auto", &[]).unwrap();
    assert_eq!(c.decode, "GZip");
    assert_eq!(c.format, "JSON");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&c.text).unwrap()["a"],
        1
    );
}
#[test]
fn custom_decoder_uses_base64_placeholder() {
    let d = Decoder {
        name: "identity".into(),
        enable: true,
        decode_path: "/bin/echo".into(),
        decode_args: vec!["{VALUE}".into()],
        encode_path: "/bin/echo".into(),
        encode_args: vec![],
        ..Default::default()
    };
    let b = vec![0, 255, 42];
    assert_eq!(
        codec::decode_bytes(&b, "identity", &[d.clone()]).unwrap(),
        b
    );
    assert_eq!(
        codec::encode("hello", "identity", "Raw", &[d]).unwrap(),
        b"hello"
    );
}
#[test]
fn original_auto_decode_rules_keep_empty_and_numeric_values() {
    let decoder = Decoder {
        name: "constant".into(),
        enable: false,
        auto: true,
        decode_path: "/bin/sh".into(),
        decode_args: vec!["-c".into(), "printf Y2hhbmdlZA==".into(), "{VALUE}".into()],
        ..Default::default()
    };
    let empty = codec::convert(b"", "Auto", "Auto", &[decoder.clone()]).unwrap();
    assert_eq!(
        (
            empty.text.as_str(),
            empty.decode.as_str(),
            empty.format.as_str()
        ),
        ("", "None", "Raw")
    );
    let number = codec::convert(b"1234", "Auto", "Auto", &[decoder]).unwrap();
    assert_eq!(number.text, "1234");
    assert_eq!(number.decode, "None");
    assert_eq!(number.format, "Raw");
}
#[test]
fn original_json_view_is_structure_based_and_save_is_lexical() {
    for text in ["1234", "true", "\"text\""] {
        assert_eq!(
            codec::convert(text.as_bytes(), "None", "Auto", &[])
                .unwrap()
                .format,
            "Raw"
        );
    }
    let draft = r#"{ "name" : "draft", "unfinished": }"#;
    assert_eq!(
        codec::encode(draft, "None", "JSON", &[]).unwrap(),
        br#"{"name":"draft","unfinished":}"#
    );
    assert_eq!(
        codec::convert(draft.as_bytes(), "None", "Auto", &[])
            .unwrap()
            .format,
        "JSON"
    );
    let binary = codec::encode("hello\0world", "Base64", "Raw", &[]).unwrap();
    assert_eq!(
        codec::convert(&binary, "Base64", "Raw", &[])
            .unwrap()
            .text
            .as_bytes(),
        binary
    );
    let old_pickle = b"(dp0\nVname\np1\nVTiny RDM\np2\ns.";
    assert_eq!(
        codec::convert(old_pickle, "Auto", "Auto", &[])
            .unwrap()
            .decode,
        "Pickle"
    );
}
#[test]
fn legacy_decoder_enable_field_is_ignored_as_in_original() {
    let decoder = Decoder {
        name: "legacy".into(),
        enable: false,
        auto: true,
        decode_path: "/bin/echo".into(),
        decode_args: vec!["{VALUE}".into()],
        encode_path: "/bin/echo".into(),
        encode_args: vec!["{VALUE}".into()],
        ..Default::default()
    };
    assert_eq!(
        codec::decode_bytes(b"value", "legacy", &[decoder.clone()]).unwrap(),
        b"value"
    );
    assert_eq!(
        codec::encode("value", "legacy", "Raw", &[decoder.clone()]).unwrap(),
        b"value"
    );
    assert_eq!(
        codec::convert(b"value", "Auto", "Auto", &[decoder])
            .unwrap()
            .decode,
        "legacy"
    );
}
#[test]
fn connection_yaml_and_json_match_original_names() {
    let y = "- name: Servers\n  type: group\n  connections:\n    - name: local\n      addr: 127.0.0.1\n      port: 6379\n      last_db: 2\n      refresh_interval: -1\n      key_separator: ':'\n      ssl:\n        certfile: client.pem\n        allow_insecure: true\n";
    let c: Vec<Connection> = serde_yaml::from_str(y).unwrap();
    let flat = flatten(&c);
    assert_eq!(flat[0].last_db, 2);
    assert_eq!(flat[0].refresh_interval, -1);
    assert_eq!(flat[0].group, "Servers");
    assert_eq!(flat[0].ssl.cert_file, "client.pem");
    let j = r#"{"name":"local","lastDB":4,"refreshInterval":-1,"connTimeout":10,"ssl":{"allowInsecure":true,"caFile":"ca.pem"}}"#;
    let c: Connection = serde_json::from_str(j).unwrap();
    assert_eq!(c.last_db, 4);
    assert_eq!(c.refresh_interval, -1);
    assert_eq!(c.conn_timeout, 10);
    assert_eq!(c.ssl.ca_file, "ca.pem");
}

#[test]
fn connection_profile_load_and_form_defaults_match_original_go_and_vue() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/connection-profile-source.json")).unwrap();
    let source_default: Connection = serde_json::from_value(corpus["formDefault"].clone()).unwrap();
    assert_eq!(Connection::form_default(), source_default);
    for case in corpus["records"].as_array().unwrap() {
        let loaded: Vec<Connection> = serde_yaml::from_str(case["yaml"].as_str().unwrap()).unwrap();
        let source_loaded: Connection = serde_json::from_value(case["loaded"].clone()).unwrap();
        assert_eq!(loaded[0], source_loaded, "source YAML: {}", case["name"]);
        let source_form: Connection = serde_json::from_value(case["form"].clone()).unwrap();
        assert_eq!(
            loaded[0].form_profile(),
            source_form,
            "source frontend merge: {}",
            case["name"]
        );
        let form = loaded[0].form_data();
        for (native, original) in [
            ("/port", "/port"),
            ("/conn_timeout", "/connTimeout"),
            ("/exec_timeout", "/execTimeout"),
            ("/key_view", "/keyView"),
            ("/load_size", "/loadSize"),
            ("/ssh/port", "/ssh/port"),
            ("/proxy/type", "/proxy/type"),
        ] {
            assert_eq!(
                form.pointer(native).unwrap(),
                case["form"]
                    .pointer(original)
                    .unwrap_or(&serde_json::Value::Null),
                "source nullable form field {native}: {}",
                case["name"]
            );
        }
    }
}

#[test]
fn connection_url_decodes_auth() {
    let c = Connection::from_url("rediss://user:p%40ss@localhost:6380/3").unwrap();
    assert_eq!(c.password, "p@ss");
    assert_eq!(c.last_db, 3);
    assert!(c.ssl.enable);
}
#[test]
fn preferences_ignore_retired_update_fields_and_preserve_editor_links() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    assert!(Preferences::default().editor.links);
    for (check, skip) in [("check_update", "skip_version"), ("checkUpdate", "skipVersion")] {
        std::fs::write(
            tmp.path().join("preferences.yaml"),
            format!("general:\n  {check}: true\n  {skip}: '1.2.3'\n  theme: dark\neditor:\n  links: false\n"),
        ).unwrap();
        let preferences = store.preferences().unwrap();
        assert_eq!(preferences.general.theme, "dark");
        assert!(!preferences.editor.links);
        store.save("preferences.yaml", &preferences).unwrap();
        let saved = serde_json::to_value(store.preferences().unwrap()).unwrap();
        for key in ["check_update", "skip_version", "checkUpdate", "skipVersion"] {
            assert!(saved["general"].get(key).is_none());
        }
        assert_eq!(saved["general"]["theme"], "dark");
        assert_eq!(saved["editor"]["links"], false);
    }
}
#[test]
fn welcome_choice_remembers_both_acceptance_and_rejection() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    for choice in [false, true] {
        let mut preferences = redisdock::model::Preferences::default();
        assert!(!preferences.behavior.welcomed);
        preferences.welcome_choice(choice);
        store.save("preferences.yaml", &preferences).unwrap();
        let loaded = store.preferences().unwrap();
        assert!(loaded.behavior.welcomed);
        assert_eq!(loaded.general.allow_track, choice);
    }
}
#[test]
fn storage_is_atomic_and_rejects_duplicate_names() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path().join("settings"));
    let mut connections = vec![];
    let c = Connection {
        name: "local".into(),
        group: "Test".into(),
        ..Default::default()
    };
    save_connection(&mut connections, None, c.clone()).unwrap();
    assert_eq!(connections[0].name, "Test");
    assert!(save_connection(&mut connections, None, c.clone()).is_err());
    store.save("connections.yaml", &connections).unwrap();
    assert_eq!(store.connections().unwrap(), connections);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(store.root.join("connections.yaml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let c2 = Connection {
        name: "renamed".into(),
        group: "New group".into(),
        ..c
    };
    save_connection(&mut connections, Some("local"), c2).unwrap();
    assert_eq!(flatten(&connections)[0].name, "renamed");
}

#[test]
fn connection_zip_import_replaces_profiles_without_form_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path().join("settings"));
    store
        .save(
            "connections.yaml",
            &vec![Connection {
                name: "existing".into(),
                ..Default::default()
            }],
        )
        .unwrap();
    let archive = tmp.path().join("source.zip");
    std::fs::write(
        &archive,
        include_bytes!("fixtures/connection-archive-source.zip"),
    )
    .unwrap();
    let imported = store.import_connections(&archive).unwrap();
    let flat = flatten(&imported);
    assert_eq!(
        flat.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["  ", "same", "same", "bad/name"]
    );
    assert_eq!(flat[0].sock, "");
    assert_eq!(flat[0].port, 0);
    assert!(flat[0].ssh.enable);
    assert_eq!(flat[0].ssh.port, 0);
    assert_eq!(flat[0].ssh.username, "");
    assert_eq!(flat[0].default_filter, "");
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/connection-archive-source.json")).unwrap();
    assert_eq!(
        std::fs::read_to_string(store.root.join("connections.yaml")).unwrap(),
        corpus["records"][0]["bytes"].as_str().unwrap(),
        "keep comments, unknown fields and omitted fields as source import does"
    );

    let mut sorted = imported.clone();
    redisdock::order::move_item(
        &mut sorted,
        "bad/name",
        false,
        redisdock::order::Destination::Group("imported".into()),
    )
    .expect("sorting imported profiles does not run connection form validation");
    assert_eq!(sorted[0].connections.len(), 3);
    assert_eq!(sorted[0].connections[2].name, "bad/name");
    assert_eq!(flatten(&sorted).len(), flat.len());
}

#[test]
fn connection_zip_export_preserves_source_yaml_bytes_and_deflate_format() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path().join("settings"));
    let original = tmp.path().join("original.zip");
    std::fs::write(
        &original,
        include_bytes!("fixtures/connection-archive-source.zip"),
    )
    .unwrap();
    store.import_connections(&original).unwrap();
    let exported = tmp.path().join("connections.zip");
    store.export_connections(&exported).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&exported).unwrap()).unwrap();
    assert_eq!(archive.len(), 1);
    let mut entry = archive.by_index(0).unwrap();
    assert_eq!(entry.name_raw(), b"connections.yaml");
    assert_eq!(entry.compression(), zip::CompressionMethod::Deflated);
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut bytes).unwrap();
    assert_eq!(
        bytes,
        std::fs::read(store.root.join("connections.yaml")).unwrap()
    );
    let restored = Store::new(tmp.path().join("restored"));
    restored.import_connections(&exported).unwrap();
    assert_eq!(
        std::fs::read(restored.root.join("connections.yaml")).unwrap(),
        bytes
    );
}

#[test]
fn connection_zip_import_entry_selection_matches_original_go_results() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::new(tmp.path());
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/connection-archive-source.json")).unwrap();
    let source_yaml = corpus["records"][0]["bytes"].as_str().unwrap();
    for record in corpus["records"].as_array().unwrap() {
        let name = record["name"].as_str().unwrap();
        let path = tmp.path().join(format!("{name}.zip"));
        match name {
            "source-export" => std::fs::write(
                &path,
                include_bytes!("fixtures/connection-archive-source.zip"),
            )
            .unwrap(),
            "first-match" => std::fs::write(
                &path,
                include_bytes!("fixtures/connection-archive-first-match.zip"),
            )
            .unwrap(),
            _ => {
                use std::io::Write;
                let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
                let entries = match name {
                    "last-entry-fallback" => vec![("note", "ignore"), ("other.yaml", source_yaml)],
                    "invalid-yaml" => vec![("connections.yaml", "- name: [\n")],
                    "empty" => vec![],
                    _ => panic!("unknown source case: {name}"),
                };
                for (filename, bytes) in entries {
                    writer
                        .start_file(filename, zip::write::FileOptions::default())
                        .unwrap();
                    writer.write_all(bytes.as_bytes()).unwrap();
                }
                writer.finish().unwrap();
            }
        }
        std::fs::write(store.root.join("connections.yaml"), "- name: existing\n").unwrap();
        if matches!(name, "last-entry-fallback" | "invalid-yaml" | "empty") {
            assert!(store.import_connections(&path).is_err(), "reject unsafe import: {name}");
            assert_eq!(std::fs::read_to_string(store.root.join("connections.yaml")).unwrap(), "- name: existing\n");
            continue;
        }
        let imported = store.import_connections(&path).unwrap();
        assert!(!imported.is_empty());
        assert_eq!(
            std::fs::read_to_string(store.root.join("connections.yaml")).unwrap(),
            record["bytes"].as_str().unwrap(),
            "source bytes: {name}"
        );
        if name == "invalid-yaml" {
            assert!(imported.is_empty());
        }
    }
    let invalid = tmp.path().join("not-a-zip.zip");
    std::fs::write(&invalid, b"invalid archive").unwrap();
    let before = std::fs::read(store.root.join("connections.yaml")).unwrap();
    assert!(store.import_connections(&invalid).is_err());
    assert_eq!(
        std::fs::read(store.root.join("connections.yaml")).unwrap(),
        before,
        "a rejected archive does not damage the previous configuration"
    );
}

#[test]
fn original_preferences_round_trip() {
    let p:Preferences=serde_yaml::from_str("general:\n  theme: dark\n  font_size: 16\n  scan_size: 100\neditor:\n  show_line_num: false\n").unwrap();
    assert_eq!(p.general.font_size, 16.);
    assert!(!p.editor.show_line_num);
    assert_eq!(p.behavior.aside_width, 300.);
}

#[test]
fn preference_numbers_match_go_rpc() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/preference-number-go-source.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let result = redisdock::model::Preferences::from_form_json(case["input"].clone());
        assert_eq!(
            result.is_ok(),
            case["success"].as_bool().unwrap(),
            "{} / {}",
            case["field"],
            case["raw"]
        );
        if let Ok(pref) = result {
            let actual = match case["field"].as_str().unwrap() {
                "general.fontSize" => pref.general.font_size as f64,
                "general.scanSize" => pref.general.scan_size as f64,
                "editor.fontSize" => pref.editor.font_size as f64,
                "cli.fontSize" => pref.cli.font_size as f64,
                _ => unreachable!(),
            };
            let expected = case["field"]
                .as_str()
                .unwrap()
                .split('.')
                .fold(&case["value"], |v, key| &v[key])
                .as_f64()
                .unwrap();
            assert_eq!(actual, expected);
        } else {
            assert_eq!(
                result.unwrap_err().to_string(),
                case["error"].as_str().unwrap()
            );
        }
    }
}

#[test]
fn preference_getter_matches_original_repairs() {
    let dir = tempfile::tempdir().unwrap();
    let store = redisdock::storage::Store::new(dir.path());
    let source = "general:\n  font_size: 0\n  scan_size: 0\nbehavior:\n  aside_width: 12\n  window_width: 100\n  window_height: 200\n";
    std::fs::write(dir.path().join("preferences.yaml"), source).unwrap();
    let pref = store.preferences().unwrap();
    assert_eq!(pref.general.font_size, 0.);
    assert_eq!(pref.general.scan_size, 3000);
    assert_eq!(pref.behavior.aside_width, 300.);
    assert_eq!(pref.behavior.window_width, 960.);
    assert_eq!(pref.behavior.window_height, 640.);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("preferences.yaml")).unwrap(),
        source
    );
    std::fs::write(dir.path().join("preferences.yaml"), "general: [invalid").unwrap();
    assert_eq!(store.preferences().unwrap().general.font_size, 14.);
}

#[test]
fn alias_indices_preserve_original_go_int_range() {
    let profile: redisdock::model::Connection = serde_json::from_value(serde_json::json!({
        "alias": {"-1": "imported", "4294967296": "large", "0": "main"}
    }))
    .unwrap();
    assert_eq!(profile.alias.get(&-1).unwrap(), "imported");
    assert_eq!(profile.alias.get(&4294967296).unwrap(), "large");
    let round_trip: redisdock::model::Connection =
        serde_yaml::from_str(&serde_yaml::to_string(&profile).unwrap()).unwrap();
    assert_eq!(round_trip.alias, profile.alias);
    for key in ["null", "1.5"] {
        let mut aliases = serde_json::Map::new();
        aliases.insert(key.into(), "fixture".into());
        assert!(
            serde_json::from_value::<redisdock::model::Connection>(
                serde_json::json!({"alias": aliases})
            )
            .is_err()
        );
    }
}
#[test]
fn redis_glob_patterns_match() {
    for (p, v, want) in [
        ("user:*", "user:42", true),
        ("a?c", "abc", true),
        ("[a-c]x", "bx", true),
        ("[^a]x", "ax", false),
        (r"a\*b", "a*b", true),
        ("[0-9]*", "abc", false),
    ] {
        assert_eq!(glob_match(p.as_bytes(), v.as_bytes()), want, "{p}");
    }
}

#[test]
fn redis_string_ttl_rename_binary_and_database_isolation() {
    let f = RedisFixture::new();
    let c = &f.config;
    let s = f.service();
    let key = [0, 255, b'k'];
    s.create(c, 0, &key, KeyType::String, &[0, 255, 10], &[], 30)
        .unwrap();
    let d = s.detail(c, 0, &key, 0, "", 100, "", false).unwrap();
    assert_eq!(d.value, vec![0, 255, 10]);
    assert!(d.ttl > 0 && d.ttl <= 30);
    s.set_string(c, 0, &key, b"edited".to_vec(), false).unwrap();
    assert!(s.detail(c, 0, &key, 0, "", 100, "", false).unwrap().ttl > 0);
    s.rename(c, 0, &key, b"renamed").unwrap();
    s.create(c, 1, b"renamed", KeyType::String, b"db1", &[], -1)
        .unwrap();
    assert_eq!(
        s.detail(c, 1, b"renamed", 0, "", 100, "", false)
            .unwrap()
            .value,
        b"db1"
    );
    assert_eq!(
        s.detail(c, 0, b"renamed", 0, "", 100, "", false)
            .unwrap()
            .value,
        b"edited"
    );
    s.ttl(c, 0, b"renamed", -1).unwrap();
    assert_eq!(
        s.detail(c, 0, b"renamed", 0, "", 100, "", false)
            .unwrap()
            .ttl,
        -1
    );
    s.delete(c, 0, b"renamed", true).unwrap();
    assert!(s.detail(c, 0, b"renamed", 0, "", 100, "", false).is_err());
}

fn check_new_key_source_backend(fixture: &RedisFixture, json_server: bool) {
    let corpus: serde_json::Value = serde_json::from_str(include_str!("fixtures/new-key-backend-source.json")).unwrap();
    let service = fixture.service();
    let config = &fixture.config;
    let key = corpus["key"].as_str().unwrap();
    for case in corpus["records"].as_array().unwrap().iter().filter(|case| case["jsonServer"] == json_server) {
        let name = case["name"].as_str().unwrap();
        service.run(config, 1, command(&["DEL", key])).unwrap();
        for setup in case["setup"].as_array().unwrap() {
            let setup = setup.as_array().unwrap();
            let mut args = vec![setup[0].as_str().unwrap().as_bytes().to_vec(), key.as_bytes().to_vec()];
            args.extend(setup[1..].iter().map(|v| v.as_str().unwrap().as_bytes().to_vec()));
            service.run(config, 1, args).unwrap();
        }
        let result = service.create_key(config, 1, key.as_bytes(), KeyType::parse(case["kind"].as_str().unwrap()), &case["value"], case["ttl"].as_i64().unwrap());
        assert_eq!(result.is_ok(), case["result"]["success"].as_bool().unwrap(), "{name}: {result:?}");
        if let Err(error) = result { assert_eq!(error.to_string(), case["result"]["msg"].as_str().unwrap(), "{name}"); }
        let kind = service.run(config, 1, command(&["TYPE", key])).unwrap().text();
        assert_eq!(kind, case["type"].as_str().unwrap(), "{name}");
        let pttl = service.run(config, 1, command(&["PTTL", key])).unwrap().integer().unwrap();
        let expected = case["pttl"].as_i64().unwrap();
        if expected < 0 { assert_eq!(pttl, expected, "{name}"); }
        else { assert!(pttl > 0 && (pttl - expected).abs() < 300, "{name} PTTL {pttl} vs {expected}"); }
        let run = |args:&[&str]| service.run(config,1,command(args)).unwrap();
        let texts = |value:Value| value.array().unwrap().iter().map(Value::text).collect::<Vec<_>>();
        let snapshot = match kind.as_str() {
            "none" => serde_json::Value::Null,
            "string" => serde_json::Value::String(run(&["GET",key]).text()),
            "hash" => {
                let pairs=texts(run(&["HGETALL",key]));
                serde_json::Value::Object(pairs.chunks_exact(2).map(|p|(p[0].clone(),serde_json::Value::String(p[1].clone()))).collect())
            }
            "list" => serde_json::json!(texts(run(&["LRANGE",key,"0","-1"]))),
            "set" => {
                let mut actual=texts(run(&["SMEMBERS",key]));actual.sort();
                let mut expected=case["snapshot"].as_array().unwrap().iter().map(|v|v.as_str().unwrap().to_string()).collect::<Vec<_>>();expected.sort();
                assert_eq!(actual,expected,"{name}");case["snapshot"].clone()
            }
            "zset" => {
                let actual=texts(run(&["ZRANGE",key,"0","-1","WITHSCORES"]));
                let actual=actual.chunks_exact(2).map(|p|(p[0].clone(),p[1].parse::<f64>().unwrap())).collect::<Vec<_>>();
                let expected=case["snapshot"].as_array().unwrap().iter().map(|p|(p[0].as_str().unwrap().to_string(),p[1].as_str().unwrap().parse::<f64>().unwrap())).collect::<Vec<_>>();
                assert_eq!(actual,expected,"{name}");case["snapshot"].clone()
            }
            "stream" => {
                let actual=run(&["XRANGE",key,"-","+"]);
                serde_json::Value::Array(actual.array().unwrap().iter().map(|row|{
                    let row=row.array().unwrap();let values=texts(row[1].clone());
                    serde_json::json!([row[0].text(),serde_json::Value::Object(values.chunks_exact(2).map(|p|(p[0].clone(),serde_json::Value::String(p[1].clone()))).collect())])
                }).collect())
            }
            "ReJSON-RL" => {
                assert_eq!(serde_json::from_str::<serde_json::Value>(&run(&["JSON.GET",key,"."]).text()).unwrap(),serde_json::from_str::<serde_json::Value>(case["snapshot"].as_str().unwrap()).unwrap(),"{name}");
                case["snapshot"].clone()
            }
            _ => panic!("unexpected type {kind}"),
        };
        assert_eq!(snapshot,case["snapshot"],"{name}");
        assert_eq!(service.run(config,0,command(&["DBSIZE"])).unwrap().integer().unwrap(),0,"creation must honor the chosen DB");
    }
}

#[test]
fn new_key_creation_matches_original_empty_rows_duplicates_and_ttl_behavior() {
    check_new_key_source_backend(&RedisFixture::new(),false);
}

#[test]
fn add_entries_matches_original_responses_data_errors_and_remaining_ttl() {
    use serde_json::{Value as Json,json};
    fn js_numbers(value:Json)->Json{match value{
        Json::Number(number)=>Json::Number(serde_json::Number::from_f64(number.as_f64().unwrap()).unwrap()),
        Json::Array(values)=>Json::Array(values.into_iter().map(js_numbers).collect()),
        Json::Object(values)=>Json::Object(values.into_iter().map(|(key,value)|(key,js_numbers(value))).collect()),
        value=>value,
    }}
    let corpus:Json=serde_json::from_str(include_str!("fixtures/add-entries-backend-source.json")).unwrap();
    let fixture=RedisFixture::new();let service=fixture.service();let config=&fixture.config;
    for sample in corpus["records"].as_array().unwrap() {
        let name=sample["name"].as_str().unwrap();let kind=KeyType::parse(sample["kind"].as_str().unwrap());
        let key=sample["key"].as_str().map(|s|s.as_bytes().to_vec()).unwrap_or_else(||sample["key"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect());
        let run=|head:&str,args:&[&str]|{
            let mut command=vec![head.as_bytes().to_vec(),key.clone()];command.extend(args.iter().map(|s|s.as_bytes().to_vec()));service.run(config,1,command).unwrap()
        };
        run("DEL",&[]);
        for setup in sample["setup"].as_array().unwrap() {
            let values=setup.as_array().unwrap();let args=values[1..].iter().map(|s|s.as_str().unwrap()).collect::<Vec<_>>();run(values[0].as_str().unwrap(),&args);
        }
        let result=service.add_key_entries(config,1,&key,kind.clone(),sample["action"].as_i64().unwrap() as i32,&sample["value"]);
        if let Some(error)=sample["bindingError"].as_str() {
            assert_eq!(result.unwrap_err().to_string(),error,"{name}");
        }else{
            assert_eq!(result.is_ok(),sample["result"]["success"].as_bool().unwrap(),"{name}: {result:?}");
            match result {
                Ok(data)=>{
                    let normalize=|mut value:Json|{
                        if kind==KeyType::ZSet {for key in ["added","updated"] {if let Some(rows)=value.get_mut(key).and_then(Json::as_array_mut){rows.sort_by_key(|row|row["v"].to_string());}}}
                        js_numbers(value)
                    };
                    assert_eq!(normalize(data),normalize(sample["result"]["data"].clone()),"{name} response");
                }
                Err(error)=>assert_eq!(error.to_string(),sample["result"]["msg"].as_str().unwrap(),"{name}"),
            }
        }
        let current=run("TYPE",&[]).text();assert_eq!(current,sample["type"].as_str().unwrap(),"{name}");
        let pttl=run("PTTL",&[]).integer().unwrap();let expected=sample["pttl"].as_i64().unwrap();
        if expected<0 {assert_eq!(pttl,expected,"{name}");}else{assert!(pttl>0&&(pttl-expected).abs()<300,"{name} TTL {pttl} vs {expected}");}
        let texts=|value:Value|value.array().unwrap().iter().map(Value::text).collect::<Vec<_>>();
        let snapshot=match current.as_str(){
            "none"=>Json::Null,
            "string"=>json!(run("GET",&[]).text()),
            "hash"=>{let values=texts(run("HGETALL",&[]));Json::Object(values.chunks_exact(2).map(|p|(p[0].clone(),json!(p[1]))).collect())}
            "list"=>json!(texts(run("LRANGE",&["0","-1"]))),
            "set"=>{let mut actual=texts(run("SMEMBERS",&[]));actual.sort();let mut expected=sample["snapshot"].as_array().unwrap().iter().map(|s|s.as_str().unwrap().to_string()).collect::<Vec<_>>();expected.sort();assert_eq!(actual,expected,"{name}");sample["snapshot"].clone()}
            "zset"=>{let values=texts(run("ZRANGE",&["0","-1","WITHSCORES"]));Json::Array(values.chunks_exact(2).map(|p|json!([p[0],p[1].parse::<f64>().unwrap()])).collect())}
            "stream"=>{let values=run("XRANGE",&["-","+"]);Json::Array(values.array().unwrap().iter().map(|row|{let row=row.array().unwrap();let fields=texts(row[1].clone());json!([row[0].text(),Json::Object(fields.chunks_exact(2).map(|p|(p[0].clone(),json!(p[1]))).collect())])}).collect())}
            _=>panic!("unexpected type {current}"),
        };
        assert_eq!(js_numbers(snapshot),js_numbers(sample["snapshot"].clone()),"{name} actual Redis data");
        if current!="none"{
            let start=service.history.lock().len();
            let summary=service.key_summary(config,1,&key).unwrap();assert_eq!(summary.kind,KeyType::parse(&current));
            assert!(service.history.lock().iter().skip(start).all(|log|!["HSCAN","SSCAN","ZRANGE","XRANGE","LRANGE","GET"].iter().any(|command|log.command.starts_with(command))),"summary must preserve the loaded page: {name}");
        }
        assert_eq!(service.run(config,0,command(&["DBSIZE"])).unwrap().integer().unwrap(),0);
    }
}

#[test]
fn edit_entries_match_original_responses_bytes_command_order_and_ttl() {
    use redisdock::service::EntryUpdate;
    use serde_json::{Value as Json,json};
    fn bytes(value:&Json)->Vec<u8>{
        value.as_str().map(|text|text.as_bytes().to_vec()).unwrap_or_else(||value.as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect())
    }
    fn numbers(value:Json)->Json {match value{
        Json::Number(n)=>json!(n.as_f64().unwrap()),
        Json::Array(values)=>Json::Array(values.into_iter().map(numbers).collect()),
        Json::Object(values)=>Json::Object(values.into_iter().map(|(key,value)|(key,numbers(value))).collect()),
        value=>value,
    }}
    let corpus:Json=serde_json::from_str(include_str!("fixtures/edit-entries-backend-source.json")).unwrap();
    let fixture=RedisFixture::new();let service=fixture.service();let config=&fixture.config;
    for original in corpus["records"].as_array().unwrap(){
        let mut sample = original.clone();
        // Approved safety differences; keep the captured original oracle unchanged.
        let name = original["name"].as_str().unwrap();
        let mut wire = sample["wire"].as_array().cloned().unwrap_or_default();
        if wire.len() == 2 && matches!(wire[0][0].as_str(), Some("hdel" | "srem" | "zrem")) {
            wire.swap(0, 1);
        }
        if name.starts_with("set-same-member") {
            wire.retain(|row| row[0] == "sadd");
            sample["result"]["data"] = json!({});
        }
        if name == "set-wrong-type" {
            wire.retain(|row| row[0] == "sadd");
            sample["result"] = json!({"success":false,"msg":"WRONGTYPE Operation against a key holding the wrong kind of value"});
        }
        if name.contains("invalid-hex") && sample["kind"] == "SET" {
            wire.clear();
            sample["snapshotHex"] = json!(sample["setup"].as_array().unwrap().iter().filter(|row| row[0].as_str().is_some_and(|op|op.eq_ignore_ascii_case("sadd"))).flat_map(|row| row.as_array().unwrap()[1..].iter()).map(|v|hex::encode(v.as_str().unwrap())).collect::<Vec<_>>());
            sample["type"] = json!("set");
            if name == "set-invalid-hex-removes-old-first" { sample["pttl"] = json!(-1); }
        }
        if name.ends_with("loses-ttl") { sample["pttl"] = json!(600000); }
        if name == "hash-rename-wrong-type" { wire.truncate(1); wire[0][0]=json!("hset"); }
        if name == "zset-wrong-type" { wire.truncate(1); wire[0][0]=json!("zadd"); }
        sample["wire"] = json!(wire);
        let param=&sample["param"];
        let kind=KeyType::parse(sample["kind"].as_str().unwrap());let key=bytes(&param["key"]);
        let run=|head:&str,args:&[&str]|{
            let mut all=vec![head.as_bytes().to_vec(),key.clone()];all.extend(args.iter().map(|s|s.as_bytes().to_vec()));service.run(config,1,all).unwrap()
        };
        run("DEL",&[]);
        for setup in sample["setup"].as_array().unwrap(){let args=setup.as_array().unwrap();run(args[0].as_str().unwrap(),&args[1..].iter().map(|s|s.as_str().unwrap()).collect::<Vec<_>>());}
        let text=|key:&str|param[key].as_str().unwrap_or_default().to_string();
        let update=EntryUpdate{field:text("field"),new_field:text("newField"),index:param["index"].as_i64().unwrap(),value:bytes(&param["value"]),new_value:bytes(&param["newValue"]),score:param["score"].as_f64().unwrap_or(0.),format:text("format"),decode:text("decode"),ret_format:text("retFormat"),ret_decode:text("retDecode")};
        let start=service.history.lock().len();
        let result=service.update_key_entry(config,1,&key,&kind,&update,&[]);
        assert_eq!(result.is_ok(),sample["result"]["success"].as_bool().unwrap(),"{name}: {result:?}");
        match result{
            Ok(data)=>assert_eq!(numbers(data),numbers(sample["result"]["data"].clone()),"{name} response"),
            Err(error)=>assert_eq!(error.to_string(),sample["result"]["msg"].as_str().unwrap(),"{name} error"),
        }
        let actual=service.history.lock().iter().skip(start).map(|entry|entry.command.split_ascii_whitespace().next().unwrap().to_ascii_lowercase()).collect::<Vec<_>>();
        let expected=sample["wire"].as_array().map(|rows|rows.iter().map(|row|row[0].as_str().unwrap().to_ascii_lowercase()).collect::<Vec<_>>()).unwrap_or_default();
        assert_eq!(actual,expected,"{name} mutation order");
        let current=run("TYPE",&[]).text();assert_eq!(current,sample["type"].as_str().unwrap(),"{name}");
        let ttl=run("PTTL",&[]).integer().unwrap();let expected=sample["pttl"].as_i64().unwrap();
        if expected<0{assert_eq!(ttl,expected,"{name} TTL");}else{assert!(ttl>0&&(ttl-expected).abs()<300,"{name} TTL {ttl} vs {expected}");}
        let hexes=|value:Value|value.array().unwrap().iter().map(|v|hex::encode(v.bytes())).collect::<Vec<_>>();
        let snapshot=match current.as_str(){
            "none"=>Json::Null,
            "string"=>json!(hex::encode(run("GET",&[]).bytes())),
            "hash"=>{let values=hexes(run("HGETALL",&[]));Json::Object(values.chunks_exact(2).map(|p|(p[0].clone(),json!(p[1]))).collect())},
            "list"=>json!(hexes(run("LRANGE",&["0","-1"]))),
            "set"=>{let mut values=hexes(run("SMEMBERS",&[]));values.sort();let mut expected=sample["snapshotHex"].as_array().unwrap().iter().map(|v|v.as_str().unwrap().to_string()).collect::<Vec<_>>();expected.sort();assert_eq!(values,expected,"{name} member bytes");sample["snapshotHex"].clone()},
            "zset"=>{let value=run("ZRANGE",&["0","-1","WITHSCORES"]);Json::Array(value.array().unwrap().chunks_exact(2).map(|p|json!([hex::encode(p[0].bytes()),p[1].text().parse::<f64>().unwrap()])).collect())},
            _=>panic!("unexpected Redis type {current}"),
        };
        assert_eq!(numbers(snapshot),numbers(sample["snapshotHex"].clone()),"{name} stored bytes");
        run("DEL",&[]);
        assert_eq!(service.run(config,0,command(&["DBSIZE"])).unwrap().integer().unwrap(),0);
    }
    assert_eq!(service.run(config,1,command(&["DBSIZE"])).unwrap().integer().unwrap(),0);
}

#[test]
#[ignore = "requires REDISDOCK_JSON_MODULE pointing to the installed RedisJSON module"]
fn new_key_json_creation_matches_original_default_invalid_and_existing_values() {
    let module=std::env::var_os("REDISDOCK_JSON_MODULE").expect("set REDISDOCK_JSON_MODULE");
    check_new_key_source_backend(&RedisFixture::with_module(Some(std::path::Path::new(&module))),true);
}

#[test]
#[ignore = "requires REDISDOCK_JSON_MODULE pointing to the installed RedisJSON module"]
fn redisjson_real_module_preserves_source_contracts_and_dump_restore() {
    let module = std::env::var_os("REDISDOCK_JSON_MODULE").expect("set REDISDOCK_JSON_MODULE to the RedisJSON library");
    let fixture = RedisFixture::with_module(Some(std::path::Path::new(&module)));
    let service = fixture.service();
    let config = &fixture.config;
    let payloads = [r#"{"name":"中文","nested":{"ok":true},"items":[1,null]}"#, r#"[1,{"x":2},"文本"]"#, r#""hello 世界""#, "123", "true", "null"];
    for (index, payload) in payloads.iter().enumerate() {
        let key = format!("json:{index}");
        let source_key = format!("source:{index}");
        // Original SetKeyValue uses the legacy root path; the Rust UI uses JSONPath.
        service.run(config, 0, vec![b"JSON.SET".to_vec(),source_key.as_bytes().to_vec(),b".".to_vec(),payload.as_bytes().to_vec()]).unwrap();
        service.create(config,0,key.as_bytes(),KeyType::Json,payload.as_bytes(),&[],-1).unwrap();
        let detail=service.detail(config,0,key.as_bytes(),0,"",100,"",false).unwrap();
        assert_eq!(detail.kind,KeyType::Json);
        assert_eq!(detail.length,0,"original JSON summary reports zero length");
        assert_eq!(detail.value,service.run(config,0,vec![b"JSON.GET".to_vec(),source_key.into_bytes()]).unwrap().bytes());
        assert_eq!(detail.ttl,-1);assert!(detail.memory>0);assert!(detail.end);
    }
    service.create(config,0,b"plain",KeyType::String,b"text",&[],-1).unwrap();
    let page=service.scan(config,0,"*","JSON",false,ScanState::default(),100).unwrap();
    assert_eq!(page.keys.len(),12);assert!(page.keys.iter().all(|key|key.kind==KeyType::Json));
    let binary=[0xff,0,b'J'];
    service.create(config,0,&binary,KeyType::Json,br#"{"before":1}"#,&[],30).unwrap();
    service.set_string(config,0,&binary,br#"{"changed":true,"items":[1,2]}"#.to_vec(),true).unwrap();
    let detail=service.detail(config,0,&binary,0,"",100,"",false).unwrap();assert!(detail.ttl>0&&detail.ttl<=30);
    assert!(service.set_string(config,0,&binary,b"{broken".to_vec(),true).is_err());
    assert_eq!(service.detail(config,0,&binary,0,"",100,"",false).unwrap().value,detail.value);
    service.create(config,1,&binary,KeyType::Json,b"[9]",&[],-1).unwrap();
    assert_eq!(service.detail(config,1,&binary,0,"",100,"",false).unwrap().value,b"[9]");
    let path=fixture.dir.path().join("json.csv");let cancel=redis::Cancellation::default();
    let exported=service.export_csv(config,0,&[binary.to_vec()],&path,true,&cancel,|_|{}).unwrap();assert_eq!(exported.done,1);
    let imported=service.import_csv(config,2,&path,false,-1,&cancel,|_|{}).unwrap();assert_eq!(imported.done,1);assert_eq!(imported.failed,0);
    let restored=service.detail(config,2,&binary,0,"",100,"",false).unwrap();assert_eq!(restored.kind,KeyType::Json);assert_eq!(restored.value,detail.value);assert!(restored.ttl>0);
    service.rename(config,0,&binary,b"renamed-json").unwrap();
    service.ttl(config,0,b"renamed-json",-1).unwrap();assert_eq!(service.detail(config,0,b"renamed-json",0,"",100,"",false).unwrap().ttl,-1);
    service.delete(config,0,b"renamed-json",false).unwrap();assert!(service.detail(config,0,b"renamed-json",0,"",100,"",false).is_err());
}

#[test]
fn cli_sessions_cannot_collide_with_real_connection_names_or_browser_databases() {
    let fixture = RedisFixture::new();
    let service = fixture.service();
    let config = &fixture.config;
    let mut alias = config.clone();
    alias.name = format!("cli:{}", config.name);
    service
        .run(config, 0, command(&["SET", "scoped", "browser"]))
        .unwrap();
    service
        .run_cli(config, 0, command(&["SELECT", "1"]))
        .unwrap();
    service
        .run_cli(config, 1, command(&["SET", "scoped", "console"]))
        .unwrap();
    assert_eq!(
        service
            .run(config, 0, command(&["GET", "scoped"]))
            .unwrap()
            .text(),
        "browser"
    );
    assert_eq!(
        service
            .run(&alias, 0, command(&["GET", "scoped"]))
            .unwrap()
            .text(),
        "browser"
    );
    assert_eq!(
        service
            .run_cli(config, 1, command(&["GET", "scoped"]))
            .unwrap()
            .text(),
        "console"
    );
    service
        .run_cli(&alias, 0, command(&["SELECT", "2"]))
        .unwrap();
    service
        .run_cli(&alias, 2, command(&["SET", "scoped", "alias console"]))
        .unwrap();
    service.close(&config.name);
    assert_eq!(
        service
            .run_cli(config, 0, command(&["GET", "scoped"]))
            .unwrap()
            .text(),
        "browser",
        "reopening resets only this CLI session to DB 0"
    );
    assert_eq!(
        service
            .run_cli(&alias, 2, command(&["GET", "scoped"]))
            .unwrap()
            .text(),
        "alias console",
        "closing the original name keeps the other CLI alive"
    );
    assert_eq!(
        service
            .run(&alias, 0, command(&["GET", "scoped"]))
            .unwrap()
            .text(),
        "browser"
    );
}
#[test]
fn redis_all_collection_types_and_entry_editing() {
    let f = RedisFixture::new();
    let c = &f.config;
    let s = f.service();
    let rows = vec![
        Entry {
            id: b"field1".to_vec(),
            value: b"value1".to_vec(),
            score: "1".into(),
            ..Default::default()
        },
        Entry {
            id: b"field2".to_vec(),
            value: b"value2".to_vec(),
            score: "2".into(),
            ..Default::default()
        },
    ];
    for (key, kind) in [
        ("hash", KeyType::Hash),
        ("list", KeyType::List),
        ("set", KeyType::Set),
        ("zset", KeyType::ZSet),
        ("stream", KeyType::Stream),
    ] {
        s.create(c, 0, key.as_bytes(), kind.clone(), b"*", &rows, -1)
            .unwrap();
        let d = s
            .detail(c, 0, key.as_bytes(), 0, "*", 100, "", false)
            .unwrap();
        assert_eq!(d.kind, kind);
        assert!(!d.rows.is_empty());
        if kind == KeyType::Stream {
            let matching = s.detail(c, 0, key.as_bytes(), 0, "\"field1\":value1", 100, "", false).unwrap();
            assert_eq!(matching.rows.len(), 1, "stream search uses the original display text");
            let json_only = s.detail(c, 0, key.as_bytes(), 0, "{", 100, "", false).unwrap();
            assert!(json_only.rows.is_empty(), "JSON serialization punctuation is not searchable stream text");
        }
        let old = d.rows[0].clone();
        if kind != KeyType::Stream {
            let new = Entry {
                id: if kind == KeyType::Hash {
                    b"renamed-field".to_vec()
                } else {
                    old.id.clone()
                },
                value: b"changed".to_vec(),
                score: "3.5".into(),
                ..Default::default()
            };
            s.edit_entry(c, 0, key.as_bytes(), &kind, &old, &new, false)
                .unwrap();
            let d = s
                .detail(c, 0, key.as_bytes(), 0, "*", 100, "", false)
                .unwrap();
            assert!(d.rows.iter().any(|r| r.value == b"changed"));
        }
        let d = s
            .detail(c, 0, key.as_bytes(), 0, "*", 100, "", false)
            .unwrap();
        let old = d.rows[0].clone();
        s.edit_entry(c, 0, key.as_bytes(), &kind, &old, &old, true)
            .unwrap();
        loop {
            let detail = s.detail(c, 0, key.as_bytes(), 0, "*", 100, "", false);
            match detail {
                Ok(detail) if detail.rows.is_empty() => {
                    assert_eq!(kind, KeyType::Stream, "only an empty stream keeps its key");
                    assert_eq!(detail.length, 0);
                    break;
                }
                Ok(detail) => {
                    let row = &detail.rows[0];
                    s.edit_entry(c, 0, key.as_bytes(), &kind, row, row, true).unwrap();
                }
                Err(error) => {
                    assert_ne!(kind, KeyType::Stream);
                    assert!(error.is::<redisdock::service::KeyNotFound>(), "{error}");
                    break;
                }
            }
        }
    }
}
#[test]
fn deleting_list_index_keeps_other_duplicate_values() {
    let f = RedisFixture::new();
    let s = f.service();
    let rows = ["same", "middle", "same"]
        .iter()
        .map(|v| Entry {
            value: v.as_bytes().to_vec(),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    s.create(&f.config, 0, b"list", KeyType::List, b"", &rows, -1)
        .unwrap();
    let old = Entry {
        id: b"2".to_vec(),
        value: b"same".to_vec(),
        ..Default::default()
    };
    s.edit_entry(&f.config, 0, b"list", &KeyType::List, &old, &old, true)
        .unwrap();
    assert_eq!(
        s.run(&f.config, 0, command(&["LRANGE", "list", "0", "-1"]))
            .unwrap(),
        Value::Array(vec![
            Value::Bytes(b"same".to_vec()),
            Value::Bytes(b"middle".to_vec())
        ])
    );
}
#[test]
fn scan_pages_filter_type_and_exact_key() {
    let f = RedisFixture::new();
    let c = &f.config;
    let s = f.service();
    for i in 0..80 {
        s.create(
            c,
            0,
            format!("user:{i}").as_bytes(),
            KeyType::String,
            b"x",
            &[],
            -1,
        )
        .unwrap();
    }
    s.create(c, 0, b"other", KeyType::String, b"x", &[], -1)
        .unwrap();
    let mut state = ScanState::default();
    let mut found = std::collections::HashSet::new();
    while !state.end {
        let p = s.scan(c, 0, "user:*", "STRING", false, state, 5).unwrap();
        for k in p.keys {
            assert_eq!(k.kind, KeyType::String);
            found.insert(k.key);
        }
        state = p.state;
    }
    assert_eq!(found.len(), 80);
    let p = s
        .scan(c, 0, "user:1", "", true, ScanState::default(), 5)
        .unwrap();
    assert_eq!(p.keys.len(), 1);
    assert!(p.state.end);
    let log = s.history.lock();
    assert!(log.iter().any(|entry| entry.command.starts_with("SCAN ")));
    assert!(log.iter().any(|entry| {
        entry
            .command
            .lines()
            .any(|command| command.starts_with("TYPE "))
    }));
}
#[test]
fn batch_ttl_uses_one_deadline_for_all_keys() {
    let fixture = RedisFixture::new();
    let service = fixture.service();
    let config = &fixture.config;
    let keys = vec![b"batch:first".to_vec(), b"batch:second".to_vec()];
    for key in &keys {
        service
            .create(config, 0, key, KeyType::String, b"value", &[], -1)
            .unwrap();
    }
    let progress = service
        .batch(
            config,
            0,
            &keys,
            Some(10),
            &Default::default(),
            |progress| {
                if progress.done == 1 {
                    std::thread::sleep(Duration::from_millis(1100));
                }
            },
        )
        .unwrap();
    assert_eq!(progress.done, 2);
    let expiry = |key: &[u8]| {
        service
            .run(config, 0, vec![b"EXPIRETIME".to_vec(), key.to_vec()])
            .unwrap()
            .integer()
            .unwrap()
    };
    assert_eq!(expiry(&keys[0]), expiry(&keys[1]));
    service
        .batch(config, 0, &keys, Some(-1), &Default::default(), |_| {})
        .unwrap();
    assert_eq!(expiry(&keys[0]), -1);
    assert_eq!(expiry(&keys[1]), -1);
}
#[test]
fn dump_csv_round_trip_and_conflict_policy() {
    let f = RedisFixture::new();
    let c = &f.config;
    let s = f.service();
    let k = vec![0, 255, 10];
    s.create(c, 0, &k, KeyType::String, &[0, 255, 13, 10], &[], 60)
        .unwrap();
    let path = f.dir.path().join("export.csv");
    let token = redis::Cancellation::default();
    let out = s
        .export_csv(c, 0, &[k.clone()], &path, true, &token, |_| {})
        .unwrap();
    assert_eq!(out.done, 1);
    let r = s.import_csv(c, 1, &path, true, -1, &token, |_| {}).unwrap();
    assert_eq!(r.done, 1);
    let d = s.detail(c, 1, &k, 0, "", 100, "", false).unwrap();
    assert_eq!(d.value, [0, 255, 13, 10]);
    assert!(d.ttl > 0);
    let r = s.import_csv(c, 1, &path, false, 0, &token, |_| {}).unwrap();
    assert_eq!(r.skipped, 1);
}
#[test]
fn import_expired_records_are_skipped() {
    let f = RedisFixture::new();
    let s = f.service();
    s.create(&f.config, 0, b"a", KeyType::String, b"b", &[], -1)
        .unwrap();
    let dump = s
        .run(&f.config, 0, command(&["DUMP", "a"]))
        .unwrap()
        .bytes();
    let path = f.dir.path().join("expired.csv");
    std::fs::write(
        &path,
        format!("{}, {},1\n", hex::encode(b"a"), hex::encode(dump)).replace(", ", ","),
    )
    .unwrap();
    let r = s
        .import_csv(
            &f.config,
            1,
            &path,
            true,
            -1,
            &redis::Cancellation::default(),
            |_| {},
        )
        .unwrap();
    assert_eq!(r.skipped, 1);
}
#[test]
fn canceled_bulk_operation_does_not_mutate() {
    let f = RedisFixture::new();
    let s = f.service();
    s.create(&f.config, 0, b"a", KeyType::String, b"b", &[], -1)
        .unwrap();
    let token = redis::Cancellation::default();
    token.cancel();
    let r = s
        .batch(&f.config, 0, &[b"a".to_vec()], None, &token, |_| {})
        .unwrap();
    assert!(r.canceled);
    assert_eq!(
        s.run(&f.config, 0, command(&["EXISTS", "a"]))
            .unwrap()
            .integer()
            .unwrap(),
        1
    );
}
#[test]
fn monitor_and_pubsub_use_independent_connections() {
    let f = RedisFixture::new();
    let c = Client::connect(&f.config, 0).unwrap();
    let mut subscriber = c.dedicated().unwrap();
    subscriber
        .send(&command(&["PSUBSCRIBE", "test:*"]))
        .unwrap();
    subscriber.read().unwrap();
    let mut publisher = Client::connect(&f.config, 0).unwrap();
    assert_eq!(
        publisher
            .query(&command(&["PUBLISH", "test:channel", "hello"]))
            .unwrap()
            .integer()
            .unwrap(),
        1
    );
    let m = subscriber.read().unwrap();
    let a = m.array().unwrap();
    assert_eq!(a[2].bytes(), b"test:channel");
    assert_eq!(a[3].bytes(), b"hello");
    let mut monitor = c.dedicated().unwrap();
    monitor.query(&command(&["MONITOR"])).unwrap();
    publisher.query(&command(&["SET", "key", "value"])).unwrap();
    assert!(
        monitor
            .read()
            .unwrap()
            .text()
            .contains("\"SET\" \"key\" \"value\"")
    );
}

#[test]
fn csv_matches_original_go_backend_samples() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!("fixtures/csv-backend-source.json")).unwrap();
    let fixture=RedisFixture::new();
    let service=fixture.service();
    let config=&fixture.config;
    let key=b"csv-source:value";
    let third=b"csv-source:third";
    service.run(config,0,vec![b"SET".to_vec(),key.to_vec(),corpus["value"].as_str().unwrap().as_bytes().to_vec()]).unwrap();
    let dump=hex::encode(service.run(config,0,vec![b"DUMP".to_vec(),key.to_vec()]).unwrap().bytes());
    let mut mismatches=Vec::new();
    for record in corpus["records"].as_array().unwrap() {
        let name=record["name"].as_str().unwrap();
        let path=fixture.dir.path().join(format!("{name}.csv"));
        let cancel=redis::Cancellation::default();
        let expected=&record["result"]["data"];
        if record["kind"]=="export" {
            let selected=if name=="export-missing-key" {b"csv-source:missing".to_vec()}else{key.to_vec()};
            let actual=service.export_csv(config,0,&[selected],&path,record["includeExpire"].as_bool().unwrap(),&cancel,|_|{
                if record["cancel"]==true {cancel.cancel();}
            }).unwrap();
            if actual.done as u64!=expected["exported"].as_u64().unwrap() || actual.failed as u64!=expected["failed"].as_u64().unwrap() || actual.canceled!=expected["canceled"].as_bool().unwrap() {
                mismatches.push(format!("{name}: {actual:?} vs {expected}"));
            }
            if actual.canceled { assert!(!path.exists(), "cancel must not publish a partial file"); continue; }
            let rows=csv::ReaderBuilder::new().has_headers(false).flexible(true).from_path(&path).unwrap().records().map(|row|row.unwrap().iter().map(str::to_string).collect::<Vec<_>>()).collect::<Vec<_>>();
            let expected_rows=record["rows"].as_array().map(|rows|rows.iter().map(|row|row.as_array().unwrap().iter().map(|value|value.as_str().unwrap().replace("$DUMP",&dump)).collect::<Vec<_>>()).collect::<Vec<_>>()).unwrap_or_default();
            if rows!=expected_rows {mismatches.push(format!("{name}: CSV {rows:?} vs {expected_rows:?}"));}
        } else {
            service.run(config,1,vec![b"DEL".to_vec(),key.to_vec(),third.to_vec()]).unwrap();
            if name=="import-conflict-ignore" {service.run(config,1,vec![b"SET".to_vec(),key.to_vec(),b"before".to_vec()]).unwrap();}
            if name=="import-broken-csv" {std::fs::write(&path,"\"unfinished\n").unwrap();} else {
                let mut writer=csv::WriterBuilder::new().flexible(true).from_path(&path).unwrap();
                for row in record["input"].as_array().unwrap() {
                    writer.write_record(row.as_array().unwrap().iter().map(|value|value.as_str().unwrap().replace("$DUMP",&dump))).unwrap();
                }
                writer.flush().unwrap();
            }
            let actual=service.import_csv(config,1,&path,record["conflict"]==0,record["ttl"].as_i64().unwrap(),&cancel,|_|{
                if record["cancel"]==true {cancel.cancel();}
            }).unwrap();
            if actual.done as u64!=expected["imported"].as_u64().unwrap() || (actual.failed+actual.skipped) as u64!=expected["ignored"].as_u64().unwrap() || actual.canceled!=expected["canceled"].as_bool().unwrap() {
                mismatches.push(format!("{name}: {actual:?} vs {expected}"));
            }
            let value=service.run(config,1,vec![b"GET".to_vec(),key.to_vec()]).unwrap().bytes();
            let expected_value=record["value"].as_str().unwrap_or("").as_bytes();
            let count=service.run(config,1,command(&["DBSIZE"])).unwrap().integer().unwrap();
            if value!=expected_value || count!=record["destinationCount"].as_i64().unwrap() {mismatches.push(format!("{name}: value {value:?}, count {count}"));}
        }
    }
    assert!(mismatches.is_empty(),"original Go CSV differences:\n{}",mismatches.join("\n"));
}

#[test]
fn csv_import_matches_original_go_parser_boundaries() {
    fn substitute(input: &[u8], needle: &[u8], value: &[u8]) -> Vec<u8> {
        let mut remaining = input;
        let mut result = Vec::new();
        while let Some(index) = remaining.windows(needle.len()).position(|part| part == needle) {
            result.extend_from_slice(&remaining[..index]);
            result.extend_from_slice(value);
            remaining = &remaining[index + needle.len()..];
        }
        result.extend_from_slice(remaining);
        result
    }
    let corpus: serde_json::Value = serde_json::from_str(include_str!("fixtures/csv-parser-source.json")).unwrap();
    let fixture = RedisFixture::new();
    let service = fixture.service();
    let config = &fixture.config;
    let key = corpus["key"].as_str().unwrap().as_bytes().to_vec();
    let other = corpus["otherKey"].as_str().unwrap().as_bytes().to_vec();
    service.run(config, 0, vec![b"SET".to_vec(), key.clone(), corpus["value"].as_str().unwrap().as_bytes().to_vec()]).unwrap();
    let dump = hex::encode(service.run(config, 0, vec![b"DUMP".to_vec(), key.clone()]).unwrap().bytes());
    let mut mismatches = Vec::new();
    for sample in corpus["records"].as_array().unwrap().iter().filter(|record| record["checkBackend"] == true) {
        let name = sample["name"].as_str().unwrap();
        service.run(config, 1, vec![b"DEL".to_vec(), key.clone(), other.clone(), vec![]]).unwrap();
        let raw = hex::decode(sample["inputHex"].as_str().unwrap()).unwrap();
        let raw = substitute(&raw, b"$KEY", hex::encode(&key).as_bytes());
        let raw = substitute(&raw, b"$OTHER", hex::encode(&other).as_bytes());
        let raw = substitute(&raw, b"$DUMP", dump.as_bytes());
        let path = fixture.dir.path().join("parser.csv");
        std::fs::write(&path, raw).unwrap();
        let mut events = Vec::new();
        let report = service.import_csv(config, 1, &path, true, -1, &redis::Cancellation::default(), |progress| {
            events.push(serde_json::json!({"imported": progress.done, "ignored": progress.failed + progress.skipped}));
        }).unwrap();
        let actual = serde_json::json!({"canceled": report.canceled, "imported": report.done, "ignored": report.failed + report.skipped});
        if actual != sample["result"]["data"] {
            mismatches.push(format!("{name}: result {actual} vs {}", sample["result"]["data"]));
        }
        let mut values = serde_json::Map::new();
        for found in service.run(config, 1, command(&["KEYS", "*"])).unwrap().array().unwrap() {
            let bytes = found.bytes();
            let value = service.run(config, 1, vec![b"GET".to_vec(), bytes.clone()]).unwrap().text();
            values.insert(hex::encode(bytes), serde_json::json!(value));
        }
        let expected_values = sample.get("values").cloned().unwrap_or_else(|| serde_json::json!({}));
        if serde_json::Value::Object(values.clone()) != expected_values {
            mismatches.push(format!("{name}: destination keys differ"));
        }
        let expected_events = sample.get("events").cloned().unwrap_or_else(|| serde_json::json!([]));
        if serde_json::json!(events) != expected_events {
            mismatches.push(format!("{name}: progress {} vs {expected_events}", serde_json::json!(events)));
        }
    }
    assert!(mismatches.is_empty(), "Go CSV parser/import differences:\n{}", mismatches.join("\n"));
}

#[test]
fn csv_import_ttl_matches_original_go_boundaries() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!("fixtures/csv-ttl-source.json")).unwrap();
    let fixture = RedisFixture::new();
    let service = fixture.service();
    let config = &fixture.config;
    let key = corpus["key"].as_str().unwrap().as_bytes().to_vec();
    service.run(config, 0, vec![b"SET".to_vec(), key.clone(), corpus["value"].as_str().unwrap().as_bytes().to_vec()]).unwrap();
    let dump = hex::encode(service.run(config, 0, vec![b"DUMP".to_vec(), key.clone()]).unwrap().bytes());
    let mut mismatches = Vec::new();
    for sample in corpus["records"].as_array().unwrap().iter().filter(|sample| sample["runtimeSafe"] == true) {
        let name = sample["name"].as_str().unwrap();
        service.run(config, 1, vec![b"DEL".to_vec(), key.clone()]).unwrap();
        let mut row = vec![hex::encode(&key).into_bytes(), dump.as_bytes().to_vec()];
        if let Some(expiry) = sample["expiryHex"].as_str() {
            row.push(hex::decode(expiry).unwrap());
        }
        let path = fixture.dir.path().join("ttl.csv");
        let mut writer = csv::Writer::from_path(&path).unwrap();
        writer.write_record(&row).unwrap();
        writer.flush().unwrap();
        let report = service.import_csv(config, 1, &path, true, sample["ttl"].as_i64().unwrap(), &redis::Cancellation::default(), |_| {}).unwrap();
        let actual = serde_json::json!({"canceled":report.canceled,"imported":report.done,"ignored":report.failed+report.skipped});
        if actual != sample["result"]["data"] {
            mismatches.push(format!("{name}: result {actual} vs {}", sample["result"]["data"]));
        }
        let pttl = service.run(config, 1, vec![b"PTTL".to_vec(), key.clone()]).unwrap().integer().unwrap();
        let source_pttl = sample["pttl"].as_i64().unwrap();
        let wire = sample["wireTtlMs"].as_i64().unwrap();
        let ttl_matches = if source_pttl < 0 {
            pttl == source_pttl
        } else {
            pttl >= 0 && pttl <= wire && pttl >= wire.saturating_sub(1000)
        };
        if !ttl_matches {
            mismatches.push(format!("{name}: PTTL {pttl}, original state {source_pttl}, wire TTL {wire}"));
        }
        let value = service.run(config, 1, vec![b"GET".to_vec(), key.clone()]).unwrap().bytes();
        if value != sample["value"].as_str().unwrap().as_bytes() {
            mismatches.push(format!("{name}: value lifetime differs"));
        }
    }
    assert!(mismatches.is_empty(), "Go CSV TTL differences:\n{}", mismatches.join("\n"));
}

#[test]
fn safe_entry_replacements_preserve_values_ttl_and_report_partial_failures() {
    use redisdock::service::EntryUpdate;
    let fixture = RedisFixture::new();
    let service = fixture.service();
    let c = &fixture.config;
    let run = |args: &[&str]| service.run(c, 0, command(args)).unwrap();
    for (kind, seed, read) in [(KeyType::Hash, "HSET", "HGET"), (KeyType::Set, "SADD", "SISMEMBER"), (KeyType::ZSet, "ZADD", "ZSCORE")] {
        let key = kind.name();
        match kind { KeyType::Hash => { run(&[seed, key, "old", "value"]); }, KeyType::ZSet => { run(&[seed, key, "1", "old"]); }, _ => { run(&[seed, key, "old"]); } }
        run(&["PEXPIRE", key, "120000"]);
        let param = EntryUpdate { field: "old".into(), new_field: "new".into(), value: b"old".to_vec(), new_value: b"new".to_vec(), score: 2., format: "Raw".into(), decode: "None".into(), ..Default::default() };
        service.update_key_entry(c, 0, key.as_bytes(), &kind, &param, &[]).unwrap();
        assert!(run(&["PTTL", key]).integer().unwrap() > 119000, "{key}: remaining TTL retained");
        assert!(!matches!(run(&[read, key, "new"]), Value::Null));
        if kind == KeyType::Set {
            let same = EntryUpdate { value: b"new".to_vec(), ..param };
            service.update_key_entry(c,0,key.as_bytes(),&kind,&same,&[]).unwrap();
            assert_eq!(run(&["SISMEMBER",key,"new"]).integer().unwrap(),1);
            assert!(run(&["PTTL",key]).integer().unwrap()>0);
        }
    }
    run(&["SADD","invalid","old"]);
    let invalid = EntryUpdate { value:b"old".to_vec(),new_value:b"zz".to_vec(),format:"Hex".into(),decode:"None".into(),..Default::default() };
    assert!(service.update_key_entry(c,0,b"invalid",&KeyType::Set,&invalid,&[]).is_err());
    assert_eq!(run(&["SISMEMBER","invalid","old"]).integer().unwrap(),1);
    run(&["ACL","SETUSER","safe-test","on",">fixture-password","~*","+ping","+sadd"]);
    let restricted = Connection { name:"restricted".into(),username:"safe-test".into(),password:"fixture-password".into(),..c.clone() };
    let param = EntryUpdate { value:b"old".to_vec(),new_value:vec![0,255,1],format:"Raw".into(),decode:"None".into(),..Default::default() };
    let error = service.update_key_entry(&restricted,0,b"invalid",&KeyType::Set,&param,&[]).unwrap_err();
    assert!(error.to_string().contains("新值已写入"));
    assert_eq!(run(&["SISMEMBER","invalid","old"]).integer().unwrap(),1);
    assert_eq!(service.run(c,0,vec![b"SISMEMBER".to_vec(),b"invalid".to_vec(),param.new_value]).unwrap().integer().unwrap(),1);
    run(&["ACL","SETUSER","safe-test","-sadd","+srem"]);
    assert!(service.update_key_entry(&restricted,0,b"invalid",&KeyType::Set,&EntryUpdate {value:b"old".to_vec(),new_value:b"denied".to_vec(),..Default::default()},&[]).is_err());
    assert_eq!(run(&["SISMEMBER","invalid","old"]).integer().unwrap(),1);
    run(&["ACL","DELUSER","safe-test"]);
}

#[test]
fn canceled_export_keeps_the_existing_backup_at_both_cancel_points() {
    let fixture = RedisFixture::new(); let service = fixture.service();
    for late in [false,true] {
        let path = fixture.dir.path().join("previous.csv");
        std::fs::write(&path,b"previous complete backup").unwrap();
        let cancel = redis::Cancellation::default();
        if !late { cancel.cancel(); }
        let report = service.export_csv(&fixture.config,0,&[b"missing".to_vec()],&path,false,&cancel,|_| { if late { cancel.cancel(); } }).unwrap();
        assert!(report.canceled);
        assert_eq!(std::fs::read(&path).unwrap(),b"previous complete backup");
        assert_eq!(std::fs::read_dir(fixture.dir.path()).unwrap().filter_map(Result::ok).filter(|f| f.file_name().to_string_lossy().starts_with(".redisdock-")).count(),0);
    }
}

#[test]
fn collection_search_resumes_from_scanned_rows_even_when_nothing_matches() {
    let fixture = RedisFixture::new(); let service = fixture.service(); let c=&fixture.config;
    for i in 0..73 {
        let id=i.to_string(); let text=if i%11==0 {"match"} else {"other"};
        for args in [command(&["RPUSH","paged-list",text]),command(&["XADD","paged-stream",&format!("{}-0",i+1),"v",text]),command(&["HSET","paged-hash",&format!("{text}-{id}"),text]),command(&["SADD","paged-set",&format!("{text}-{id}")]),command(&["ZADD","paged-zset",&id,&format!("{text}-{id}")])] { service.run(c,0,args).unwrap(); }
    }
    for key in ["paged-list","paged-stream","paged-hash","paged-set","paged-zset"] {
        let mut cursor=0;let mut after=String::new();let mut rows=std::collections::HashMap::new(); let mut batches=0;
        loop {
            let d=service.detail(c,0,key.as_bytes(),cursor,"match",5,&after,false).unwrap();
            batches+=1; assert!(batches<100);
            for row in d.rows { rows.insert(row.id.clone(),row); }
            cursor=d.cursor;after=d.stream_cursor;
            if d.end {break;}
        }
        assert_eq!(rows.len(),7,"all matching rows reached: {key}");
        if key=="paged-list" { for id in [0,11,22,33,44,55,66] {assert!(rows.contains_key(id.to_string().as_bytes()));} }
    }
}

#[test]
fn backup_failures_leave_existing_files_unchanged() {
    use std::io::Write;
    let dir=tempfile::tempdir().unwrap();let store=Store::new(dir.path());
    let original=b"- name: previous\n";std::fs::write(dir.path().join("connections.yaml"),original).unwrap();
    let archive=dir.path().join("corrupt.zip");
    let mut writer=zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    writer.start_file("connections.yaml",zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
    writer.write_all(b"- name: replacement\n").unwrap();writer.finish().unwrap();
    let mut bytes=std::fs::read(&archive).unwrap();
    let i=bytes.windows(b"replacement".len()).position(|b|b==b"replacement").unwrap();bytes[i]^=1;std::fs::write(&archive,bytes).unwrap();
    assert!(store.import_connections(&archive).is_err());
    assert_eq!(std::fs::read(dir.path().join("connections.yaml")).unwrap(),original);
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        let output=dir.path().join("previous.csv");std::fs::write(&output,b"complete backup").unwrap();
        std::fs::set_permissions(dir.path(),std::fs::Permissions::from_mode(0o500)).unwrap();
        let service=Service::new(store);
        let result=service.export_csv(&Connection::default(),0,&[],&output,false,&redis::Cancellation::default(),|_|{});
        std::fs::set_permissions(dir.path(),std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());assert_eq!(std::fs::read(&output).unwrap(),b"complete backup");
    }
}

#[test]
fn zset_equal_scores_keep_redis_member_order_across_pages() {
    let fixture = RedisFixture::new();
    let service = fixture.service();
    service.run(&fixture.config, 0, command(&["ZADD", "ties", "1", "match-a", "1", "match-b", "1", "match-c"])).unwrap();
    for reverse in [false, true] {
        let mut actual = Vec::new();
        let mut cursor = 0;
        loop {
            let page = service.detail(&fixture.config, 0, b"ties", cursor, "*", 2, "", reverse).unwrap();
            actual.extend(page.rows.into_iter().map(|row| row.id));
            cursor = page.cursor;
            if page.end { break; }
        }
        let mut expected = vec![b"match-a".to_vec(), b"match-b".to_vec(), b"match-c".to_vec()];
        if reverse { expected.reverse(); }
        assert_eq!(actual, expected);
    }
}
