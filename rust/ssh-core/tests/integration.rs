//! Integration coverage against the OpenSSH test server in `tests/docker`.
//!
//! The suite needs a server, so it runs only when `SSH_TEST_HOST` is set (the
//! CI job starts the container and sets it). Without it every test returns
//! immediately and `cargo test` stays hermetic.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ssh_core::{
    AuthMethod, ByteSink, ConnectConfig, Prompt, SessionHandler, ShellChannel, SshError, SshSession,
};

const TIMEOUT: Duration = Duration::from_secs(4_000);
const OUTPUT_WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(25);

struct Server {
    host: String,
    password_port: u16,
    keyboard_interactive_port: u16,
    silent_port: u16,
    user: String,
    password: String,
    keyboard_interactive_code: String,
    keys: PathBuf,
}

impl Server {
    fn from_env() -> Option<Self> {
        let host = std::env::var("SSH_TEST_HOST").ok()?;
        let port = |name: &str, default: u16| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default)
        };
        let variable =
            |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_string());
        let keys = std::env::var("SSH_TEST_KEYS").map_or_else(
            |_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")),
            PathBuf::from,
        );

        Some(Self {
            host,
            password_port: port("SSH_TEST_PASSWORD_PORT", 2222),
            keyboard_interactive_port: port("SSH_TEST_KEYBOARD_INTERACTIVE_PORT", 2223),
            silent_port: port("SSH_TEST_SILENT_PORT", 2224),
            user: variable("SSH_TEST_USER", "user"),
            password: variable("SSH_TEST_PASSWORD", "ssh-core-password"),
            keyboard_interactive_code: variable("SSH_TEST_KEYBOARD_INTERACTIVE_CODE", "424242"),
            keys,
        })
    }

    fn config(&self, port: u16, auth: AuthMethod) -> ConnectConfig {
        ConnectConfig {
            host: self.host.clone(),
            port,
            username: self.user.clone(),
            auth,
            connect_timeout_ms: TIMEOUT.as_millis() as u32,
            keepalive_interval_s: KEEPALIVE_INTERVAL_S,
        }
    }

    fn password_config(&self) -> ConnectConfig {
        let password = AuthMethod::Password {
            password: self.password.clone(),
        };
        self.config(self.password_port, password)
    }

    fn key_config(&self, name: &str, passphrase: Option<&str>) -> ConnectConfig {
        let key = AuthMethod::Key {
            private_key_openssh: self.key(name),
            passphrase: passphrase.map(str::to_string),
        };
        self.config(self.password_port, key)
    }

    fn key(&self, name: &str) -> String {
        let path = self.keys.join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
    }
}

const KEEPALIVE_INTERVAL_S: u32 = 5;

macro_rules! server {
    () => {
        match Server::from_env() {
            Some(server) => server,
            None => {
                eprintln!("skipped: SSH_TEST_HOST is not set");
                return;
            }
        }
    };
}

#[derive(Debug)]
struct SeenHostKey {
    host: String,
    port: u16,
    algorithm: String,
    public_key_base64: String,
    fingerprint_sha256: String,
}

struct TestHandler {
    accept_host_key: bool,
    answers: Vec<Vec<String>>,
    answer_by_prompt: Option<(String, String)>,
    rounds: Mutex<Vec<Vec<Prompt>>>,
    host_keys: Mutex<Vec<SeenHostKey>>,
    disconnects: AtomicUsize,
}

impl TestHandler {
    fn accepting() -> Self {
        Self {
            accept_host_key: true,
            answers: Vec::new(),
            answer_by_prompt: None,
            rounds: Mutex::new(Vec::new()),
            host_keys: Mutex::new(Vec::new()),
            disconnects: AtomicUsize::new(0),
        }
    }

    fn answering_by_prompt(code: String, password: String) -> Self {
        Self {
            answer_by_prompt: Some((code, password)),
            ..Self::accepting()
        }
    }

    fn answering(answers: Vec<Vec<String>>, accept_host_key: bool) -> Self {
        Self {
            accept_host_key,
            answers,
            ..Self::accepting()
        }
    }

    fn rounds(&self) -> Vec<Vec<Prompt>> {
        let mut rounds = self.rounds.lock().expect("rounds");
        std::mem::take(&mut *rounds)
    }

    fn host_keys(&self) -> Vec<SeenHostKey> {
        let mut keys = self.host_keys.lock().expect("host keys");
        std::mem::take(&mut *keys)
    }
}

#[async_trait]
impl SessionHandler for TestHandler {
    async fn verify_host_key(
        &self,
        host: String,
        port: u16,
        algorithm: String,
        public_key_base64: String,
        fingerprint_sha256: String,
    ) -> bool {
        self.host_keys.lock().expect("host keys").push(SeenHostKey {
            host,
            port,
            algorithm,
            public_key_base64,
            fingerprint_sha256,
        });

        self.accept_host_key
    }

    async fn keyboard_interactive(
        &self,
        name: String,
        _instructions: String,
        prompts: Vec<Prompt>,
    ) -> Option<Vec<String>> {
        assert!(name.len() < 256, "the server name is a short identifier");
        let answers = self.answer_by_prompt.as_ref().map(|(code, password)| {
            prompts
                .iter()
                .map(|prompt| {
                    if prompt.text.to_lowercase().contains("password") {
                        password.clone()
                    } else {
                        code.clone()
                    }
                })
                .collect::<Vec<String>>()
        });

        let round = {
            let mut rounds = self.rounds.lock().expect("rounds");
            let round = rounds.len();
            rounds.push(prompts);
            round
        };

        answers.or_else(|| self.answers.get(round).cloned())
    }

    fn on_disconnect(&self, _reason: String) {
        self.disconnects.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct CollectingSink {
    data: Mutex<Vec<u8>>,
    closed: Mutex<Option<(Option<u32>, Option<String>)>>,
}

impl CollectingSink {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.data.lock().expect("data")).to_string()
    }

    fn clear(&self) {
        self.data.lock().expect("data").clear();
    }

    fn closed(&self) -> Option<(Option<u32>, Option<String>)> {
        self.closed.lock().expect("closed").clone()
    }
}

impl ByteSink for CollectingSink {
    fn on_data(&self, bytes: Vec<u8>) {
        self.data.lock().expect("data").extend_from_slice(&bytes);
    }

    fn on_closed(&self, exit_status: Option<u32>, error: Option<String>) {
        *self.closed.lock().expect("closed") = Some((exit_status, error));
    }
}

async fn wait_for_text(sink: &CollectingSink, needle: &str) {
    let deadline = tokio::time::Instant::now() + OUTPUT_WAIT;

    loop {
        let text = sink.text();
        if text.contains(needle) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "waited {OUTPUT_WAIT:?} for {needle:?}; output so far: {text:?}"
        );
        tokio::time::sleep(POLL).await;
    }
}

async fn wait_for_close(sink: &CollectingSink) {
    let deadline = tokio::time::Instant::now() + OUTPUT_WAIT;

    while sink.closed().is_none() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "channel never reported a close; output: {:?}",
            sink.text()
        );
        tokio::time::sleep(POLL).await;
    }
}

/// Fails a test with `context` when the connection unexpectedly succeeds.
#[track_caller]
fn expect_failure(result: Result<Arc<SshSession>, SshError>, context: &str) -> SshError {
    match result {
        Ok(_) => panic!("{context}"),
        Err(error) => error,
    }
}

async fn password_session(server: &Server) -> (Arc<SshSession>, Arc<TestHandler>) {
    let handler = Arc::new(TestHandler::accepting());
    let session = SshSession::connect(
        server.password_config(),
        handler.clone() as Arc<dyn SessionHandler>,
    )
    .await
    .expect("password session");

    (session, handler)
}

async fn open_shell(
    server: &Server,
    cols: u32,
    rows: u32,
) -> (Arc<SshSession>, Arc<ShellChannel>, Arc<CollectingSink>) {
    let (session, _handler) = password_session(server).await;
    let sink = Arc::new(CollectingSink::default());
    let shell = session
        .open_shell(
            "xterm-256color".to_string(),
            cols,
            rows,
            Arc::clone(&sink) as Arc<dyn ByteSink>,
        )
        .await
        .expect("shell");

    (session, shell, sink)
}

#[tokio::test]
async fn password_authentication_succeeds() {
    let server = server!();
    let (session, handler) = password_session(&server).await;

    let seen = handler.host_keys();
    assert_eq!(seen.len(), 1, "one host key per key exchange");
    assert_eq!(seen[0].host, server.host);
    assert_eq!(seen[0].port, server.password_port);
    assert!(
        seen[0].algorithm.starts_with("ssh-") || seen[0].algorithm.starts_with("ecdsa-"),
        "unexpected algorithm {:?}",
        seen[0].algorithm
    );
    assert!(!seen[0].public_key_base64.is_empty());
    assert!(
        seen[0].fingerprint_sha256.starts_with("SHA256:"),
        "unexpected fingerprint {:?}",
        seen[0].fingerprint_sha256
    );

    session.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn password_authentication_rejects_a_wrong_password() {
    let server = server!();
    let mut config = server.password_config();
    config.auth = AuthMethod::Password {
        password: "not the password".to_string(),
    };
    let handler = Arc::new(TestHandler::accepting());

    let error = expect_failure(
        SshSession::connect(config, handler as Arc<dyn SessionHandler>).await,
        "the server must reject a wrong password",
    );

    assert!(
        matches!(error, SshError::AuthFailed { .. }),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn key_authentication_succeeds_for_every_supported_algorithm() {
    let server = server!();

    for (name, algorithm) in [
        ("id_ed25519", "ssh-ed25519"),
        ("id_rsa", "ssh-rsa"),
        ("id_ecdsa_p256", "ecdsa-sha2-nistp256"),
        ("id_ecdsa_p384", "ecdsa-sha2-nistp384"),
    ] {
        let handler = Arc::new(TestHandler::accepting());
        let session = SshSession::connect(
            server.key_config(name, None),
            handler as Arc<dyn SessionHandler>,
        )
        .await
        .unwrap_or_else(|error| panic!("{name} ({algorithm}): {error:?}"));

        let result = session.exec("echo key".to_string()).await.expect("exec");
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "key");
        session.disconnect().await.expect("disconnect");
    }
}

#[tokio::test]
async fn encrypted_key_authentication_needs_the_passphrase() {
    let server = server!();

    let handler = Arc::new(TestHandler::accepting());
    let session = SshSession::connect(
        server.key_config("id_ed25519_encrypted", Some("correct horse battery")),
        handler as Arc<dyn SessionHandler>,
    )
    .await
    .expect("the right passphrase authenticates");
    session.disconnect().await.expect("disconnect");

    let handler = Arc::new(TestHandler::accepting());
    let error = expect_failure(
        SshSession::connect(
            server.key_config("id_ed25519_encrypted", Some("wrong")),
            handler as Arc<dyn SessionHandler>,
        )
        .await,
        "a wrong passphrase must not authenticate",
    );

    assert!(
        matches!(error, SshError::WrongPassphrase),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn keyboard_interactive_answers_every_round() {
    let server = server!();
    let handler = Arc::new(TestHandler::answering_by_prompt(
        server.keyboard_interactive_code.clone(),
        server.password.clone(),
    ));
    let config = server.config(
        server.keyboard_interactive_port,
        AuthMethod::KeyboardInteractive,
    );

    let session = SshSession::connect(config, handler.clone() as Arc<dyn SessionHandler>)
        .await
        .expect("keyboard-interactive session");

    let prompts: Vec<Prompt> = handler.rounds().into_iter().flatten().collect();
    assert_eq!(prompts.len(), 2, "the PAM stack asks two questions");
    assert!(
        prompts
            .iter()
            .any(|prompt| prompt.text.contains("Verification code") && prompt.echo),
        "the code prompt echoes: {prompts:?}"
    );
    assert!(
        prompts
            .iter()
            .any(|prompt| prompt.text.contains("Password") && !prompt.echo),
        "the password prompt does not echo: {prompts:?}"
    );

    session.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn keyboard_interactive_can_be_cancelled() {
    let server = server!();
    let handler = Arc::new(TestHandler::answering(Vec::new(), true));
    let config = server.config(
        server.keyboard_interactive_port,
        AuthMethod::KeyboardInteractive,
    );

    let error = expect_failure(
        SshSession::connect(config, handler as Arc<dyn SessionHandler>).await,
        "declining the prompts must fail authentication",
    );

    assert!(
        matches!(error, SshError::AuthCancelled),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn host_key_rejection_aborts_the_connection() {
    let server = server!();
    let handler = Arc::new(TestHandler::answering(Vec::new(), false));

    let error = expect_failure(
        SshSession::connect(
            server.password_config(),
            handler.clone() as Arc<dyn SessionHandler>,
        )
        .await,
        "a rejected host key must abort",
    );

    assert!(
        matches!(error, SshError::HostKeyRejected),
        "unexpected error: {error:?}"
    );
    assert_eq!(handler.host_keys().len(), 1);
}

#[tokio::test]
async fn shell_round_trip_delivers_output_and_close() {
    let server = server!();
    let (session, shell, sink) = open_shell(&server, 80, 24).await;

    shell
        .write(b"echo hello-from-ssh\r".to_vec())
        .expect("write input");
    wait_for_text(&sink, "hello-from-ssh").await;

    shell.write(b"exit\r".to_vec()).expect("write exit");
    wait_for_close(&sink).await;
    let (exit_status, error) = sink.closed().expect("close event");
    assert_eq!(error, None);
    assert!(
        exit_status.is_none() || exit_status == Some(0),
        "unexpected exit status: {exit_status:?}"
    );

    session.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn shell_resize_reaches_the_remote_pty() {
    let server = server!();
    let (_session, shell, sink) = open_shell(&server, 80, 24).await;

    shell.write(b"stty size\r".to_vec()).expect("write");
    wait_for_text(&sink, "24 80").await;

    sink.clear();
    shell.resize(100, 40).expect("resize");
    shell.write(b"stty size\r".to_vec()).expect("write");
    wait_for_text(&sink, "40 100").await;
}

#[tokio::test]
async fn exec_reports_stdout_stderr_and_exit_status() {
    let server = server!();
    let (session, _handler) = password_session(&server).await;

    let result = session
        .exec("sh -c 'echo out; echo err >&2; exit 7'".to_string())
        .await
        .expect("exec");

    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "out");
    assert_eq!(String::from_utf8_lossy(&result.stderr).trim(), "err");
    assert_eq!(result.exit_status, Some(7));

    session.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn keepalive_keeps_an_idle_session_connected() {
    let server = server!();
    let mut config = server.password_config();
    config.keepalive_interval_s = 1;
    let handler = Arc::new(TestHandler::accepting());

    let session = SshSession::connect(config, handler.clone() as Arc<dyn SessionHandler>)
        .await
        .expect("session");

    tokio::time::sleep(Duration::from_secs(65)).await;

    let result = session
        .exec("echo alive".to_string())
        .await
        .expect("the session survives 65 idle seconds");
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "alive");
    assert_eq!(
        handler.disconnects.load(Ordering::SeqCst),
        0,
        "the server must not have dropped the session"
    );

    session.disconnect().await.expect("disconnect");
}

#[tokio::test]
async fn connect_timeout_bounds_a_silent_listener() {
    let server = server!();
    let mut config = server.password_config();
    config.port = server.silent_port;
    config.connect_timeout_ms = 1_500;
    let handler = Arc::new(TestHandler::accepting());

    let started = tokio::time::Instant::now();
    let error = expect_failure(
        SshSession::connect(config, handler as Arc<dyn SessionHandler>).await,
        "a listener that never speaks SSH must time out",
    );
    let waited = started.elapsed();

    assert!(
        matches!(error, SshError::Timeout { .. }),
        "unexpected error: {error:?}"
    );
    assert!(
        waited < Duration::from_secs(10),
        "the timeout fired after {waited:?}"
    );
}
