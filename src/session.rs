//! Probe, then enable, then replace the desk with what the unit sent.
//! Demo edits are never written down this path.

use crate::desk::Desk;
use crate::engine::{DeviceEvent, Hub};
use crate::midi::{self, OpenError, Port};
use crate::protocol::{self, Incoming, Splitter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub trait BytePort {
    fn write_frame(&mut self, frame: &[u8]) -> Result<(), String>;
    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, String>;
}

impl BytePort for Port {
    fn write_frame(&mut self, frame: &[u8]) -> Result<(), String> {
        self.write(frame)
    }

    fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        self.read(buf)
    }
}

pub fn pull_desk<P: BytePort>(
    port: &mut P,
    mut now: impl FnMut() -> Instant,
    mut on_idle: impl FnMut(),
) -> Result<Desk, String> {
    port.write_frame(&protocol::probe_frame())?;
    let started = now();
    let mut splitter = Splitter::default();
    let mut buf = vec![0u8; 64 * 1024];
    let mut saw_probe = false;
    while now().saturating_duration_since(started) < Duration::from_secs(1) {
        let n = port.read_some(&mut buf)?;
        if n == 0 {
            on_idle();
            continue;
        }
        for frame in splitter.push(&buf[..n]) {
            if matches!(protocol::decode(&frame), Incoming::ProbeReply) {
                saw_probe = true;
            }
        }
        if saw_probe {
            break;
        }
    }
    if !saw_probe {
        return Err("The control port did not answer.".into());
    }
    port.write_frame(&protocol::enable_frame())?;
    let enabled = now();
    let mut desk = Desk::blank();
    loop {
        let elapsed = now().saturating_duration_since(enabled);
        if elapsed > Duration::from_millis(2000) {
            break;
        }
        let n = port.read_some(&mut buf)?;
        if n == 0 {
            if desk.identity_ready() && (desk.mix_loaded || elapsed > Duration::from_millis(1500)) {
                break;
            }
            on_idle();
            continue;
        }
        for frame in splitter.push(&buf[..n]) {
            if let Incoming::Report { prop, index, raw } = protocol::decode(&frame) {
                desk.apply_report(prop, index, &raw);
            }
        }
    }
    if !desk.identity_ready() {
        return Err("The 828 did not finish introducing itself.".into());
    }
    Ok(desk)
}

pub fn begin(hub: &Arc<Hub>) -> Result<(), String> {
    {
        let engine = hub.lock();
        if engine.connecting {
            return Err(format!("{} is still opening the 828.", crate::APP_NAME));
        }
        if matches!(engine.link, crate::engine::Link::Live | crate::engine::Link::ReadOnly { .. }) {
            return Err(format!("{} already has the 828 control port.", crate::APP_NAME));
        }
    }
    let (event_tx, event_rx) = mpsc::channel();
    let (frame_tx, frame_rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut engine = hub.lock();
        engine.attach_connect(event_rx, frame_tx, Arc::clone(&stop));
        engine.notice = "Looking for the 828…".into();
    }
    let hub = Arc::clone(hub);
    thread::Builder::new()
        .name("crimson-midi".into())
        .spawn(move || thread_main(event_tx, frame_rx, stop, hub))
        .map_err(|_| "The connection thread did not start.".to_string())?;
    Ok(())
}

fn thread_main(events: Sender<DeviceEvent>, frames: Receiver<Vec<u8>>, stop: Arc<AtomicBool>, hub: Arc<Hub>) {
    match open_and_sync(&stop) {
        Ok((desk, mut port)) => {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let readonly = desk.contract_problem();
            let _ = events.send(DeviceEvent::Synced { desk, readonly });
            hub.poke();
            steady(&mut port, &events, &frames, &stop, &hub);
        }
        Err(message) => {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let _ = events.send(DeviceEvent::Failed(message));
            hub.poke();
        }
    }
}

fn open_and_sync(stop: &AtomicBool) -> Result<(Desk, Port), String> {
    if stop.load(Ordering::Relaxed) {
        return Err("The connection was cancelled.".into());
    }
    let hw = midi::locate_com()?;
    midi::prepare(&hw)?;
    let mut port = match Port::open(&hw) {
        Ok(port) => port,
        Err(OpenError::Busy) => {
            return Err(format!("Another program has the 828 control port. {} is staying on the demo.", crate::APP_NAME));
        }
        Err(OpenError::Failed(message)) => return Err(message),
    };
    let desk = pull_desk(&mut port, Instant::now, || thread::sleep(Duration::from_millis(5)))?;
    Ok((desk, port))
}

fn steady(port: &mut Port, events: &Sender<DeviceEvent>, frames: &Receiver<Vec<u8>>, stop: &AtomicBool, hub: &Hub) {
    let mut splitter = Splitter::default();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match frames.recv_timeout(Duration::from_millis(20)) {
            Ok(frame) => {
                if port.write(&frame).is_err() {
                    let _ = events.send(DeviceEvent::Lost("The 828 control port closed.".into()));
                    hub.poke();
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        match port.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => {
                let mut other = false;
                let mut meter = false;
                for frame in splitter.push(&buf[..n]) {
                    if let Incoming::Report { prop, index, raw } = protocol::decode(&frame) {
                        if prop == protocol::PROP_METERS && crate::gear::full_meter(index, &raw) {
                            hub.store_meter(index, raw);
                            meter = true;
                        } else {
                            let _ = events.send(DeviceEvent::Report { prop, index, raw });
                            other = true;
                        }
                    }
                }
                // A full meter picture replaces the previous one. Waking a minimized
                // window for it is what kept the process painting with nobody looking.
                if other || (meter && hub.show_live()) {
                    hub.poke();
                }
            }
            Err(_) => {
                let _ = events.send(DeviceEvent::Lost("The 828 control port closed.".into()));
                hub.poke();
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{encode_i16, PROP_API, PROP_MODEL, PROP_NAME, PROP_UID};
    use std::cell::Cell;

    struct Script {
        chunks: Vec<Vec<u8>>,
        cursor: usize,
        written: Vec<Vec<u8>>,
    }

    impl BytePort for Script {
        fn write_frame(&mut self, frame: &[u8]) -> Result<(), String> {
            self.written.push(frame.to_vec());
            Ok(())
        }

        fn read_some(&mut self, buf: &mut [u8]) -> Result<usize, String> {
            if self.cursor >= self.chunks.len() {
                return Ok(0);
            }
            let chunk = &self.chunks[self.cursor];
            let n = chunk.len().min(buf.len());
            buf[..n].copy_from_slice(&chunk[..n]);
            self.cursor += 1;
            Ok(n)
        }
    }

    #[test]
    fn sync_replaces_from_the_unit_and_sends_only_probe_and_enable() {
        let mut script = Script {
            chunks: vec![
                protocol::probe_reply_frame(),
                protocol::encode_report(PROP_MODEL, 0, b"828E\0"),
                protocol::encode_report(PROP_API, 0, &encode_i16(9)),
                protocol::encode_report(PROP_NAME, 0, b"828\0"),
                protocol::encode_report(PROP_UID, 0, b"ABC\0"),
                protocol::encode_report(crate::protocol::PROP_PATCH, 37, &[127]),
                protocol::encode_report(crate::protocol::PROP_MONITOR, 0, &[55]),
            ],
            cursor: 0,
            written: Vec::new(),
        };
        let start = Instant::now();
        let jumps = Cell::new(0u32);
        let desk = pull_desk(
            &mut script,
            || {
                if jumps.get() < 2 { start } else { start + Duration::from_secs(3) }
            },
            || jumps.set(jumps.get() + 1),
        )
        .unwrap();
        assert_eq!(script.written.len(), 2);
        assert_eq!(script.written[0], protocol::probe_frame());
        assert_eq!(script.written[1], protocol::enable_frame());
        assert_eq!(desk.model, "828E");
        assert_eq!(desk.api, 9);
        assert_eq!(desk.uid, "ABC");
        assert_eq!(desk.patch[37], 127);
        assert_eq!(desk.monitor, Some(55));
        assert!(desk.contract_problem().is_none());
        let mut other = Desk::blank();
        other.model = "other".into();
        other.api = 1;
        other.saw_model = true;
        other.saw_api = true;
        assert!(other.contract_problem().unwrap().contains("read-only"));
    }
}
