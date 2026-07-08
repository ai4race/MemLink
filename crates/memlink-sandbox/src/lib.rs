use async_trait::async_trait;
use memlink_protocol::StateRef;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tempfile::TempDir;
use thiserror::Error;
use tokio::process::Command;
use tokio::time;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRequest {
    pub code: String,
    pub language: SandboxLanguage,
    pub input_refs: Vec<StateRef>,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxLanguage {
    Python,
    Shell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub run_id: Uuid,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub summary: String,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub output_ref: Option<StateRef>,
}

#[async_trait]
pub trait Sandbox: Send + Sync {
    async fn execute(&self, request: SandboxRequest) -> Result<SandboxResult, SandboxError>;
}

#[derive(Debug, Clone)]
pub struct RestrictedProcessSandbox {
    pub python_bin: String,
    pub shell_bin: String,
    pub default_timeout_ms: u64,
    pub max_output_bytes: usize,
    pub max_memory_bytes: u64,
    pub max_file_bytes: u64,
    pub max_cpu_seconds: u64,
}

impl Default for RestrictedProcessSandbox {
    fn default() -> Self {
        Self {
            python_bin: "python3".to_owned(),
            shell_bin: "sh".to_owned(),
            default_timeout_ms: 5_000,
            max_output_bytes: 64 * 1024,
            max_memory_bytes: 256 * 1024 * 1024,
            max_file_bytes: 16 * 1024 * 1024,
            max_cpu_seconds: 5,
        }
    }
}

#[async_trait]
impl Sandbox for RestrictedProcessSandbox {
    async fn execute(&self, request: SandboxRequest) -> Result<SandboxResult, SandboxError> {
        let started = std::time::Instant::now();
        let tempdir = TempDir::new().map_err(SandboxError::Io)?;
        let script_path = write_script(&tempdir, request.language, &request.code).await?;
        let timeout_ms = if request.timeout_ms == 0 {
            self.default_timeout_ms
        } else {
            request.timeout_ms
        };
        let max_output_bytes = if request.max_output_bytes == 0 {
            self.max_output_bytes
        } else {
            request.max_output_bytes
        };
        let mut command = match request.language {
            SandboxLanguage::Python => {
                let mut command = Command::new(&self.python_bin);
                command.arg(&script_path);
                command
            }
            SandboxLanguage::Shell => {
                let mut command = Command::new(&self.shell_bin);
                command.arg(&script_path);
                command
            }
        };
        command.current_dir(tempdir.path()).kill_on_drop(true);
        command.env_clear();
        command.env("PATH", "/usr/bin:/bin:/usr/local/bin");
        command.env("HOME", tempdir.path());
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        apply_process_limits(
            &mut command,
            self.max_memory_bytes,
            self.max_file_bytes,
            self.max_cpu_seconds,
        );
        let child = command.spawn().map_err(SandboxError::Io)?;
        let child_id = child.id();
        let output = match time::timeout(
            Duration::from_millis(timeout_ms),
            child.wait_with_output(),
        )
        .await
        {
            Ok(output) => output.map_err(SandboxError::Io)?,
            Err(_) => {
                terminate_child_group(child_id).await;
                return Err(SandboxError::Timeout(timeout_ms));
            }
        };
        let stdout = bounded_utf8(output.stdout, max_output_bytes);
        let stderr = bounded_utf8(output.stderr, max_output_bytes);
        let success = output.status.success();
        Ok(SandboxResult {
            run_id: Uuid::new_v4(),
            success,
            exit_code: output.status.code(),
            summary: summarize(success, &stdout, &stderr),
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis() as u64,
            output_ref: None,
        })
    }
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sandbox timed out after {0} ms")]
    Timeout(u64),
}

async fn write_script(
    tempdir: &TempDir,
    language: SandboxLanguage,
    code: &str,
) -> Result<PathBuf, SandboxError> {
    let file_name = match language {
        SandboxLanguage::Python => "main.py",
        SandboxLanguage::Shell => "main.sh",
    };
    let path = tempdir.path().join(file_name);
    tokio::fs::write(&path, code).await?;
    Ok(path)
}

fn bounded_utf8(bytes: Vec<u8>, max_output_bytes: usize) -> String {
    let truncated = bytes.len() > max_output_bytes;
    let mut bytes = bytes;
    bytes.truncate(max_output_bytes);
    let mut output = String::from_utf8_lossy(&bytes).to_string();
    if truncated {
        output.push_str("\n[truncated]");
    }
    output
}

fn summarize(success: bool, stdout: &str, stderr: &str) -> String {
    let source = if success { stdout } else { stderr };
    let preview = source.lines().next().unwrap_or_default();
    format!("success={success}; preview={preview}")
}

#[cfg(unix)]
fn apply_process_limits(
    command: &mut Command,
    max_memory_bytes: u64,
    max_file_bytes: u64,
    max_cpu_seconds: u64,
) {
    unsafe {
        command.pre_exec(move || {
            set_limit(libc::RLIMIT_FSIZE, max_file_bytes)?;
            set_limit(libc::RLIMIT_CPU, max_cpu_seconds.max(1))?;
            set_limit(libc::RLIMIT_NOFILE, 64)?;
            set_memory_limit(max_memory_bytes)?;
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn apply_process_limits(
    _command: &mut Command,
    _max_memory_bytes: u64,
    _max_file_bytes: u64,
    _max_cpu_seconds: u64,
) {
}

#[cfg(unix)]
fn set_limit(resource: libc::c_int, value: u64) -> std::io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    if unsafe { libc::setrlimit(resource as _, &limit) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
fn set_memory_limit(max_memory_bytes: u64) -> std::io::Result<()> {
    if max_memory_bytes > 0 {
        set_limit(libc::RLIMIT_AS, max_memory_bytes)?;
    }
    Ok(())
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn set_memory_limit(_max_memory_bytes: u64) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn terminate_child_group(child_id: Option<u32>) {
    if let Some(pid) = child_id {
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

#[cfg(not(unix))]
async fn terminate_child_group(_child_id: Option<u32>) {}
