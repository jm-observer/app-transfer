//! Transfer program that forwards I/O between a launcher process and a target executable,
//! logging all transferred data to a file. Constants are injected at compile time via build.rs.

use anyhow::Result;
use std::path::PathBuf;
use std::process::Stdio;
use log::info;
use log::LevelFilter::Info;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};

// Compile‑time injected constants
const TARGET_EXE_NAME: &str = env!("TARGET_EXE_NAME");

// Write a log entry with timestamp and direction tag
async fn write_log(log_path: &PathBuf, direction: &str, data: &[u8]) -> Result<()> {
    use chrono::Local;
    let timestamp = Local::now().to_rfc3339();
    let entry = format!(
        "[{}][{}] {}\n",
        timestamp,
        direction,
        String::from_utf8_lossy(data)
    );
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .await?;
    file.write_all(entry.as_bytes()).await?;
    Ok(())
}

// Forward stdin to child stdin and log
async fn forward_stdin(child_stdin: ChildStdin) {
    let mut stdin = io::stdin();
    let mut child_stdin = child_stdin;
    let mut buf = [0u8; 8192];
    loop {
        let n = match stdin.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error reading from stdin: {}", e);
                break;
            }
        };
        if let Err(e) = child_stdin.write_all(&buf[..n]).await {
            eprintln!("Error writing to child stdin: {}", e);
            break;
        }
        info!("read from child stdin:\n\t{}", String::from_utf8_lossy(&buf[..n]));
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
                eprintln!("Error reading child stdout: {}", e);
                break;
            }
        };
        if let Err(e) = parent_stdout.write_all(&buf[..n]).await {
            eprintln!("Error writing to stdout: {}", e);
            break;
        }
        info!("read from child stdout:\n\t{}", String::from_utf8_lossy(&buf[..n]));
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
                eprintln!("Error reading child stderr: {}", e);
                break;
            }
        };
        if let Err(e) = parent_stderr.write_all(&buf[..n]).await {
            eprintln!("Error writing to stderr: {}", e);
            break;
        }
        info!("read from child stderr:\n\t{}", String::from_utf8_lossy(&buf[..n]));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = custom_utils::logger::logger_feature(&format!("app-transfer-{TARGET_EXE_NAME}"), "info", Info, false).build();
    // Resolve paths based on current working directory
    let cwd = std::env::current_dir()?;
    let target_path = cwd.join(TARGET_EXE_NAME);

    // Ensure the target executable exists
    if !target_path.is_file() {
        eprintln!("Target executable not found: {}", target_path.display());
        std::process::exit(1);
    }

    // Spawn the child process with piped stdio streams
    let mut child = Command::new(&target_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn target executable");

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
