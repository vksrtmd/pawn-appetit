//! UCI protocol communication utilities for chess engines.
//!
//! This module provides the `UciCommunicator` struct for spawning and communicating with UCI engines
//! using async I/O. Handles stdin/stdout/stderr and line-based protocol.

use log::{error, info};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use vampirc_uci::{parse_one, UciMessage};

use crate::error::Error;

use super::types::EngineConfig;

/// Async communicator for a running UCI engine process.
pub struct UciCommunicator {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout_lines: Lines<BufReader<ChildStdout>>,
}

impl UciCommunicator {
    /// Spawn a new UCI engine process and set up async I/O.
    ///
    /// # Arguments
    /// * `path` - Path to the engine binary.
    ///
    /// # Returns
    /// `UciCommunicator` with stdin and stdout line reader.
    ///
    /// # Errors
    /// Returns `Error` if process or I/O setup fails.
    pub async fn spawn(path: PathBuf) -> Result<Self, Error> {
        let mut command = Command::new(&path);
        command.current_dir(path.parent().unwrap());
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(target_os = "windows")]
        command.creation_flags(super::process::CREATE_NO_WINDOW);

        let mut child = command.spawn()?;
        info!("Starting engine process: {:?}", path);
        let stdin = child.stdin.take().ok_or(Error::NoStdin)?;
        let stdout = child.stdout.take().ok_or(Error::NoStdout)?;
        let stdout_lines = BufReader::new(stdout).lines();

        // Drain stderr to avoid deadlocks when buffer fills up
        let stderr = child.stderr.take();
        tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    error!("[engine-stderr] {}", line);
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            stdout_lines,
        })
    }

    /// Write a line to the engine's stdin (async).
    ///
    /// # Arguments
    /// * `line` - The command string to send (should end with `\n`).
    ///
    /// # Errors
    /// Returns `Error` if writing fails.
    pub async fn write_line(&mut self, line: &str) -> Result<(), Error> {
        info!("[engine-stdin] {}", line.trim_end());
        self.stdin.write_all(line.as_bytes()).await?;
        Ok(())
    }

    /// Send `command` and read engine lines, handing each to `on_line`, until one equals `until`.
    ///
    /// # Errors
    /// `Error::EngineTimeout` when `until` does not arrive within `timeout`;
    /// `Error::EngineInitFailed` when the engine closes its output first.
    pub async fn send_and_wait(
        &mut self,
        command: &str,
        until: &str,
        timeout: Duration,
        mut on_line: impl FnMut(&str),
    ) -> Result<(), Error> {
        self.write_line(command).await?;
        let found = tokio::time::timeout(timeout, async {
            while let Some(line) = self.stdout_lines.next_line().await? {
                on_line(&line);
                if line == until {
                    return Ok::<_, Error>(true);
                }
            }
            Ok(false)
        })
        .await
        .map_err(|_| {
            Error::EngineTimeout(format!("Engine did not answer {:?}", command.trim_end()))
        })??;
        if found {
            Ok(())
        } else {
            Err(Error::EngineInitFailed(format!(
                "Engine closed before sending {until}"
            )))
        }
    }
}

/// Time an engine gets to send `uciok`.
pub const UCI_TIMEOUT: Duration = Duration::from_secs(10);

/// Spawn the engine at `path` and read its name and options, within `timeout`. The process
/// is killed when this returns, whatever the outcome.
pub async fn read_engine_config(path: PathBuf, timeout: Duration) -> Result<EngineConfig, Error> {
    let mut comm = UciCommunicator::spawn(path).await?;
    let mut config = EngineConfig::default();
    comm.send_and_wait("uci\n", "uciok", timeout, |line| match parse_one(line) {
        UciMessage::Id {
            name: Some(name), ..
        } => config.name = name,
        UciMessage::Option(opt) => config.options.push(opt),
        _ => {}
    })
    .await?;
    Ok(config)
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::test_support::script;
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn config_of_an_engine_that_exits_without_uciok_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "quitter", "read l\necho 'id name Quitter'\n");
        let started = Instant::now();

        let result = read_engine_config(path, Duration::from_secs(5)).await;

        assert!(matches!(result, Err(Error::EngineInitFailed(_))));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn config_of_a_silent_engine_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(dir.path(), "silent", "sleep 30\n");

        let result = read_engine_config(path, Duration::from_millis(300)).await;

        assert!(matches!(result, Err(Error::EngineTimeout(_))));
    }

    #[tokio::test]
    async fn config_collects_name_and_options() {
        let dir = tempfile::tempdir().unwrap();
        let path = script(
            dir.path(),
            "engine",
            "read l\necho 'id name Fake 1'\necho 'option name Hash type spin default 16 min 1 max 64'\necho uciok\n",
        );

        let config = read_engine_config(path, Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(config.name, "Fake 1");
        assert_eq!(config.options.len(), 1);
    }
}
