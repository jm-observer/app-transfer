//! Transfer program that forwards I/O between a launcher process and a target executable,
//! logging all transferred data to a file. Constants are injected at compile time via build.rs.

use anyhow::{anyhow, Context, Result};
use log::LevelFilter::Info;
use log::{error, info};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};

const APP_TARGET_ENV: &str = "APP_TARGET";
const LOCAL_TARGET_NAME: &str = if cfg!(windows) {
    "origin.exe"
} else {
    "origin"
};
const WINDOWS_TARGET_NAME: &str = "origin.exe";
const NON_WINDOWS_TARGET_NAME: &str = "origin";

enum TargetExecutable {
    Path(PathBuf),
    Program(String),
}

impl TargetExecutable {
    fn command(&self) -> Command {
        match self {
            Self::Path(path) => Command::new(path),
            Self::Program(program) => Command::new(program),
        }
    }

    fn display(&self) -> String {
        match self {
            Self::Path(path) => path.display().to_string(),
            Self::Program(program) => program.clone(),
        }
    }
}

fn resolve_target(exe_dir: &Path) -> Result<TargetExecutable> {
    let local_target = exe_dir.join(LOCAL_TARGET_NAME);
    if local_target.is_file() {
        return Ok(TargetExecutable::Path(local_target));
    }

    let env_target = std::env::var(APP_TARGET_ENV).with_context(|| {
        format!("target executable not found in current directory and {APP_TARGET_ENV} is not set")
    })?;
    let trimmed = env_target.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("{APP_TARGET_ENV} is set but empty"));
    }

    Ok(parse_env_target(trimmed))
}

fn parse_env_target(value: &str) -> TargetExecutable {
    let path = Path::new(value);
    if path.is_absolute() || path.parent().is_some() {
        return TargetExecutable::Path(path.to_path_buf());
    }

    TargetExecutable::Program(value.to_string())
}

fn collect_args() -> Vec<OsString> {
    std::env::args_os().collect()
}

fn is_help_requested(args: &[OsString]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| arg == "--help" || arg == "-h")
}

fn help_text(exe_name: &str) -> String {
    format!(
        "{exe_name}\n\n\
Usage:\n  {exe_name} [args...]\n\n\
Behavior:\n  1. First try the local target in the same directory as this executable.\n\
     Windows: {windows_target}\n\
     Non-Windows: {linux_target}\n\
  2. If the local target does not exist, read {env_name}.\n\
  3. {env_name} may be an executable path or a program name.\n\
  4. Remaining arguments are forwarded to the target process.\n\n\
Environment:\n  {env_name}   Executable path or program name used as fallback target.\n\n\
Options:\n  -h, --help   Show this help message and exit.\n",
        windows_target = WINDOWS_TARGET_NAME,
        linux_target = NON_WINDOWS_TARGET_NAME,
        env_name = APP_TARGET_ENV,
    )
}

fn print_help(exe_name: &str) -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    use std::io::Write;
    stdout.write_all(help_text(exe_name).as_bytes())?;
    stdout.flush()?;
    Ok(())
}

// Forward stdin to child stdin and log (using info!)
async fn forward_stdin(child_stdin: ChildStdin) {
    let mut stdin = io::stdin();
    let mut child_stdin = child_stdin;
    let mut buf = [0u8; 8192];
    loop {
        let n = match stdin.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                error!("Error reading from stdin: {}", e);
                break;
            }
        };
        if let Err(e) = child_stdin.write_all(&buf[..n]).await {
            error!("Error writing to child stdin: {}", e);
            break;
        }
        info!(
            "read from parent stdin:\n\t{}",
            String::from_utf8_lossy(&buf[..n])
        );
    }
}

// Forward child stdout to parent stdout and log
async fn forward_stdout(child_stdout: ChildStdout) {
    let mut child_stdout = child_stdout;
    let mut parent_stdout = io::stdout();
    let mut buf = [0u8; 8192];
    loop {
        let n = match child_stdout.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                error!("Error reading child stdout: {}", e);
                break;
            }
        };
        if let Err(e) = parent_stdout.write_all(&buf[..n]).await {
            error!("Error writing to stdout: {}", e);
            break;
        }
        info!(
            "read from child stdout:\n\t{}",
            String::from_utf8_lossy(&buf[..n])
        );
    }
}

// Forward child stderr to parent stderr and log
async fn forward_stderr(child_stderr: ChildStderr) {
    let mut child_stderr = child_stderr;
    let mut parent_stderr = io::stderr();
    let mut buf = [0u8; 8192];
    loop {
        let n = match child_stderr.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                error!("Error reading child stderr: {}", e);
                break;
            }
        };
        if let Err(e) = parent_stderr.write_all(&buf[..n]).await {
            error!("Error writing to stderr: {}", e);
            break;
        }
        info!(
            "read from child stderr:\n\t{}",
            String::from_utf8_lossy(&buf[..n])
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let exe_path = std::env::current_exe()?;
    let exe_name = exe_path.file_name().unwrap().to_str().unwrap().to_string();
    let args = collect_args();
    if is_help_requested(&args) {
        print_help(&exe_name)?;
        return Ok(());
    }
    // 获取本程序完整路径
    let _ = custom_utils::logger::logger_feature(
        &format!("app-transfer_{exe_name}"),
        "info",
        Info,
        false,
    )
    .build();
    let exe_dir = exe_path.parent().context("failed to get exe directory")?;
    let target = resolve_target(exe_dir)?;

    info!("{:?}", args);
    info!("{:?}", std::env::vars_os());
    info!("resolved target executable: {}", target.display());

    // Spawn the child process with piped stdio streams
    let mut child = target
        .command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args.iter().skip(1))
        .envs(std::env::vars_os())
        .spawn()
        .with_context(|| format!("failed to spawn target executable: {}", target.display()))?;

    // Capture child stdio handles
    let child_stdin = child.stdin.take().expect("failed to capture child stdin");
    let child_stdout = child.stdout.take().expect("failed to capture child stdout");
    let child_stderr = child.stderr.take().expect("failed to capture child stderr");

    // Launch forwarding tasks
    let stdin_fwd = tokio::spawn(forward_stdin(child_stdin));
    let stdout_fwd = tokio::spawn(forward_stdout(child_stdout));
    let stderr_fwd = tokio::spawn(forward_stderr(child_stderr));

    // Wait for the child process to exit
    let status = child.wait().await?;

    // Log exit information
    let exit_code = status.code().unwrap_or(-1);
    let timestamp = chrono::Local::now().to_rfc3339();
    info!("[{}] child exited with code: {}\n", timestamp, exit_code);

    // Ensure all forwarding tasks have completed
    let _ = tokio::join!(stdin_fwd, stdout_fwd, stderr_fwd);

    // Propagate the child exit code to the launcher process
    std::process::exit(exit_code);
}
