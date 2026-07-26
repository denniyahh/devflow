//! Spike 2: prove a socket-addressable Rust supervisor can REPLACE the
//! production `sh -c` monitor — i.e. covers every responsibility it has.
//!
//! Production responsibilities under test (R-A..R-J), plus socket-specific
//! semantics the replacement introduces (R-K..R-M).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct Cfg {
    sock: String,
    workdir: String,
    stdout: String,
    stderr: String,
    pidfile: String,
    exitfile: String,
    advance_marker: Option<String>,
    envs: Vec<(String, String)>,
    cmd: Vec<String>,
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("monitor") => monitor(parse(&a[2..])),
        Some("status") => status(&a[2]),
        Some("stop") => stop(&a[2]),
        Some("sweep") => sweep(&a[2]),
        _ => {
            eprintln!("usage: monitor <flags> -- cmd... | status <sock> | stop <sock> | sweep <dir>");
            std::process::exit(2);
        }
    }
}

fn parse(a: &[String]) -> Cfg {
    let mut c = Cfg {
        sock: String::new(), workdir: ".".into(), stdout: String::new(),
        stderr: String::new(), pidfile: String::new(), exitfile: String::new(),
        advance_marker: None, envs: vec![], cmd: vec![],
    };
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--sock" => { c.sock = a[i + 1].clone(); i += 2 }
            "--workdir" => { c.workdir = a[i + 1].clone(); i += 2 }
            "--stdout" => { c.stdout = a[i + 1].clone(); i += 2 }
            "--stderr" => { c.stderr = a[i + 1].clone(); i += 2 }
            "--pidfile" => { c.pidfile = a[i + 1].clone(); i += 2 }
            "--exitfile" => { c.exitfile = a[i + 1].clone(); i += 2 }
            "--advance" => { c.advance_marker = Some(a[i + 1].clone()); i += 2 }
            "--env" => {
                let (k, v) = a[i + 1].split_once('=').expect("k=v");
                c.envs.push((k.into(), v.into())); i += 2
            }
            "--" => { c.cmd = a[i + 1..].to_vec(); break }
            _ => i += 1,
        }
    }
    c
}

fn monitor(c: Cfg) {
    // Takeover safety: never steal a socket a live monitor owns.
    if Path::new(&c.sock).exists() {
        if UnixStream::connect(&c.sock).is_ok() {
            eprintln!("refusing: live monitor owns {}", c.sock);
            std::process::exit(3);
        }
        let _ = std::fs::remove_file(&c.sock);
    }
    if let Some(p) = Path::new(&c.sock).parent() { let _ = std::fs::create_dir_all(p); }
    let listener = UnixListener::bind(&c.sock).expect("bind");
    // R-L: socket must not be world-accessible — anyone who can connect can stop us.
    let _ = std::fs::set_permissions(&c.sock, std::os::unix::fs::PermissionsExt::from_mode(0o600));

    // R-A/R-B: agent runs in the WORKTREE; captures land where told, stdout and
    // stderr to SEPARATE files so stderr can never corrupt JSON stdout parsing.
    let out = std::fs::File::create(&c.stdout).expect("stdout file");
    let err = std::fs::File::create(&c.stderr).expect("stderr file");
    let mut cmd = Command::new(&c.cmd[0]);
    cmd.args(&c.cmd[1..])
        .current_dir(&c.workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .process_group(0); // whole agent subtree in one killable group
    for (k, v) in &c.envs { cmd.env(k, v); } // R-F
    let child = cmd.spawn().expect("spawn agent");
    let apid = child.id();
    std::fs::write(&c.pidfile, format!("{apid}\n")).expect("pidfile"); // R-C
    println!("MONITOR pid={} agent={apid}", std::process::id());

    let child = Arc::new(Mutex::new(child));
    let stopping = Arc::new(AtomicBool::new(false));

    // Natural-exit path: record exit code, then run the advance tail IN-PROCESS.
    {
        let (child, stopping) = (Arc::clone(&child), Arc::clone(&stopping));
        let sock = c.sock.clone();
        let exitfile = c.exitfile.clone();
        let advance = c.advance_marker.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(30));
            if stopping.load(Ordering::SeqCst) { return; }
            let st = { child.lock().unwrap().try_wait() };
            if let Ok(Some(st)) = st {
                // R-D: real exit code, including signal-death (128+n, shell convention)
                let code = st.code().unwrap_or_else(|| 128 + st.signal().unwrap_or(0));
                let _ = std::fs::write(&exitfile, format!("{code}\n"));
                // R-E: the advance tail. In-process — no forked tail process to
                // orphan, which is what the ORIGINAL bug was.
                if let Some(marker) = &advance {
                    let _ = std::fs::write(marker, "advance ran\n");
                }
                let _ = std::fs::remove_file(&sock);
                println!("MONITOR agent exited code={code}; advance={}; exiting",
                    advance.is_some());
                std::process::exit(0);
            }
        });
    }

    for s in listener.incoming() {
        let Ok(mut s) = s else { continue };
        let mut line = String::new();
        if BufReader::new(s.try_clone().unwrap()).read_line(&mut line).is_err() { continue }
        match line.trim() {
            "ping" => { let _ = writeln!(s, "alive pid={} agent={apid}", std::process::id()); }
            "shutdown" => {
                // R-M: a STOP is not a completion. Suppress the advance tail.
                stopping.store(true, Ordering::SeqCst);
                unsafe { libc::kill(-(apid as i32), libc::SIGTERM); }
                let mut g = child.lock().unwrap();
                let dl = std::time::Instant::now() + std::time::Duration::from_secs(3);
                loop {
                    if matches!(g.try_wait(), Ok(Some(_))) { break }
                    if std::time::Instant::now() >= dl {
                        unsafe { libc::kill(-(apid as i32), libc::SIGKILL); }
                        let _ = g.wait(); break
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                // R-K: teardown still records an exit code so downstream
                // evaluation sees a definite outcome, not silence.
                let _ = std::fs::write(&c.exitfile, "143\n");
                let _ = writeln!(s, "stopped");
                let _ = s.flush();
                let _ = std::fs::remove_file(&c.sock);
                println!("MONITOR stopped by request; advance SUPPRESSED");
                std::process::exit(0);
            }
            o => { let _ = writeln!(s, "unknown {o}"); }
        }
    }
}

fn status(sock: &str) {
    if !Path::new(sock).exists() { println!("GONE"); return }
    match UnixStream::connect(sock) {
        Ok(mut s) => {
            let _ = writeln!(s, "ping");
            let mut r = String::new();
            let _ = BufReader::new(s).read_line(&mut r);
            println!("ALIVE ({})", r.trim());
        }
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => println!("STALE"),
        Err(e) => println!("UNKNOWN {e}"),
    }
}

fn stop(sock: &str) {
    match UnixStream::connect(sock) {
        Ok(mut s) => {
            let _ = writeln!(s, "shutdown");
            let mut r = String::new();
            let _ = BufReader::new(s).read_line(&mut r);
            println!("STOP -> {}", r.trim());
        }
        Err(e) => println!("STOP failed: {e}"),
    }
}

/// R-J: project-wide sweep with NO state file — just list and probe.
fn sweep(dir: &str) {
    let Ok(rd) = std::fs::read_dir(dir) else { println!("sweep: no dir"); return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("sock") { continue }
        let ps = p.to_str().unwrap();
        let state = match UnixStream::connect(ps) {
            Ok(_) => "ALIVE",
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => "STALE",
            Err(_) => "UNKNOWN",
        };
        println!("  {state}  {ps}");
    }
}
