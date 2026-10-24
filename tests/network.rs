use redisdock::{
    model::*,
    redis::{Client, command},
    service::Service,
    storage::Store,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn trust_ssh_fixture(config: &mut Connection) {
    let error = Client::connect(config, 0).err().expect("unknown fixture must not authenticate");
    let error = error.downcast_ref::<redisdock::redis::SshHostKeyError>().expect("typed unknown host error");
    assert!(error.expected.is_none());
    assert!(error.observed.fingerprint.starts_with("SHA256:"));
    for sentinel in [false, true] {
        let mut discovery = config.clone();
        discovery.sentinel.enable = sentinel;
        discovery.cluster.enable = !sentinel;
        let blocked = Client::connect(&discovery, 0).err().expect("discovery must verify SSH before authentication");
        assert!(blocked.downcast_ref::<redisdock::redis::SshHostKeyError>().unwrap().expected.is_none());
    }
    config.ssh.trusted_host = Some(error.observed.clone());
    let mut changed = config.clone();
    changed.ssh.trusted_host.as_mut().unwrap().fingerprint = "SHA256:changed-fixture-key".into();
    let error = Client::connect(&changed, 0).err().expect("changed fixture must not authenticate");
    assert!(error.downcast_ref::<redisdock::redis::SshHostKeyError>().unwrap().expected.is_some());
    let text = error.to_string();
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("Expected: SHA256:"));
    assert!(text.contains("Received: SHA256:"));
}

struct Server {
    child: Child,
    dir: tempfile::TempDir,
    port: u16,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}
fn redis_server(extra: &[String], tls: bool) -> Server {
    let dir = tempfile::Builder::new()
        .prefix("rdm-network-")
        .tempdir_in("/private/tmp")
        .unwrap();
    let port = port();
    let port_text = port.to_string();
    let mut cmd = Command::new("redis-server");
    cmd.args([
        "--bind",
        "127.0.0.1",
        "--save",
        "",
        "--appendonly",
        "no",
        "--port",
        if tls { "0" } else { &port_text },
    ])
    .args(extra)
    .arg("--logfile")
    .arg(dir.path().join("redis.log"))
    .current_dir(dir.path())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let child = cmd.spawn().unwrap();
    let server = Server { child, dir, port };
    if !tls {
        let start = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(4),
                "{}",
                std::fs::read_to_string(server.dir.path().join("redis.log")).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    server
}
struct Proxy {
    port: u16,
    stop: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}
impl Proxy {
    fn new(target: u16, socks: bool) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        count.fetch_add(1, Ordering::Relaxed);
                        stream.set_nonblocking(false).unwrap();
                        std::thread::spawn(move || {
                            if socks {
                                let mut hello = [0; 4];
                                stream.read_exact(&mut hello).unwrap();
                                assert_eq!(hello, [5, 2, 0, 2]);
                                stream.write_all(&[5, 2]).unwrap();
                                let mut h = [0; 2];
                                stream.read_exact(&mut h).unwrap();
                                let mut user = vec![0; h[1] as usize];
                                stream.read_exact(&mut user).unwrap();
                                let mut n = [0];
                                stream.read_exact(&mut n).unwrap();
                                let mut pass = vec![0; n[0] as usize];
                                stream.read_exact(&mut pass).unwrap();
                                assert_eq!(user, b"proxy-user");
                                assert_eq!(pass, b"proxy-pass");
                                stream.write_all(&[1, 0]).unwrap();
                                let mut req = [0; 4];
                                stream.read_exact(&mut req).unwrap();
                                assert_eq!(&req[..3], &[5, 1, 0]);
                                let length = match req[3] {
                                    1 => 4,
                                    4 => 16,
                                    3 => {
                                        let mut n = [0];
                                        stream.read_exact(&mut n).unwrap();
                                        n[0] as usize
                                    }
                                    kind => panic!("invalid SOCKS address type {kind}"),
                                };
                                let mut rest = vec![0; length + 2];
                                stream.read_exact(&mut rest).unwrap();
                                stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
                            } else {
                                let mut header = vec![];
                                while !header.ends_with(b"\r\n\r\n") {
                                    let mut b = [0];
                                    stream.read_exact(&mut b).unwrap();
                                    header.push(b[0]);
                                }
                                let header = String::from_utf8(header).unwrap();
                                assert!(header.starts_with("CONNECT "));
                                assert!(header.contains("Proxy-Authorization: Basic "));
                                stream
                                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                                    .unwrap();
                            }
                            let mut remote = TcpStream::connect(("127.0.0.1", target)).unwrap();
                            let mut remote_write = remote.try_clone().unwrap();
                            let mut stream_read = stream.try_clone().unwrap();
                            let outbound = std::thread::spawn(move || {
                                let _ = std::io::copy(&mut stream_read, &mut remote_write);
                                let _ = remote_write.shutdown(std::net::Shutdown::Write);
                            });
                            let _ = std::io::copy(&mut remote, &mut stream);
                            let _ = stream.shutdown(std::net::Shutdown::Both);
                            let _ = outbound.join();
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("proxy accept: {e}"),
                }
            }
        });
        Self {
            port,
            stop,
            accepted,
            thread: Some(thread),
        }
    }
}
#[test]
fn tcp_auth_and_both_authenticated_proxy_protocols() {
    let server = redis_server(&["--requirepass".into(), "redis-pass".into()], false);
    for socks in [false, true] {
        let proxy = Proxy::new(server.port, socks);
        let config = Connection {
            name: "proxy-test".into(),
            port: server.port,
            password: "redis-pass".into(),
            conn_timeout: 2,
            exec_timeout: 2,
            proxy: redisdock::model::Proxy {
                kind: 2,
                schema: if socks { "socks5h" } else { "http" }.into(),
                addr: "127.0.0.1".into(),
                port: proxy.port,
                username: "proxy-user".into(),
                password: "proxy-pass".into(),
            },
            ..Default::default()
        };
        let mut c = Client::connect(&config, 2)
            .unwrap_or_else(|e| panic!("{}: {e}", if socks { "SOCKS5" } else { "HTTP" }));
        c.query(&command(&["SET", "network:key", "works"])).unwrap();
        assert_eq!(
            c.query(&command(&["GET", "network:key"])).unwrap().bytes(),
            b"works"
        );
    }
}
#[test]
fn sentinel_uses_general_auth_for_sentinel_and_separate_master_auth() {
    let master = redis_server(&["--requirepass".into(), "master-pass".into()], false);
    let replica = redis_server(
        &[
            "--requirepass".into(),
            "master-pass".into(),
            "--masterauth".into(),
            "master-pass".into(),
            "--replicaof".into(),
            "127.0.0.1".into(),
            master.port.to_string(),
        ],
        false,
    );
    let sentinel_dir = tempfile::Builder::new()
        .prefix("rdm-sentinel-")
        .tempdir_in("/private/tmp")
        .unwrap();
    let port = port();
    let path = sentinel_dir.path().join("sentinel.conf");
    std::fs::write(&path,format!("port {port}\nbind 127.0.0.1\nprotected-mode no\nrequirepass sentinel-pass\nsentinel monitor mymaster 127.0.0.1 {} 1\nsentinel auth-pass mymaster master-pass\nsentinel down-after-milliseconds mymaster 5000\n",master.port)).unwrap();
    let child = Command::new("redis-server")
        .arg(&path)
        .arg("--sentinel")
        .arg("--logfile")
        .arg(sentinel_dir.path().join("sentinel.log"))
        .current_dir(sentinel_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _sentinel = Server {
        child,
        dir: sentinel_dir,
        port,
    };
    let start = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(start.elapsed() < Duration::from_secs(4));
        std::thread::sleep(Duration::from_millis(20));
    }
    let config = Connection {
        name: "sentinel".into(),
        port,
        password: "sentinel-pass".into(),
        sentinel: Sentinel {
            enable: true,
            master: "mymaster".into(),
            password: "master-pass".into(),
            username: String::new(),
        },
        conn_timeout: 2,
        exec_timeout: 2,
        ..Default::default()
    };
    let mut c = Client::connect(&config, 1).unwrap();
    c.query(&command(&["SET", "via-sentinel", "value"]))
        .unwrap();
    assert_eq!(
        c.query(&command(&["GET", "via-sentinel"])).unwrap().bytes(),
        b"value"
    );
    let service = Service::new(Store::new(master.dir.path().join("settings")));
    assert!(
        !service
            .sentinel_masters(&config)
            .unwrap()
            .array()
            .unwrap()
            .is_empty()
    );
    let mut replica_client = Client::connect(
        &Connection {
            port: replica.port,
            password: "master-pass".into(),
            conn_timeout: 2,
            exec_timeout: 2,
            ..Default::default()
        },
        1,
    )
    .unwrap();
    let started = Instant::now();
    while replica_client
        .query(&command(&["GET", "via-sentinel"]))
        .unwrap()
        .bytes()
        != b"value"
    {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "replica did not synchronize"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut sentinel_client = Client::connect(
        &Connection {
            port,
            password: "sentinel-pass".into(),
            conn_timeout: 2,
            exec_timeout: 2,
            ..Default::default()
        },
        0,
    )
    .unwrap();
    let started = Instant::now();
    loop {
        match sentinel_client.query(&command(&["SENTINEL", "FAILOVER", "mymaster"])) {
            Ok(_) => break,
            Err(error) => {
                assert!(started.elapsed() < Duration::from_secs(10), "{error}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    let started = Instant::now();
    loop {
        let address = sentinel_client
            .query(&command(&[
                "SENTINEL",
                "get-master-addr-by-name",
                "mymaster",
            ]))
            .unwrap();
        if address.array().unwrap()[1].text() == replica.port.to_string() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "sentinel did not switch master"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // The existing connection discovers the promoted master even while the
    // old master's demotion lags, and retains its selected database.
    c.query(&command(&["SET", "after-failover", "new-master"]))
        .unwrap();
    assert_eq!(c.primary.1, replica.port);
    assert_eq!(c.db, 1);
    assert_eq!(
        c.query(&command(&["GET", "via-sentinel"])).unwrap().bytes(),
        b"value"
    );
}
#[test]
fn tls_ca_sni_and_mutual_authentication() {
    let dir = tempfile::Builder::new()
        .prefix("rdm-tls-")
        .tempdir_in("/private/tmp")
        .unwrap();
    let key = dir.path().join("key.pem");
    let cert = dir.path().join("cert.pem");
    let port = port();
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
            "-addext",
            "extendedKeyUsage=serverAuth,clientAuth",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let child = Command::new("redis-server")
        .args([
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--tls-port",
            &port.to_string(),
            "--save",
            "",
            "--appendonly",
            "no",
            "--tls-cert-file",
        ])
        .arg(&cert)
        .arg("--tls-key-file")
        .arg(&key)
        .arg("--tls-ca-cert-file")
        .arg(&cert)
        .args(["--tls-auth-clients", "yes", "--logfile"])
        .arg(dir.path().join("redis.log"))
        .current_dir(dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let server = Server { child, dir, port };
    let start = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{}",
            std::fs::read_to_string(server.dir.path().join("redis.log")).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let config = Connection {
        name: "tls".into(),
        port,
        ssl: Ssl {
            enable: true,
            ca_file: cert.to_string_lossy().into_owned(),
            cert_file: cert.to_string_lossy().into_owned(),
            key_file: key.to_string_lossy().into_owned(),
            sni: "localhost".into(),
            ..Default::default()
        },
        conn_timeout: 2,
        exec_timeout: 2,
        ..Default::default()
    };
    let mut c = Client::connect(&config, 0).unwrap();
    assert_eq!(c.query(&command(&["PING"])).unwrap().bytes(), b"PONG");
    for scheme in ["http", "https"] {
        let proxy = Proxy::new(port, false);
        let mut tunneled = config.clone();
        tunneled.proxy = redisdock::model::Proxy {
            kind: 2,
            schema: scheme.into(),
            addr: "127.0.0.1".into(),
            port: proxy.port,
            username: "proxy-user".into(),
            password: "proxy-pass".into(),
        };
        assert_eq!(
            Client::connect(&tunneled, 0)
                .unwrap()
                .query(&command(&["PING"]))
                .unwrap()
                .bytes(),
            b"PONG"
        );
        tunneled.ssl.sni = "wrong.example.invalid".into();
        assert!(
            Client::connect(&tunneled, 0).is_err(),
            "Redis TLS still checks the destination name through {scheme} CONNECT"
        );
    }
    let mut missing_identity = config.clone();
    missing_identity.ssl.cert_file.clear();
    missing_identity.ssl.key_file.clear();
    assert!(Client::connect(&missing_identity, 0).is_err());
    let traditional = server.dir.path().join("rsa-key.pem");
    assert!(
        Command::new("openssl")
            .args(["rsa", "-traditional", "-in"])
            .arg(&key)
            .arg("-out")
            .arg(&traditional)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let mut rsa_config = config.clone();
    rsa_config.ssl.key_file = traditional.to_string_lossy().into_owned();
    assert_eq!(
        Client::connect(&rsa_config, 0)
            .unwrap()
            .query(&command(&["PING"]))
            .unwrap()
            .bytes(),
        b"PONG"
    );
    let mut wrong = config.clone();
    wrong.ssl.sni = "wrong.example.invalid".into();
    assert!(Client::connect(&wrong, 0).is_err());
    wrong.ssl.allow_insecure = true;
    assert!(Client::connect(&wrong, 0).is_ok());
}

#[test]
fn cluster_redirects_and_scans_every_master() {
    let mut base = 19000 + (std::process::id() % 1000) as u16;
    loop {
        if (0..3).all(|i| {
            TcpListener::bind(("127.0.0.1", base + i)).is_ok()
                && TcpListener::bind(("127.0.0.1", base + i + 10000)).is_ok()
        }) {
            break;
        }
        base += 3;
    }
    let mut nodes = vec![];
    for i in 0..3 {
        let dir = tempfile::Builder::new()
            .prefix("rdm-cluster-")
            .tempdir_in("/private/tmp")
            .unwrap();
        let port = base + i;
        let child = Command::new("redis-server")
            .args([
                "--bind",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--save",
                "",
                "--appendonly",
                "no",
                "--cluster-enabled",
                "yes",
                "--cluster-announce-ip",
                "127.0.0.1",
                "--cluster-config-file",
            ])
            .arg(dir.path().join("nodes.conf"))
            .arg("--logfile")
            .arg(dir.path().join("redis.log"))
            .current_dir(dir.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        nodes.push(Server { child, dir, port });
    }
    let start = Instant::now();
    for n in &nodes {
        while TcpStream::connect(("127.0.0.1", n.port)).is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(4),
                "{}",
                std::fs::read_to_string(n.dir.path().join("redis.log")).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let args = nodes
        .iter()
        .map(|n| format!("127.0.0.1:{}", n.port))
        .collect::<Vec<_>>();
    let output = Command::new("redis-cli")
        .args(["--cluster", "create"])
        .args(&args)
        .args(["--cluster-replicas", "0", "--cluster-yes"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config = Connection {
        name: "cluster".into(),
        port: base,
        cluster: Cluster { enable: true },
        conn_timeout: 2,
        exec_timeout: 2,
        ..Default::default()
    };
    let mut client = Client::connect(&config, 0).unwrap();
    // A successful write to one slot only proves that master's readiness.
    // Wait for every fixture master before writing keys across all slots.
    for node in &nodes {
        let mut direct = Client::connect(
            &Connection {
                port: node.port,
                cluster: Cluster { enable: false },
                ..config.clone()
            },
            0,
        )
        .unwrap();
        let started = Instant::now();
        loop {
            let info = direct.query(&command(&["CLUSTER", "INFO"])).unwrap().text();
            if info.lines().any(|line| line == "cluster_state:ok") {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(6), "{info}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let started = Instant::now();
    loop {
        match client.query(&command(&["SET", "cluster:ready", "1"])) {
            Ok(_) => break,
            Err(e) => {
                assert!(started.elapsed() < Duration::from_secs(6), "{e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
    for i in 0..90 {
        client
            .query(&command(&["SET", &format!("cluster:{i}"), &i.to_string()]))
            .unwrap();
    }
    let service = Service::new(Store::new(nodes[0].dir.path().join("settings")));
    let all = service
        .all_keys(&config, 0, "cluster:*", &Default::default())
        .unwrap();
    assert_eq!(all.len(), 91);
    assert_eq!(
        client
            .query(&command(&["GET", "cluster:42"]))
            .unwrap()
            .bytes(),
        b"42"
    );
    for node in &nodes {
        client
            .query_at(
                "127.0.0.1",
                node.port,
                &command(&["CONFIG", "SET", "slowlog-log-slower-than", "0"]),
            )
            .unwrap();
        client
            .query_at("127.0.0.1", node.port, &command(&["SLOWLOG", "RESET"]))
            .unwrap();
        client
            .query_at(
                "127.0.0.1",
                node.port,
                &command(&["ECHO", &format!("node:{}", node.port)]),
            )
            .unwrap();
    }
    let slow = service.slow_logs(&config, 100).unwrap();
    for node in &nodes {
        assert!(
            slow.iter()
                .any(|log| log.args == format!("ECHO node:{}", node.port))
        );
    }
    assert!(Client::connect(&config, 1).is_err());
}

#[test]
fn streaming_frame_survives_a_timeout_mid_response() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let worker = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.write_all(b"$10\r\nab").unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        s.write_all(b"cdefghij\r\n").unwrap();
    });
    let config = Connection {
        port,
        conn_timeout: 1,
        exec_timeout: 1,
        ..Default::default()
    };
    let mut wire = redisdock::redis::Wire::open(&config, "127.0.0.1", port, 0, false).unwrap();
    assert!(redisdock::redis::is_timeout(&wire.read().unwrap_err()));
    assert_eq!(wire.read().unwrap().bytes(), b"abcdefghij");
    worker.join().unwrap();
}

#[test]
fn https_proxy_uses_original_plain_connect_and_authenticated_redis() {
    let server = redis_server(&["--requirepass".into(), "redis-pass".into()], false);
    let proxy = Proxy::new(server.port, false);
    let config = Connection {
        name: "original-https-proxy".into(),
        port: server.port,
        password: "redis-pass".into(),
        conn_timeout: 2,
        exec_timeout: 2,
        proxy: redisdock::model::Proxy {
            kind: 2,
            schema: "https".into(),
            addr: "127.0.0.1".into(),
            port: proxy.port,
            username: "proxy-user".into(),
            password: "proxy-pass".into(),
        },
        // The original proxy does not read the Redis TLS CA when SSL is disabled.
        ssl: Ssl {
            ca_file: "/private/tmp/rdm-proxy-unused-ca".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut client = Client::connect(&config, 2).unwrap();
    client
        .query(&command(&["SET", "original-proxy", "fixture"]))
        .unwrap();
    assert_eq!(
        client
            .query(&command(&["GET", "original-proxy"]))
            .unwrap()
            .bytes(),
        b"fixture"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires local OpenSSH sshd and ssh-keygen"]
fn ssh_tunnel_uses_a_disposable_key_and_server() {
    let redis = redis_server(&[], false);
    let dir = tempfile::Builder::new()
        .prefix("rdm-ssh-")
        .tempdir()
        .unwrap();
    let host_key = dir.path().join("host.pem");
    let client_key = dir.path().join("client.pem");
    for key in [&host_key, &client_key] {
        assert!(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "rsa", "-b", "2048", "-m", "PEM", "-N", "", "-f"])
                .arg(key)
                .status()
                .unwrap()
                .success()
        );
    }
    let authorized = dir.path().join("authorized_keys");
    std::fs::copy(dir.path().join("client.pem.pub"), &authorized).unwrap();
    let username = String::from_utf8(Command::new("id").arg("-un").output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_string();
    let port = port();
    let conf = dir.path().join("sshd.conf");
    let log = dir.path().join("sshd.log");
    std::fs::write(&conf, format!("Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPidFile {}\nUsePAM no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nPubkeyAuthentication yes\nAllowUsers {username}\nAllowTcpForwarding local\nMaxSessions 0\nPermitOpen 127.0.0.1:{}\nLogLevel DEBUG\n", host_key.display(), authorized.display(), dir.path().join("sshd.pid").display(), redis.port)).unwrap();
    let child = Command::new("/usr/sbin/sshd")
        .arg("-D")
        .arg("-f")
        .arg(&conf)
        .arg("-E")
        .arg(&log)
        .current_dir(dir.path())
        .spawn()
        .unwrap();
    let mut ssh = Server { child, dir, port };
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            ssh.child.try_wait().unwrap().is_none() && started.elapsed() < Duration::from_secs(4),
            "{}",
            std::fs::read_to_string(&log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut config = Connection {
        name: "ssh-fixture".into(),
        port: redis.port,
        ssh: Ssh {
            enable: true,
            addr: "127.0.0.1".into(),
            port,
            username,
            login_type: "pkfile".into(),
            pk_file: client_key.to_string_lossy().into_owned(),
            ..Default::default()
        },
        conn_timeout: 3,
        exec_timeout: 3,
        ..Default::default()
    };
    trust_ssh_fixture(&mut config);
    let mut client = Client::connect(&config, 3).unwrap_or_else(|error| {
        panic!(
            "{error}\n{}",
            std::fs::read_to_string(&log).unwrap_or_default()
        )
    });
    client
        .query(&command(&["SET", "through-ssh", "tunnel-value"]))
        .unwrap();
    assert_eq!(
        client
            .query(&command(&["GET", "through-ssh"]))
            .unwrap()
            .bytes(),
        b"tunnel-value"
    );
    assert_eq!(client.db, 3);
}

#[test]
fn system_proxy_routes_real_connections() {
    if let Ok(raw) = std::env::var("RDM_PROXY_CHILD_CONFIG") {
        let config: Connection = serde_json::from_str(&raw).unwrap();
        let mut client = Client::connect(&config, 0).unwrap();
        assert_eq!(client.query(&command(&["PING"])).unwrap().bytes(), b"PONG");
        return;
    }
    let redis = redis_server(&["--requirepass".into(), "redis-pass".into()], false);
    for (name, scheme, variable, bypass, expected) in [
        ("http", "http", "ALL_PROXY", "", 1),
        ("https", "https", "ALL_PROXY", "", 1),
        ("socks5", "socks5", "ALL_PROXY", "", 1),
        ("socks5h", "socks5h", "ALL_PROXY", "", 1),
        ("lowercase", "http", "all_proxy", "", 1),
        ("http-only", "http", "HTTP_PROXY", "", 0),
        ("host-bypass", "http", "ALL_PROXY", "127.0.0.1", 0),
        ("cidr-bypass", "http", "ALL_PROXY", "127.0.0.0/8", 0),
        ("literal-asterisk", "http", "ALL_PROXY", "*", 1),
        ("unknown-scheme", "unknown", "ALL_PROXY", "", 0),
    ] {
        let proxy = Proxy::new(redis.port, scheme.starts_with("socks"));
        let config = Connection {
            name: format!("system-{name}"),
            port: redis.port,
            password: "redis-pass".into(),
            proxy: redisdock::model::Proxy {
                kind: 1,
                ..Default::default()
            },
            conn_timeout: 2,
            exec_timeout: 2,
            ..Default::default()
        };
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args([
            "--exact",
            "system_proxy_routes_real_connections",
            "--nocapture",
        ]);
        for variable in [
            "ALL_PROXY",
            "all_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            child.env_remove(variable);
        }
        child
            .env(
                variable,
                format!("{scheme}://proxy-user:proxy-pass@127.0.0.1:{}", proxy.port),
            )
            .env("NO_PROXY", bypass)
            .env(
                "RDM_PROXY_CHILD_CONFIG",
                serde_json::to_string(&config).unwrap(),
            );
        let result = child.output().unwrap();
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert_eq!(
            proxy.accepted.load(Ordering::Relaxed),
            expected,
            "source route for {name}"
        );
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires REDISDOCK_SSH_FIXTURE built from tests/fixtures/ssh-server"]
fn ssh_authentication_and_unix_forwarding_combinations() {
    if let Ok(raw) = std::env::var("RDM_SSH_AGENT_CHILD_CONFIG") {
        let configs: Vec<Connection> = serde_json::from_str(&raw).unwrap();
        for config in configs {
            let mut client = Client::connect(&config, 3)
                .unwrap_or_else(|error| panic!("{} agent: {error}", config.name));
            assert_eq!(client.query(&command(&["PING"])).unwrap().bytes(), b"PONG");
        }
        return;
    }
    struct Process(Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::Builder::new()
        .prefix("rdm-ssh-auth-")
        .tempdir_in("/private/tmp")
        .unwrap();
    let socket = dir.path().join("redis.sock");
    let redis = redis_server(
        &["--unixsocket".into(), socket.to_string_lossy().into_owned()],
        false,
    );
    let host_key = dir.path().join("host.pem");
    let client_key = dir.path().join("client.pem");
    for key in [&host_key, &client_key] {
        assert!(
            Command::new("ssh-keygen")
                .args(["-q", "-t", "rsa", "-b", "2048", "-m", "PEM", "-N", "", "-f"])
                .arg(key)
                .status()
                .unwrap()
                .success()
        );
    }
    let ssh_port = port();
    let log = dir.path().join("ssh.log");
    let ready = dir.path().join("ready.log");
    let mut server = Process(
        Command::new(
            std::env::var("REDISDOCK_SSH_FIXTURE")
                .expect("build tests/fixtures/ssh-server and set REDISDOCK_SSH_FIXTURE"),
        )
        .args([
            "-addr",
            &format!("127.0.0.1:{ssh_port}"),
            "-redis",
            &format!("127.0.0.1:{}", redis.port),
            "-unix",
        ])
        .arg(&socket)
        .arg("-host-key")
        .arg(&host_key)
        .arg("-authorized-key")
        .arg(client_key.with_extension("pem.pub"))
        .stdout(std::fs::File::create(&ready).unwrap())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .unwrap(),
    );
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", ssh_port)).is_err() {
        assert!(
            server.0.try_wait().unwrap().is_none() && started.elapsed() < Duration::from_secs(5),
            "{}",
            std::fs::read_to_string(&log).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut base = Connection {
        name: "ssh-auth".into(),
        port: redis.port,
        sock: socket.to_string_lossy().into_owned(),
        ssh: Ssh {
            enable: true,
            addr: "127.0.0.1".into(),
            port: ssh_port,
            username: "fixture-user".into(),
            password: "rdm-password-fixture".into(),
            pk_file: client_key.to_string_lossy().into_owned(),
            ..Default::default()
        },
        conn_timeout: 3,
        exec_timeout: 1,
        ..Default::default()
    };
    trust_ssh_fixture(&mut base);
    assert!(!std::fs::read_to_string(&log).unwrap().contains("AUTH "), "unknown and changed keys must stop before authentication");
    let agent_socket = dir.path().join("agent.sock");
    let mut agent = Process(
        Command::new("ssh-agent")
            .args(["-D", "-a"])
            .arg(&agent_socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let started = Instant::now();
    while std::os::unix::net::UnixStream::connect(&agent_socket).is_err() {
        assert!(
            agent.0.try_wait().unwrap().is_none() && started.elapsed() < Duration::from_secs(5)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        Command::new("ssh-add")
            .arg("-q")
            .arg(&client_key)
            .env("SSH_AUTH_SOCK", &agent_socket)
            .stdin(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let mut agent_configs = vec![];
    let mut proxies = vec![];
    for scheme in ["", "http", "https", "socks5", "socks5h"] {
        let proxy = (!scheme.is_empty()).then(|| Proxy::new(ssh_port, scheme.starts_with("socks")));
        for network in ["tcp", "unix"] {
            let mut config = base.clone();
            config.name = format!("ssh-{scheme}-{network}");
            config.network = network.into();
            if let Some(proxy) = &proxy {
                config.proxy = redisdock::model::Proxy {
                    kind: 2,
                    schema: scheme.into(),
                    addr: "127.0.0.1".into(),
                    port: proxy.port,
                    username: "proxy-user".into(),
                    password: "proxy-pass".into(),
                };
            }
            for login in ["password", "pkfile"] {
                config.ssh.login_type = login.into();
                let mut client = Client::connect(&config, 3)
                    .unwrap_or_else(|error| panic!("{} {login}: {error}", config.name));
                assert_eq!(client.query(&command(&["PING"])).unwrap().bytes(), b"PONG");
                if scheme.is_empty() && network == "unix" && login == "password" {
                    let started = Instant::now();
                    let error = client.query(&command(&["BLPOP", "empty-ssh-fixture", "2"])).unwrap_err();
                    assert!(redisdock::redis::is_timeout(&error), "{error}");
                    assert!(started.elapsed() < Duration::from_secs(2), "SSH observes the configured execution deadline");
                }
            }
            config.ssh.login_type = "agent".into();
            agent_configs.push(config);
        }
        if let Some(proxy) = proxy {
            proxies.push(proxy);
        }
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ssh_authentication_and_unix_forwarding_combinations",
            "--include-ignored",
            "--nocapture",
        ])
        .env("SSH_AUTH_SOCK", &agent_socket)
        .env(
            "RDM_SSH_AGENT_CHILD_CONFIG",
            serde_json::to_string(&agent_configs).unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "agent: {}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let mut rejected = base.clone();
    rejected.ssh.password = "wrong-fixture-password".into();
    assert!(Client::connect(&rejected, 0).is_err());
    let events = std::fs::read_to_string(&log).unwrap();
    for event in [
        "AUTH password",
        "AUTH publickey",
        "FORWARD tcp",
        "FORWARD unix",
    ] {
        assert!(events.contains(event), "fixture proof for {event}");
    }
}

#[test]
fn a_lost_write_reply_is_never_replayed() {
    let listener=TcpListener::bind(("127.0.0.1",0)).unwrap();
    let port=listener.local_addr().unwrap().port();
    let server=std::thread::spawn(move || {
        let (stream,_)=listener.accept().unwrap();
        let mut reader=std::io::BufReader::new(stream);
        assert_eq!(redisdock::redis::read_value(&mut reader).unwrap().array().unwrap()[0].bytes(),b"PING");
        reader.get_mut().write_all(b"+PONG\r\n").unwrap();
        assert_eq!(redisdock::redis::read_value(&mut reader).unwrap().array().unwrap()[0].bytes(),b"INCR");
        drop(reader);
        listener
    });
    let config=Connection {port,conn_timeout:1,exec_timeout:1,..Default::default()};
    let mut client=Client::connect(&config,0).unwrap();
    assert!(client.query(&command(&["INCR","counter"])).is_err());
    let listener=server.join().unwrap();listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),"no second connection may replay the uncertain write");
}
