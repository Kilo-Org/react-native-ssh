//! Sessions: connect, authenticate, run commands, and open shell channels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use russh::client::{
    AuthResult, Config, DisconnectReason, Handle, KeyboardInteractiveAuthResponse,
};
use russh::keys::ssh_key;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyBase64, PublicKeyOrCertificate};
use russh::{Channel, ChannelMsg, Disconnect};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::error::SshError;
use crate::key::load_private_key;
use crate::runtime;
use crate::shell::{ByteSink, ShellChannel};

/// The keepalives a session may miss before russh gives up on the server.
const KEEPALIVE_MAX: usize = 3;
/// Unprocessed packets russh buffers per channel before backpressure.
const CHANNEL_BUFFER_SIZE: usize = 256;

#[derive(Debug, uniffi::Record)]
pub struct ConnectConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    /// Budget for reaching an authenticated session. Prompts answered by the
    /// session handler do not count against it.
    pub connect_timeout_ms: u32,
    /// Idle keepalive period; `0` disables keepalives.
    pub keepalive_interval_s: u32,
}

#[derive(Debug, uniffi::Enum)]
pub enum AuthMethod {
    Password {
        password: String,
    },
    Key {
        private_key_openssh: String,
        passphrase: Option<String>,
    },
    KeyboardInteractive,
}

#[derive(Debug, uniffi::Record)]
pub struct Prompt {
    pub text: String,
    /// Whether the answer may be displayed as it is typed.
    pub echo: bool,
}

#[derive(Debug, uniffi::Record)]
pub struct ExecResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_status: Option<u32>,
}

/// Decisions the core cannot make on its own: trust in the server and answers
/// to interactive authentication prompts.
///
/// Both methods are awaited inside the connection, so the implementation may
/// suspend for as long as the user needs.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait SessionHandler: Send + Sync {
    /// Called once per key exchange with the server's host key. Returning
    /// `false` aborts the connection with [`SshError::HostKeyRejected`].
    async fn verify_host_key(
        &self,
        host: String,
        port: u16,
        algorithm: String,
        public_key_base64: String,
        fingerprint_sha256: String,
    ) -> bool;

    /// Called with each question the server asks. Returning `None` aborts
    /// authentication with [`SshError::AuthCancelled`].
    async fn keyboard_interactive(
        &self,
        name: String,
        instructions: String,
        prompts: Vec<Prompt>,
    ) -> Option<Vec<String>>;

    /// Called when the connection ends without the app asking for it.
    fn on_disconnect(&self, reason: String);
}

#[derive(uniffi::Object)]
pub struct SshSession {
    handle: Mutex<Handle<ClientHandler>>,
    closed_by_app: Arc<AtomicBool>,
}

#[uniffi::export]
impl SshSession {
    /// Connects and authenticates, reporting host-key and prompt decisions to
    /// `handler` while it does.
    #[uniffi::constructor]
    pub async fn connect(
        config: ConnectConfig,
        handler: Arc<dyn SessionHandler>,
    ) -> Result<Arc<Self>, SshError> {
        runtime::run(connect(config, handler)).await
    }

    /// Opens a pty-backed shell whose output goes to `sink`.
    pub async fn open_shell(
        &self,
        term: String,
        cols: u32,
        rows: u32,
        sink: Arc<dyn ByteSink>,
    ) -> Result<Arc<ShellChannel>, SshError> {
        let handle = self.handle.lock().await;
        let mut channel = handle.channel_open_session().await?;

        channel
            .request_pty(true, &term, cols, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;
        await_success(&mut channel).await?;
        await_success(&mut channel).await?;

        Ok(Arc::new(ShellChannel::start(channel, sink)))
    }

    /// Runs one command against a fresh session channel.
    pub async fn exec(&self, command: String) -> Result<ExecResult, SshError> {
        let handle = self.handle.lock().await;
        let mut channel = handle.channel_open_session().await?;
        channel.exec(true, command.into_bytes()).await?;

        let mut result = ExecResult {
            stdout: Vec::new(),
            stderr: Vec::new(),
            exit_status: None,
        };

        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => result.stdout.extend_from_slice(&data),
                Some(ChannelMsg::ExtendedData { data, ext }) => {
                    if ext == STDERR_EXTENDED_DATA {
                        result.stderr.extend_from_slice(&data);
                    } else {
                        result.stdout.extend_from_slice(&data);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => {
                    result.exit_status = Some(exit_status);
                }
                Some(ChannelMsg::Eof) => {}
                Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            }
        }

        Ok(result)
    }

    /// Ends the connection. Shell channels close and report it through their
    /// sink; [`SessionHandler::on_disconnect`] is not called.
    pub async fn disconnect(&self) -> Result<(), SshError> {
        if self.closed_by_app.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let handle = self.handle.lock().await;
        handle
            .disconnect(Disconnect::ByApplication, "", "")
            .await
            .map_err(SshError::from)
    }
}

/// RFC 4254 extended data type for stderr.
const STDERR_EXTENDED_DATA: u32 = 1;

async fn connect(
    config: ConnectConfig,
    handler: Arc<dyn SessionHandler>,
) -> Result<Arc<SshSession>, SshError> {
    let awaiting_user = Arc::new(AtomicBool::new(false));
    let host_key_rejected = Arc::new(AtomicBool::new(false));
    let closed_by_app = Arc::new(AtomicBool::new(false));

    let client_config = Arc::new(client_config(&config));
    let timeout = Duration::from_millis(u64::from(config.connect_timeout_ms.max(1)));
    let address = (config.host.clone(), config.port);
    let client_handler = ClientHandler {
        host: address.0.clone(),
        port: address.1,
        app: Arc::clone(&handler),
        awaiting_user: Arc::clone(&awaiting_user),
        host_key_rejected: Arc::clone(&host_key_rejected),
        closed_by_app: Arc::clone(&closed_by_app),
    };

    let mut connecting = Box::pin(russh::client::connect(
        client_config,
        (address.0.clone(), address.1),
        client_handler,
    ));
    let mut deadline = Instant::now() + timeout;
    let connected = loop {
        tokio::select! {
            connected = &mut connecting => break connected,
            () = tokio::time::sleep_until(deadline) => {
                if awaiting_user.load(Ordering::SeqCst) {
                    deadline = Instant::now() + timeout;
                } else {
                    return Err(SshError::Timeout {
                        message: format!("connecting to {}:{}", address.0, address.1),
                    });
                }
            }
        }
    };

    let mut handle = match connected {
        Ok(handle) => handle,
        Err(_) if host_key_rejected.load(Ordering::SeqCst) => {
            return Err(SshError::HostKeyRejected);
        }
        Err(error) => return Err(error),
    };

    authenticate(&mut handle, &config, handler.as_ref()).await?;

    Ok(Arc::new(SshSession {
        handle: Mutex::new(handle),
        closed_by_app,
    }))
}

fn client_config(config: &ConnectConfig) -> Config {
    Config {
        nodelay: true,
        channel_buffer_size: CHANNEL_BUFFER_SIZE,
        keepalive_interval: (config.keepalive_interval_s > 0)
            .then(|| Duration::from_secs(u64::from(config.keepalive_interval_s))),
        keepalive_max: KEEPALIVE_MAX,
        ..Config::default()
    }
}

async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    config: &ConnectConfig,
    handler: &dyn SessionHandler,
) -> Result<(), SshError> {
    match &config.auth {
        AuthMethod::Password { password } => {
            let result = handle
                .authenticate_password(config.username.clone(), password.clone())
                .await?;

            accepted(result, "the server rejected the password")
        }
        AuthMethod::Key {
            private_key_openssh,
            passphrase,
        } => {
            let key = Arc::new(load_private_key(private_key_openssh, passphrase.as_deref())?.key);
            // RSA needs the strongest hash the server accepts; every other
            // algorithm ignores it.
            let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
            let result = handle
                .authenticate_publickey(
                    config.username.clone(),
                    PrivateKeyWithHashAlg::new(key, hash_alg),
                )
                .await?;

            accepted(result, "the server rejected the key")
        }
        AuthMethod::KeyboardInteractive => {
            let mut response = handle
                .authenticate_keyboard_interactive_start(config.username.clone(), None)
                .await?;

            loop {
                match response {
                    KeyboardInteractiveAuthResponse::Success => return Ok(()),
                    KeyboardInteractiveAuthResponse::Failure { .. } => {
                        return Err(SshError::AuthFailed {
                            message: "the server rejected the answers".to_string(),
                        });
                    }
                    KeyboardInteractiveAuthResponse::InfoRequest {
                        name,
                        instructions,
                        prompts,
                    } => {
                        let asked = prompts
                            .iter()
                            .map(|prompt| Prompt {
                                text: prompt.prompt.clone(),
                                echo: prompt.echo,
                            })
                            .collect::<Vec<_>>();
                        let expected = asked.len();

                        let answers = handler
                            .keyboard_interactive(name, instructions, asked)
                            .await
                            .ok_or(SshError::AuthCancelled)?;
                        if answers.len() != expected {
                            return Err(SshError::Protocol {
                                message: format!(
                                    "answered {expected} prompts with {} answers",
                                    answers.len()
                                ),
                            });
                        }

                        response = handle
                            .authenticate_keyboard_interactive_respond(answers)
                            .await?;
                    }
                }
            }
        }
    }
}

fn accepted(result: AuthResult, rejected: &str) -> Result<(), SshError> {
    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure {
            remaining_methods, ..
        } => Err(SshError::AuthFailed {
            message: format!("{rejected}; the server still accepts {remaining_methods:?}"),
        }),
    }
}

/// Waits for the server's answer to a channel request.
async fn await_success(channel: &mut Channel<russh::client::Msg>) -> Result<(), SshError> {
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => return Ok(()),
            Some(ChannelMsg::Failure) => {
                return Err(SshError::Protocol {
                    message: "the server refused the channel request".to_string(),
                });
            }
            Some(ChannelMsg::Close) | None => {
                return Err(SshError::ChannelClosed {
                    message: "the channel closed before the request was answered".to_string(),
                });
            }
            Some(_) => {}
        }
    }
}

struct ClientHandler {
    host: String,
    port: u16,
    app: Arc<dyn SessionHandler>,
    awaiting_user: Arc<AtomicBool>,
    host_key_rejected: Arc<AtomicBool>,
    closed_by_app: Arc<AtomicBool>,
}

impl russh::client::Handler for ClientHandler {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let (algorithm, public_key_base64, fingerprint) = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => (
                key.algorithm().to_string(),
                key.public_key_base64(),
                key.fingerprint(HashAlg::Sha256).to_string(),
            ),
            PublicKeyOrCertificate::Certificate(certificate) => {
                let key = ssh_key::PublicKey::from(certificate.public_key().clone());
                (
                    key.algorithm().to_string(),
                    key.public_key_base64(),
                    key.fingerprint(HashAlg::Sha256).to_string(),
                )
            }
        };

        self.awaiting_user.store(true, Ordering::SeqCst);
        let accepted = self
            .app
            .verify_host_key(
                self.host.clone(),
                self.port,
                algorithm,
                public_key_base64,
                fingerprint,
            )
            .await;
        self.awaiting_user.store(false, Ordering::SeqCst);

        if !accepted {
            self.host_key_rejected.store(true, Ordering::SeqCst);
        }

        Ok(accepted)
    }

    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        if !self.closed_by_app.load(Ordering::SeqCst) {
            let text = match reason {
                DisconnectReason::ReceivedDisconnect(info) => {
                    format!("{:?}: {}", info.reason_code, info.message)
                }
                DisconnectReason::Error(error) => error.to_string(),
            };
            self.app.on_disconnect(text);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(keepalive_interval_s: u32, connect_timeout_ms: u32) -> ConnectConfig {
        ConnectConfig {
            host: "example.test".to_string(),
            port: 2222,
            username: "test".to_string(),
            auth: AuthMethod::Password {
                password: "secret".to_string(),
            },
            connect_timeout_ms,
            keepalive_interval_s,
        }
    }

    #[test]
    fn keepalive_interval_zero_disables_keepalives() {
        assert_eq!(client_config(&config(0, 1_000)).keepalive_interval, None);
        assert_eq!(
            client_config(&config(30, 1_000)).keepalive_interval,
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn interactive_sessions_use_nodelay_and_no_inactivity_timeout() {
        let client = client_config(&config(30, 1_000));

        assert!(client.nodelay);
        assert!(client.inactivity_timeout.is_none());
        assert_eq!(client.keepalive_max, KEEPALIVE_MAX);
    }
}
