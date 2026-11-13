//! Interactive shell channels: terminal bytes flow through a foreign sink and
//! never enter the JavaScript runtime.

use std::sync::{Arc, RwLock};

use russh::{Channel, ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::error::SshError;
use crate::runtime;

/// Receives the bytes and the close event of a shell channel.
#[uniffi::export(with_foreign)]
pub trait ByteSink: Send + Sync {
    /// A chunk of terminal output, in the order the server sent it.
    fn on_data(&self, bytes: Vec<u8>);

    /// The channel ended. `exit_status` is the shell's exit status when the
    /// server reported one; `error` explains an abnormal end.
    fn on_closed(&self, exit_status: Option<u32>, error: Option<String>);
}

enum Command {
    Write(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

#[derive(uniffi::Object)]
pub struct ShellChannel {
    commands: UnboundedSender<Command>,
    sink: RwLock<Arc<dyn ByteSink>>,
}

impl ShellChannel {
    /// Starts the reader and writer tasks for a channel whose pty and shell
    /// have already been requested.
    pub(crate) fn start(channel: Channel<russh::client::Msg>, sink: Arc<dyn ByteSink>) -> Self {
        let (reader, writer) = channel.split();
        let (commands, receiver) = unbounded_channel();

        runtime::spawn(write_loop(receiver, writer));
        runtime::spawn(read_loop(reader, Arc::clone(&sink)));

        Self {
            commands,
            sink: RwLock::new(sink),
        }
    }

    fn send(&self, command: Command) -> Result<(), SshError> {
        self.commands
            .send(command)
            .map_err(|_| SshError::ChannelClosed {
                message: "shell channel is closed".to_string(),
            })
    }
}

#[uniffi::export]
impl ShellChannel {
    /// Sends terminal input, preserving order with [`ShellChannel::resize`].
    pub fn write(&self, bytes: Vec<u8>) -> Result<(), SshError> {
        self.send(Command::Write(bytes))
    }

    pub fn resize(&self, cols: u32, rows: u32) -> Result<(), SshError> {
        self.send(Command::Resize { cols, rows })
    }

    pub fn close(&self) -> Result<(), SshError> {
        self.send(Command::Close)
    }

    /// Replaces the sink that receives output; bytes that arrive after the swap
    /// go to the new sink.
    pub fn set_sink(&self, sink: Arc<dyn ByteSink>) {
        *self
            .sink
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = sink;
    }
}

async fn write_loop(
    mut commands: UnboundedReceiver<Command>,
    writer: ChannelWriteHalf<russh::client::Msg>,
) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Write(bytes) => {
                if writer.data_bytes(bytes).await.is_err() {
                    break;
                }
            }
            Command::Resize { cols, rows } => {
                if writer.window_change(cols, rows, 0, 0).await.is_err() {
                    break;
                }
            }
            Command::Close => {
                let _ = writer.eof().await;
                let _ = writer.close().await;
                break;
            }
        }
    }
}

async fn read_loop(mut reader: ChannelReadHalf, sink: Arc<dyn ByteSink>) {
    let mut exit_status = None;

    loop {
        match reader.wait().await {
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                sink.on_data(data.to_vec());
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => exit_status = Some(status),
            Some(ChannelMsg::Eof) => {}
            Some(ChannelMsg::Close) | None => break,
            Some(_) => {}
        }
    }

    sink.on_closed(exit_status, None);
}
