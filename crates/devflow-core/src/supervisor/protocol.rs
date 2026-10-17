//! The supervisor's wire protocol: one request per connection, one JSON
//! object per line, newline-terminated.
//!
//! A line is at most [`MAX_LINE`] bytes including its newline, and every read
//! and write on a connection is bounded by [`IO_TIMEOUT`], so an oversized or
//! slow peer cannot wedge either side (T-50-11). There is no version field:
//! DevFlow makes no cross-version promise for this socket (D-20).
//!
//! | Request | Reply |
//! |---|---|
//! | `{"op":"ping"}` | `{"ok":true, ...PingReply fields}` |
//! | `{"op":"shutdown"}` | `{"ok":true,"accepted":true,"owner":…}` then `{"ok":true,"stopped":true,"exit_code":143,"advance_suppressed":true}`, or `{"ok":false,"error":"handed_off"}` |
//! | anything else | `{"ok":false,"error":"unknown_op"}` |
//!
//! The pids in a ping reply are for display only: nothing on the liveness
//! path reads them.

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// The longest line either side accepts, newline included.
pub const MAX_LINE: usize = 4096;

/// The read and write timeout applied to every protocol connection.
pub const IO_TIMEOUT: Duration = Duration::from_secs(2);

/// A request a client sends to a supervisor, serialized as
/// `{"op":"ping"}` or `{"op":"shutdown"}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Request {
    /// Ask the supervisor to describe itself.
    Ping,
    /// Ask the supervisor to stop its agent and exit.
    Shutdown,
}

/// The body of a successful ping reply. On the wire it also carries
/// `"ok":true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PingReply {
    /// The per-launch id this supervisor was started with.
    pub launch_id: String,
    /// The phase label, as `PhaseId` displays it.
    pub phase: String,
    /// The stage the supervised agent runs.
    pub stage: String,
    /// The launch shape: `capture` (stdin null, stdout to a file) or
    /// `stream` (the pipe-owning shape).
    pub launch: String,
    /// Who owns the run right now: `agent_running`, `advancing`, `gate_wait`
    /// or `handed_off`.
    pub owner: String,
    /// The agent's pid (display only).
    pub agent_pid: u32,
    /// The agent's process group id (display only).
    pub agent_pgid: u32,
    /// The agent's start time in clock ticks since boot (display only).
    pub agent_start_time: u64,
    /// The kernel boot id the supervisor read at start, when readable.
    pub boot_id: Option<String>,
    /// The supervisor's own pid (display only).
    pub supervisor_pid: u32,
}

/// The first shutdown reply: the request was accepted. On the wire,
/// `{"ok":true,"accepted":true,"owner":…}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ShutdownAccepted {
    /// The owner phase at the moment the request was accepted.
    pub owner: String,
}

/// The final shutdown reply, sent once the stop is persisted. On the wire,
/// `{"ok":true,"stopped":true,"exit_code":143,"advance_suppressed":true}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ShutdownStopped {
    /// The exit code the supervisor ends with.
    pub exit_code: i32,
    /// Whether the stage advance was suppressed by the stop.
    pub advance_suppressed: bool,
}

/// Every reply a server writes, one variant per wire shape.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "server side of the protocol; the supervisor runtime (50-07) is its first non-test caller"
    )
)]
pub(crate) enum Reply {
    /// `{"ok":true, ...}` answering a ping.
    Ping(PingReply),
    /// `{"ok":true,"accepted":true,"owner":…}`.
    Accepted(ShutdownAccepted),
    /// `{"ok":true,"stopped":true,...}`.
    Stopped(ShutdownStopped),
    /// `{"ok":false,"error":…}`, e.g. `unknown_op` or `handed_off`.
    Error(String),
}

impl Reply {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used only by write_reply; see Reply")
    )]
    fn to_json(&self) -> serde_json::Result<serde_json::Value> {
        let mut object = match self {
            Self::Ping(reply) => serde_json::to_value(reply)?,
            Self::Accepted(reply) => {
                let mut value = serde_json::to_value(reply)?;
                value["accepted"] = serde_json::Value::Bool(true);
                value
            }
            Self::Stopped(reply) => {
                let mut value = serde_json::to_value(reply)?;
                value["stopped"] = serde_json::Value::Bool(true);
                value
            }
            Self::Error(error) => {
                return Ok(serde_json::json!({ "ok": false, "error": error }));
            }
        };
        object["ok"] = serde_json::Value::Bool(true);
        Ok(object)
    }
}

/// Read one newline-terminated line of at most [`MAX_LINE`] bytes (newline
/// included). A longer line, a line cut off by end of stream, an empty
/// stream and invalid UTF-8 are all errors.
pub(crate) fn read_line(reader: &mut impl BufRead) -> io::Result<String> {
    let mut buf = Vec::new();
    reader.take(MAX_LINE as u64).read_until(b'\n', &mut buf)?;
    if buf.last() != Some(&b'\n') {
        let (kind, message) = if buf.len() >= MAX_LINE {
            (
                io::ErrorKind::InvalidData,
                format!("protocol line exceeds {MAX_LINE} bytes"),
            )
        } else if buf.is_empty() {
            (
                io::ErrorKind::UnexpectedEof,
                "connection closed before a line arrived".to_string(),
            )
        } else {
            (
                io::ErrorKind::UnexpectedEof,
                "connection closed in the middle of a line".to_string(),
            )
        };
        return Err(io::Error::new(kind, message));
    }
    buf.pop();
    String::from_utf8(buf).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn set_timeouts(stream: &UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))
}

/// Server side: read the connection's single request. A request that is not
/// valid JSON or names an unknown op is `InvalidData`; the caller answers it
/// with `Reply::Error("unknown_op")`.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "server side of the protocol; see Reply")
)]
pub(crate) fn read_request(stream: &UnixStream) -> io::Result<Request> {
    set_timeouts(stream)?;
    let line = read_line(&mut BufReader::new(stream))?;
    serde_json::from_str(&line).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Server side: write one reply line.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "server side of the protocol; see Reply")
)]
pub(crate) fn write_reply(mut stream: &UnixStream, reply: &Reply) -> io::Result<()> {
    set_timeouts(stream)?;
    let mut line = serde_json::to_string(&reply.to_json()?)?;
    line.push('\n');
    if line.len() > MAX_LINE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("reply line of {} bytes exceeds {MAX_LINE}", line.len()),
        ));
    }
    stream.write_all(line.as_bytes())?;
    stream.flush()
}

/// The effective uid of this process.
pub(crate) fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// Server side: whether the connected peer runs as this process's effective
/// uid. Linux reads the peer uid with `SO_PEERCRED`; macOS with `getpeereid`.
/// A failed read returns false, and so does every other target, so the uid
/// check is never skipped (T-50-10, T-50-87).
#[cfg(target_os = "linux")]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "server side of the protocol; see Reply")
)]
pub(crate) fn peer_is_same_user(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let expected_len = std::mem::size_of::<libc::ucred>();
    let mut len = expected_len as libc::socklen_t;
    // SAFETY: `cred` and `len` are live, correctly sized out-parameters for
    // SO_PEERCRED, and the fd is borrowed from a stream that outlives the call.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast(),
            &raw mut len,
        )
    };
    rc == 0 && len as usize == expected_len && cred.uid == euid()
}

/// Server side: whether the connected peer runs as this process's effective
/// uid, read with `getpeereid`. Compile-checked only: no macOS host has run it
/// (backlog 999.160).
#[cfg(target_os = "macos")]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "server side of the protocol; see Reply")
)]
pub(crate) fn peer_is_same_user(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    // SAFETY: `uid` and `gid` are live out-parameters, and the fd is borrowed
    // from a stream that outlives the call.
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &raw mut uid, &raw mut gid) };
    rc == 0 && uid == euid()
}

/// This target has no peer-uid call DevFlow uses, so every peer is rejected:
/// fail closed. A client then reads no reply (`NoReply`), never `Stale`, so
/// the backstop cannot arm on this target.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "server side of the protocol; see Reply")
)]
pub(crate) fn peer_is_same_user(_stream: &UnixStream) -> bool {
    false
}

/// Client side: send one ping on a connected stream and parse the reply.
/// Every failure (timeout, closed connection, oversized or malformed line,
/// `"ok":false`) is returned as a message; a connect alone is never proof of
/// life.
pub(crate) fn ping(stream: UnixStream) -> Result<PingReply, String> {
    set_timeouts(&stream).map_err(|error| format!("could not set socket timeouts: {error}"))?;
    let request = serde_json::to_string(&Request::Ping)
        .map_err(|error| format!("could not encode ping: {error}"))?;
    (&stream)
        .write_all(format!("{request}\n").as_bytes())
        .map_err(|error| format!("could not send ping: {error}"))?;
    let line = read_line(&mut BufReader::new(&stream))
        .map_err(|error| format!("no ping reply: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(&line)
        .map_err(|error| format!("ping reply is not JSON ({error}): {line}"))?;
    if value.get("ok") != Some(&serde_json::Value::Bool(true)) {
        return Err(format!("ping reply is not ok: {line}"));
    }
    serde_json::from_value(value)
        .map_err(|error| format!("ping reply is missing fields ({error}): {line}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(reply: &Reply) -> serde_json::Value {
        reply.to_json().expect("serialize a reply")
    }

    #[test]
    fn every_reply_has_its_wire_shape() {
        let accepted = wire(&Reply::Accepted(ShutdownAccepted {
            owner: "agent_running".to_string(),
        }));
        assert_eq!(
            accepted,
            serde_json::json!({"ok": true, "accepted": true, "owner": "agent_running"})
        );
        let stopped = wire(&Reply::Stopped(ShutdownStopped {
            exit_code: 143,
            advance_suppressed: true,
        }));
        assert_eq!(
            stopped,
            serde_json::json!({"ok": true, "stopped": true, "exit_code": 143, "advance_suppressed": true})
        );
        assert_eq!(
            wire(&Reply::Error("handed_off".to_string())),
            serde_json::json!({"ok": false, "error": "handed_off"})
        );
        let ping = wire(&Reply::Ping(PingReply {
            launch_id: "00000000000000ab".to_string(),
            phase: "50".to_string(),
            stage: "code".to_string(),
            launch: "capture".to_string(),
            owner: "gate_wait".to_string(),
            agent_pid: 10,
            agent_pgid: 10,
            agent_start_time: 99,
            boot_id: None,
            supervisor_pid: 9,
        }));
        assert_eq!(ping["ok"], serde_json::Value::Bool(true));
        assert_eq!(ping["launch"], "capture");
        assert_eq!(ping["supervisor_pid"], 9);
        assert_eq!(
            serde_json::to_string(&Request::Shutdown).expect("encode"),
            r#"{"op":"shutdown"}"#
        );
        assert!(
            serde_json::from_str::<Request>(r#"{"op":"reboot"}"#).is_err(),
            "an unknown op must not parse"
        );
    }

    #[test]
    fn a_line_longer_than_max_line_is_refused() {
        let long = format!("{}\n", "x".repeat(MAX_LINE));
        let error = read_line(&mut long.as_bytes()).expect_err("an oversized line must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        // Control: the longest allowed line, newline included, is read whole.
        let fits = format!("{}\n", "x".repeat(MAX_LINE - 1));
        assert_eq!(
            read_line(&mut fits.as_bytes()).expect("a line at the cap is read"),
            "x".repeat(MAX_LINE - 1)
        );
    }
}
