// Example program: reads stdin, appends a suffix, writes to stdout, and prints a number to stderr every 5 seconds.

use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt};
use tokio::task;
use tokio::time::{self, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spawn a background task that writes a number to stderr every 5 seconds.
    let _stderr_task = task::spawn(async {
        let mut interval = time::interval(Duration::from_secs(5));
        let mut count: u64 = 0;
        loop {
            interval.tick().await;
            eprintln!("{}", count);
            count += 1;
        }
    });

    // Read lines from stdin, append a suffix, and write to stdout.
    let mut stdin = io::BufReader::new(io::stdin());
    let mut line = String::new();
    let suffix = "_suffix";
    let mut stdout = io::stdout();

    loop {
        line.clear();
        let bytes = stdin.read_line(&mut line).await?;
        if bytes == 0 {
            break; // EOF
        }
        // Remove trailing newline, append suffix, and write newline.
        let output = format!("{}{}\n", line.trim_end(), suffix);
        stdout.write_all(output.as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok(())
}
