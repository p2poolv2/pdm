// SPDX-FileCopyrightText: 2024 PDM Authors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Result;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self::Stopped
    }
}

impl ProcessState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::Starting => "Starting",
            Self::Running => "Running",
            Self::Stopping => "Stopping",
            Self::Failed => "Failed",
        }
    }

    #[must_use]
    pub const fn can_start(self) -> bool {
        matches!(self, Self::Stopped | Self::Failed)
    }

    #[must_use]
    pub const fn can_stop(self) -> bool {
        matches!(self, Self::Running | Self::Starting)
    }

    #[must_use]
    pub const fn can_restart(self) -> bool {
        matches!(self, Self::Running | Self::Stopped | Self::Failed)
    }
}

#[derive(Debug)]
pub struct ProcessManager {
    state: ProcessState,
    error: Option<String>,
    child: Option<Child>,
    stop_requested_at: Option<Instant>,
    stop_timeout: Duration,
}

impl ProcessManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: ProcessState::Stopped,
            error: None,
            child: None,
            stop_requested_at: None,
            stop_timeout: DEFAULT_STOP_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_stop_timeout(stop_timeout: Duration) -> Self {
        Self {
            state: ProcessState::Stopped,
            error: None,
            child: None,
            stop_requested_at: None,
            stop_timeout,
        }
    }

    #[must_use]
    pub fn state(&self) -> ProcessState {
        self.state
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.child.is_some()
    }

    pub fn mark_failed(&mut self, error: impl Into<String>) {
        if self.child.is_some() {
            self.shutdown_blocking();
        }
        self.child = None;
        self.stop_requested_at = None;
        self.state = ProcessState::Failed;
        self.error = Some(error.into());
    }

    pub fn start(&mut self, command: &mut Command) -> Result<()> {
        self.poll();

        if self.child.is_some() || self.state.can_stop() {
            return Err(anyhow::anyhow!(
                "Process is already {}",
                self.state.as_str().to_lowercase()
            ));
        }

        self.error = None;
        self.state = ProcessState::Starting;
        self.stop_requested_at = None;

        match command.spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.state = ProcessState::Running;
                Ok(())
            }
            Err(error) => {
                self.child = None;
                self.state = ProcessState::Failed;
                self.error = Some(format!("Failed to start process: {error}"));
                Err(anyhow::anyhow!("Failed to start process: {error}"))
            }
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        self.poll();

        let Some(child) = self.child.as_mut() else {
            self.state = ProcessState::Stopped;
            self.error = None;
            self.stop_requested_at = None;
            return Ok(());
        };

        if self.state == ProcessState::Stopping {
            return Ok(());
        }

        self.state = ProcessState::Stopping;
        self.error = None;
        self.stop_requested_at = Some(Instant::now());

        match Self::request_child_stop(child) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.error = Some(format!(
                    "Failed to request graceful stop: {error}. Will force kill after timeout."
                ));
                Err(anyhow::anyhow!("Failed to stop process: {error}"))
            }
        }
    }

    pub fn restart(&mut self, command: &mut Command) -> Result<()> {
        if self.state == ProcessState::Stopping {
            return Err(anyhow::anyhow!("Process is still stopping"));
        }

        if self.child.is_some() {
            self.stop()?;
            self.shutdown_blocking();
        }

        self.start(command)
    }

    pub fn shutdown_blocking(&mut self) {
        if self.child.is_none() {
            return;
        }

        let _ = self.stop();
        while self.child.is_some() {
            self.poll();
            if self.child.is_some() {
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    pub fn poll(&mut self) {
        let Some(child) = self.child.as_mut() else {
            if self.state == ProcessState::Stopping {
                self.state = ProcessState::Stopped;
            }
            return;
        };

        let was_stopping = self.state == ProcessState::Stopping;

        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = None;
                self.stop_requested_at = None;

                if was_stopping {
                    self.state = ProcessState::Stopped;
                    if !status.success() {
                        self.error = Some(format!("Process stopped with status {status}"));
                    }
                } else {
                    self.state = ProcessState::Failed;
                    self.error = Some(format!("Process exited unexpectedly with status {status}"));
                }
            }
            Ok(None) => self.kill_after_timeout(),
            Err(error) => {
                self.child = None;
                self.stop_requested_at = None;
                self.state = ProcessState::Failed;
                self.error = Some(format!("Failed to check process state: {error}"));
            }
        }
    }

    fn kill_after_timeout(&mut self) {
        if self.state != ProcessState::Stopping {
            return;
        }

        let Some(stop_requested_at) = self.stop_requested_at else {
            return;
        };

        if stop_requested_at.elapsed() < self.stop_timeout {
            return;
        }

        let Some(mut child) = self.child.take() else {
            self.state = ProcessState::Stopped;
            self.stop_requested_at = None;
            return;
        };

        match Self::force_kill_child(&mut child).and_then(|()| child.wait().map(|_| ())) {
            Ok(()) => {
                self.state = ProcessState::Stopped;
                self.error = Some(format!(
                    "Process did not stop within {}s and was killed.",
                    self.stop_timeout.as_secs()
                ));
            }
            Err(error) => {
                self.state = ProcessState::Failed;
                self.error = Some(format!("Failed to kill process: {error}"));
            }
        }
        self.stop_requested_at = None;
    }

    fn request_child_stop(child: &mut Child) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            let pid = child.id() as libc::pid_t;
            let result = unsafe { libc::kill(pid, libc::SIGTERM) };
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }

        #[cfg(not(unix))]
        {
            child.kill()?;
            Ok(())
        }
    }

    fn force_kill_child(child: &mut Child) -> std::io::Result<()> {
        child.kill()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        self.shutdown_blocking();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::thread;

    fn short_lived_failure_command() -> Command {
        #[cfg(unix)]
        {
            let mut command = Command::new("sh");
            command.arg("-c").arg("exit 7");
            command
        }

        #[cfg(windows)]
        {
            let mut command = Command::new("cmd");
            command.args(["/C", "exit 7"]);
            command
        }
    }

    fn long_running_command() -> Command {
        #[cfg(unix)]
        {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        }

        #[cfg(windows)]
        {
            let mut command = Command::new("powershell");
            command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
            command
        }
    }

    #[test]
    fn start_marks_failed_for_missing_executable() {
        let mut manager = ProcessManager::new();
        let mut command = std::process::Command::new("/definitely/not/a/real/executable");

        let err = manager.start(&mut command).unwrap_err();

        assert_eq!(manager.state(), ProcessState::Failed);
        assert!(manager.error().is_some());
        assert!(err.to_string().contains("Failed to start"));
    }

    #[test]
    fn stop_on_idle_process_is_a_noop() {
        let mut manager = ProcessManager::new();

        let result = manager.stop();

        assert!(result.is_ok());
        assert_eq!(manager.state(), ProcessState::Stopped);
    }

    #[test]
    fn poll_marks_unexpected_exit_failed() {
        let mut manager = ProcessManager::new();
        let mut command = short_lived_failure_command();
        command.stdout(Stdio::null()).stderr(Stdio::null());

        manager.start(&mut command).unwrap();
        thread::sleep(Duration::from_millis(50));
        manager.poll();

        assert_eq!(manager.state(), ProcessState::Failed);
        assert!(
            manager
                .error()
                .is_some_and(|error| error.contains("unexpectedly"))
        );
    }

    #[test]
    fn stop_moves_running_process_to_stopping() {
        let mut manager = ProcessManager::with_stop_timeout(Duration::from_secs(1));
        let mut command = long_running_command();
        command.stdout(Stdio::null()).stderr(Stdio::null());

        manager.start(&mut command).unwrap();
        manager.stop().unwrap();

        assert_eq!(manager.state(), ProcessState::Stopping);
        manager.shutdown_blocking();
        assert_eq!(manager.state(), ProcessState::Stopped);
    }
}
