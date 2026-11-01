//! One desk, one socket. A second window leaves when this socket answers.
//! The file is mode 0600. A leftover file is removed only when a ping fails.

use crate::engine::Hub;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const LINE_CAP: usize = 64 * 1024;

pub fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").ok().filter(|dir| !dir.is_empty()).unwrap_or_else(|| {
        let uid = unsafe { libc::geteuid() };
        format!("/run/user/{uid}")
    });
    PathBuf::from(dir).join(format!("{}.sock", crate::PROCESS_NAME))
}

pub enum SpawnError {
    AlreadyRunning,
    Failed(String),
}

pub enum Claim {
    Ready(UnixListener),
    AlreadyRunning,
}

/// Bind the desk socket, or report that a running desk already answered.
pub fn spawn(hub: Arc<Hub>) -> Result<(), SpawnError> {
    match claim_at(&socket_path()).map_err(SpawnError::Failed)? {
        Claim::AlreadyRunning => Err(SpawnError::AlreadyRunning),
        Claim::Ready(listener) => {
            thread::Builder::new()
                .name("rustmix-socket".into())
                .spawn(move || accept_loop(listener, hub))
                .map_err(|err| SpawnError::Failed(err.to_string()))?;
            Ok(())
        }
    }
}

pub fn claim_at(path: &Path) -> Result<Claim, String> {
    if path.exists() {
        if ping(path) {
            return Ok(Claim::AlreadyRunning);
        }
        std::fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    let listener = UnixListener::bind(path).map_err(|err| err.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|err| err.to_string())?;
    Ok(Claim::Ready(listener))
}

fn accept_loop(listener: UnixListener, hub: Arc<Hub>) {
    for incoming in listener.incoming() {
        let Ok(stream) = incoming else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
        handle_client(stream, &hub);
    }
}

pub fn handle_client(mut stream: UnixStream, hub: &Hub) {
    loop {
        let Ok(line) = read_line(&mut stream) else { return };
        if line.is_empty() {
            continue;
        }
        let reply = {
            let mut engine = hub.lock();
            engine.request(&line)
        };
        hub.poke();
        if write_line(&mut stream, &reply).is_err() {
            return;
        }
    }
}

fn ping(path: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
    if stream.write_all(b"{\"method\":\"ping\"}\n").is_err() {
        return false;
    }
    let Ok(line) = read_line(&mut stream) else { return false };
    line.contains("\"ok\":true")
}

fn read_line(stream: &mut UnixStream) -> Result<String, ()> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return Err(()),
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => {
                if buf.len() >= LINE_CAP {
                    return Err(());
                }
                buf.push(byte[0]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(()),
        }
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|_| ())
}

fn write_line(stream: &mut UnixStream, line: &str) -> Result<(), ()> {
    stream.write_all(line.as_bytes()).map_err(|_| ())?;
    stream.write_all(b"\n").map_err(|_| ())?;
    stream.flush().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use crate::pipewire::{CmdOut, Runner};
    use crate::settings::Settings;
    use std::fs;

    struct Nop;

    impl Runner for Nop {
        fn run(&self, _args: &[String]) -> Result<CmdOut, String> {
            Ok(CmdOut { code: 0, stdout: "[]".into(), stderr: String::new() })
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rustmix-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_stale_socket_is_replaced_and_a_live_one_is_kept() {
        let dir = scratch("sock");
        let path = dir.join("desk.sock");
        fs::write(&path, b"not a socket").unwrap();
        let listener = match claim_at(&path).unwrap() {
            Claim::Ready(listener) => listener,
            Claim::AlreadyRunning => panic!("a dead file answered"),
        };
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let hub = Hub::new(Engine::from_parts(Settings::default(), None, Arc::new(Nop)));
        let path2 = path.clone();
        let server = thread::spawn(move || {
            for incoming in listener.incoming() {
                let Ok(stream) = incoming else { break };
                handle_client(stream, &hub);
            }
        });
        let mut answered = false;
        for _ in 0..50 {
            if ping(&path2) {
                answered = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(answered, "the bound socket did not answer");
        assert!(matches!(claim_at(&path2).unwrap(), Claim::AlreadyRunning));
        drop(server);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_line_refuses_a_private_level_without_naming_it() {
        let dir = scratch("line");
        let path = dir.join("desk.sock");
        let listener = match claim_at(&path).unwrap() {
            Claim::Ready(listener) => listener,
            Claim::AlreadyRunning => panic!("empty dir was taken"),
        };
        let hub = Hub::new(Engine::from_parts(Settings::default(), None, Arc::new(Nop)));
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_client(stream, &hub);
        });
        let mut client = UnixStream::connect(&path).unwrap();
        client.write_all(b"{\"id\":1,\"method\":\"set_monitor\",\"kMainTrim\":0}\n").unwrap();
        let line = read_line(&mut client).unwrap();
        assert!(line.contains("\"ok\":false"), "{line}");
        assert!(line.contains("not available"), "{line}");
        assert!(!line.contains("kMainTrim"), "{line}");
        assert!(!line.contains("5011"), "{line}");
        drop(client);
        let _ = server.join();
        let _ = fs::remove_dir_all(&dir);
    }
}
