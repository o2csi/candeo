//! `candeo --mcp`, the bridge an assistant starts (`docs/design/mcp-server.md`
//! §2.3).
//!
//! It connects to the pipe, copies its standard input to the pipe and the pipe
//! to its standard output until either side closes, and exits. It parses
//! nothing: the protocol, both revisions and every check live in Candeo, so the
//! bridge never changes with the specification.
//!
//! It starts nothing either: no Tauri, no single-instance plugin, no HID, no
//! settings, no log. It never starts Candeo, whose start resumes the applied
//! effects on the hardware: an assistant must not cause that.
//!
//! The release build is a Windows GUI program, and that is enough: started with
//! redirected handles, as every assistant starts a server, it reads and writes
//! them as a console program would, and no window flashes. Tried with the
//! release executable started by Node with piped standard streams, as Claude
//! Desktop, Claude Code, VS Code and Cursor start one (2026-11-06).

use std::future::Future;
use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::pipe::{self, Unreachable};

/// How long the bridge waits for the pipe to appear: a Candeo still starting at
/// login opens it within seconds.
pub(crate) const PATIENCE: Duration = Duration::from_secs(5);

/// How often it looks again meanwhile, and while every instance is busy.
const EVERY: Duration = Duration::from_millis(100);

/// What the bridge says when there is no pipe. Assistants show standard error
/// in their server log: English, one line, and what to do.
pub(crate) const NOT_RUNNING: &str = "Candeo is not running, or MCP is off in its Settings.";

const NOT_OURS: &str =
    "The pipe Candeo listens on belongs to another user on this computer: not connected.";

/// Why the bridge gave up.
#[derive(Debug)]
pub(crate) enum GaveUp {
    NotRunning,
    NotOurs,
    Failed(io::Error),
}

impl std::fmt::Display for GaveUp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning => f.write_str(NOT_RUNNING),
            Self::NotOurs => f.write_str(NOT_OURS),
            Self::Failed(e) => write!(f, "Cannot reach Candeo: {e}"),
        }
    }
}

/// Connects through `connect`, trying again while there is no pipe, for
/// `patience`, and while every instance is in use, for as long as it takes: a
/// fifth assistant waits for one of four to close (§2.5).
pub(crate) async fn reach<S, F, Fut>(
    mut connect: F,
    patience: Duration,
    every: Duration,
) -> Result<S, GaveUp>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<S, Unreachable>>,
{
    let deadline = tokio::time::Instant::now() + patience;
    loop {
        match connect().await {
            Ok(stream) => return Ok(stream),
            Err(Unreachable::Busy) => {}
            Err(Unreachable::Absent) if tokio::time::Instant::now() < deadline => {}
            Err(Unreachable::Absent) => return Err(GaveUp::NotRunning),
            Err(Unreachable::NotOurs) => return Err(GaveUp::NotOurs),
            Err(Unreachable::Other(e)) => return Err(GaveUp::Failed(e)),
        }
        tokio::time::sleep(every).await;
    }
}

/// Copies `input` to `pipe` and `pipe` to `output`, until either side closes.
pub(crate) async fn relay<I, O, P>(mut input: I, mut output: O, pipe: P) -> io::Result<()>
where
    I: AsyncRead + Unpin,
    O: AsyncWrite + Unpin,
    P: AsyncRead + AsyncWrite + Unpin,
{
    let (mut from_pipe, mut to_pipe) = tokio::io::split(pipe);
    tokio::select! {
        up = pump(&mut input, &mut to_pipe) => up,
        down = pump(&mut from_pipe, &mut output) => down,
    }
}

/// Copies until the end, flushing each chunk: a response must reach the
/// assistant when it is written, not when a buffer fills.
async fn pump<R, W>(from: &mut R, to: &mut W) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = from.read(&mut buffer).await?;
        if read == 0 {
            return Ok(());
        }
        to.write_all(&buffer[..read]).await?;
        to.flush().await?;
    }
}

/// The bridge, from `main()`: its exit code. Writes nothing but one line to
/// standard error when it cannot connect.
pub fn run() -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Cannot start the MCP bridge: {e}");
            return 1;
        }
    };
    let code = runtime.block_on(async {
        let pipe = match reach(pipe::connect, PATIENCE, EVERY).await {
            Ok(pipe) => pipe,
            Err(gave_up) => {
                eprintln!("{gave_up}");
                return 1;
            }
        };
        match relay(tokio::io::stdin(), tokio::io::stdout(), pipe).await {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("The connection to Candeo failed: {e}");
                1
            }
        }
    });
    // Standard input is read on a blocking thread that may still wait for a
    // line: dropping the runtime would wait for it.
    runtime.shutdown_background();
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{duplex, AsyncBufReadExt, BufReader};

    /// What the assistant writes reaches Candeo, and what Candeo answers reaches
    /// the assistant, byte for byte.
    #[tokio::test]
    async fn the_bridge_copies_both_ways() {
        let (mut assistant_in, bridge_in) = duplex(1024);
        let (bridge_out, assistant_out) = duplex(1024);
        let (bridge_pipe, candeo) = duplex(1024);

        // Candeo, echoing each line back with a mark.
        let echo = tokio::spawn(async move {
            let (read, mut write) = tokio::io::split(candeo);
            let mut lines = BufReader::new(read).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                write
                    .write_all(format!("candeo:{line}\n").as_bytes())
                    .await
                    .unwrap();
            }
        });
        let bridge = tokio::spawn(relay(bridge_in, bridge_out, bridge_pipe));

        let message = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        assistant_in
            .write_all(format!("{message}\n").as_bytes())
            .await
            .unwrap();
        let mut answers = BufReader::new(assistant_out).lines();
        assert_eq!(
            answers.next_line().await.unwrap().unwrap(),
            format!("candeo:{message}")
        );

        // The assistant closes its side: the bridge stops.
        drop(assistant_in);
        bridge.await.unwrap().unwrap();
        echo.abort();
    }

    /// Candeo quitting, or MCP turned off, closes the pipe: the bridge stops.
    #[tokio::test]
    async fn the_bridge_stops_when_the_pipe_closes() {
        let (_assistant_in, bridge_in) = duplex(64);
        let (bridge_out, _assistant_out) = duplex(64);
        let (bridge_pipe, candeo) = duplex(64);
        drop(candeo);
        relay(bridge_in, bridge_out, bridge_pipe).await.unwrap();
    }

    /// With MCP off there is no pipe: the bridge tries for its patience, then
    /// says Candeo is not running.
    #[tokio::test]
    async fn no_pipe_gives_up_after_its_patience() {
        let tries = AtomicUsize::new(0);
        let started = tokio::time::Instant::now();
        let result: Result<(), GaveUp> = reach(
            || {
                tries.fetch_add(1, Ordering::Relaxed);
                async { Err(Unreachable::Absent) }
            },
            Duration::from_millis(200),
            Duration::from_millis(20),
        )
        .await;
        assert!(matches!(result, Err(GaveUp::NotRunning)));
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(tries.load(Ordering::Relaxed) > 2);
        assert_eq!(GaveUp::NotRunning.to_string(), NOT_RUNNING);
    }

    /// A Candeo still starting opens the pipe within the patience: connected.
    /// A busy pipe is waited for, whatever the patience.
    #[tokio::test]
    async fn a_pipe_that_appears_or_frees_is_reached() {
        let tries = AtomicUsize::new(0);
        let result = reach(
            || {
                let n = tries.fetch_add(1, Ordering::Relaxed);
                async move {
                    match n {
                        0 => Err(Unreachable::Absent),
                        1..=5 => Err(Unreachable::Busy),
                        _ => Ok("pipe"),
                    }
                }
            },
            Duration::from_millis(1),
            Duration::from_millis(5),
        )
        .await;
        assert_eq!(result.unwrap(), "pipe");
    }

    /// A pipe served by another account is refused, without waiting.
    #[tokio::test]
    async fn a_pipe_of_another_user_is_refused() {
        let result: Result<(), GaveUp> = reach(
            || async { Err(Unreachable::NotOurs) },
            Duration::from_secs(5),
            Duration::from_millis(1),
        )
        .await;
        assert!(matches!(result, Err(GaveUp::NotOurs)));
    }
}
