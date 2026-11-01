//! The 828 control port, found by the name "828 COM". Opening it is refused
//! while any process already holds the device node. The open itself is
//! non-blocking, and the input buffer is raised before the dump arrives.

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;
use std::process::Command;

const NONBLOCK: c_int = 0x0002;
pub const INPUT_BUFFER: usize = 1 << 20;
const PORT_NAME: &str = "828 COM";

#[link(name = "asound")]
unsafe extern "C" {
    fn snd_rawmidi_open(
        input: *mut *mut c_void,
        output: *mut *mut c_void,
        name: *const c_char,
        mode: c_int,
    ) -> c_int;
    fn snd_rawmidi_close(raw: *mut c_void) -> c_int;
    fn snd_rawmidi_nonblock(raw: *mut c_void, nonblock: c_int) -> c_int;
    fn snd_rawmidi_read(raw: *mut c_void, buffer: *mut c_void, size: usize) -> isize;
    fn snd_rawmidi_write(raw: *mut c_void, buffer: *const c_void, size: usize) -> isize;
    fn snd_rawmidi_params_malloc(ptr: *mut *mut c_void) -> c_int;
    fn snd_rawmidi_params_free(raw: *mut c_void) -> c_int;
    fn snd_rawmidi_params_current(raw: *mut c_void, params: *mut c_void) -> c_int;
    fn snd_rawmidi_params_set_buffer_size(raw: *mut c_void, params: *mut c_void, size: usize) -> c_int;
    fn snd_rawmidi_params(raw: *mut c_void, params: *mut c_void) -> c_int;
}

pub enum OpenError {
    Busy,
    Failed(String),
}

pub struct Port {
    input: *mut c_void,
    output: *mut c_void,
}

unsafe impl Send for Port {}

impl Port {
    pub fn open(hw: &str) -> Result<Self, OpenError> {
        let name = CString::new(hw).map_err(|_| OpenError::Failed("The port name is not valid.".into()))?;
        let mut input = std::ptr::null_mut();
        let mut output = std::ptr::null_mut();
        let rc = unsafe { snd_rawmidi_open(&mut input, &mut output, name.as_ptr(), NONBLOCK) };
        if rc == -libc::EBUSY {
            return Err(OpenError::Busy);
        }
        if rc < 0 || input.is_null() || output.is_null() {
            return Err(OpenError::Failed("The 828 control port did not open.".into()));
        }
        unsafe { snd_rawmidi_nonblock(input, 1) };
        if let Err(message) = grow_input(input) {
            unsafe {
                snd_rawmidi_close(input);
                snd_rawmidi_close(output);
            }
            return Err(OpenError::Failed(message));
        }
        Ok(Self { input, output })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        let n = unsafe { snd_rawmidi_read(self.input, buf.as_mut_ptr().cast(), buf.len()) };
        if n == -(libc::EAGAIN as isize) {
            return Ok(0);
        }
        if n < 0 {
            return Err("The 828 control port closed.".into());
        }
        Ok(n as usize)
    }

    pub fn write(&mut self, frame: &[u8]) -> Result<(), String> {
        let n = unsafe { snd_rawmidi_write(self.output, frame.as_ptr().cast(), frame.len()) };
        if n < 0 || n as usize != frame.len() {
            Err("The 828 control port closed.".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        unsafe {
            if !self.input.is_null() {
                snd_rawmidi_close(self.input);
            }
            if !self.output.is_null() {
                snd_rawmidi_close(self.output);
            }
        }
    }
}

fn grow_input(handle: *mut c_void) -> Result<(), String> {
    let mut params = std::ptr::null_mut();
    let rc = unsafe { snd_rawmidi_params_malloc(&mut params) };
    if rc < 0 || params.is_null() {
        return Err("The 828 control port could not take a large enough input buffer.".into());
    }
    let result = unsafe {
        let mut rc = snd_rawmidi_params_current(handle, params);
        if rc >= 0 {
            rc = snd_rawmidi_params_set_buffer_size(handle, params, INPUT_BUFFER);
        }
        if rc >= 0 {
            rc = snd_rawmidi_params(handle, params);
        }
        rc
    };
    unsafe { snd_rawmidi_params_free(params) };
    if result < 0 {
        Err("The 828 control port could not take a large enough input buffer.".into())
    } else {
        Ok(())
    }
}

pub fn parse_amidi(text: &str) -> Option<String> {
    for line in text.lines() {
        let (_, device, name) = split_amidi(line)?;
        if name == PORT_NAME {
            return Some(device.to_string());
        }
    }
    None
}

fn split_amidi(line: &str) -> Option<(&str, &str, &str)> {
    let line = line.trim();
    let mut parts = line.split_whitespace();
    let dir = parts.next()?;
    let device = parts.next()?;
    let start = line.find(device)? + device.len();
    let name = line[start..].trim();
    if name.is_empty() {
        None
    } else {
        Some((dir, device, name))
    }
}

pub fn device_node(hw: &str) -> Option<String> {
    let rest = hw.strip_prefix("hw:")?;
    let mut parts = rest.split(',');
    let card: u32 = parts.next()?.parse().ok()?;
    let device: u32 = parts.next()?.parse().ok()?;
    Some(format!("/dev/snd/midiC{card}D{device}"))
}

/// `holders` is the number of processes that already have the node open.
/// An unknown count is treated as busy so the port is never opened on a guess.
pub fn gate(node_exists: bool, holders: Result<usize, ()>) -> Result<(), &'static str> {
    if !node_exists {
        return Err("The 828 control port is not connected.");
    }
    match holders {
        Ok(0) => Ok(()),
        _ => Err("Another program has the 828 control port. RustMix Crimson Edition is staying on the demo."),
    }
}

pub fn locate_com() -> Result<String, String> {
    let output = Command::new("amidi").arg("-l").output().map_err(|_| {
        "The amidi tool is not installed, so the 828 control port cannot be found.".to_string()
    })?;
    if !output.status.success() {
        return Err("The amidi tool is not installed, so the 828 control port cannot be found.".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_amidi(&text).ok_or_else(|| "The 828 control port is not connected.".into())
}

pub fn prepare(hw: &str) -> Result<(), String> {
    let node = device_node(hw).ok_or_else(|| "The 828 control port is not connected.".to_string())?;
    let exists = Path::new(&node).exists();
    let holders = if exists { count_holders(&node).map_err(|_| ()) } else { Ok(0) };
    gate(exists, holders).map_err(|message| message.to_string())
}

fn count_holders(node: &str) -> Result<usize, String> {
    let mut count = 0;
    let proc = std::fs::read_dir("/proc").map_err(|err| err.to_string())?;
    for entry in proc.flatten() {
        let pid = entry.file_name();
        let pid = pid.to_string_lossy();
        if !pid.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            if std::fs::read_link(fd.path()).ok().as_ref().and_then(|path| path.to_str()) == Some(node) {
                count += 1;
                break;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_control_port_is_chosen_by_name() {
        let listing = "\
Dir Device    Name
IO  hw:4,0,0  828 MIDI
IO  hw:4,0,1  828 COM
";
        assert_eq!(parse_amidi(listing).as_deref(), Some("hw:4,0,1"));
        assert_eq!(device_node("hw:4,0,1").as_deref(), Some("/dev/snd/midiC4D0"));
        assert_eq!(gate(false, Ok(0)), Err("The 828 control port is not connected."));
        assert!(gate(true, Ok(1)).unwrap_err().contains("Another program"));
        assert!(gate(true, Err(())).unwrap_err().contains("Another program"));
        assert!(gate(true, Ok(0)).is_ok());
    }
}
