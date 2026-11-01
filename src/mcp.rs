//! MCP on stdin, and the address Control opens.
//! This process never opens the 828. Stdin forwards tool calls to the desk
//! that is already running. The desk also listens on the address from Control
//! when that switch is on. It has no level tools.

use crate::engine::Hub;
use crate::ipc;
use crate::settings;
use serde_json::{json, Map, Value};
use std::io::{self, BufRead, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn serve() -> Result<(), String> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        let Some(message) = read_message(&mut input)? else { return Ok(()) };
        if let Some(reply) = dispatch(&message, &mut ask_socket) {
            write_message(&reply)?;
        }
    }
}

fn ask_socket(line: &str) -> Result<String, ()> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;
    let mut stream = UnixStream::connect(ipc::socket_path()).map_err(|_| ())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
    stream.write_all(line.as_bytes()).map_err(|_| ())?;
    stream.write_all(b"\n").map_err(|_| ())?;
    stream.flush().map_err(|_| ())?;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return Err(()),
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => {
                if buf.len() >= 64 * 1024 {
                    return Err(());
                }
                buf.push(byte[0]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(()),
        }
    }
    String::from_utf8(buf).map_err(|_| ())
}

pub fn dispatch(message: &Value, ask: &mut dyn FnMut(&str) -> Result<String, ()>) -> Option<Value> {
    let Value::Object(object) = message else { return None };
    let method = object.get("method").and_then(|item| item.as_str()).unwrap_or("");
    let id = object.get("id").cloned();
    if method.starts_with("notifications/") || id.is_none() && method != "initialize" {
        return None;
    }
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": crate::PROCESS_NAME, "version": "0.1.0" }
        })),
        "tools/list" => Ok(json!({ "tools": tool_list() })),
        "tools/call" => call_tool(object.get("params").unwrap_or(&Value::Null), ask),
        "ping" => Ok(json!({})),
        _ => Err((-32601, "That request is not available.".to_string())),
    };
    Some(match result {
        Ok(body) => json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": body }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0",
            "id": id.unwrap_or(Value::Null),
            "error": { "code": code, "message": message }
        }),
    })
}

fn call_tool(params: &Value, ask: &mut dyn FnMut(&str) -> Result<String, ()>) -> Result<Value, (i32, String)> {
    let name = params.get("name").and_then(|item| item.as_str()).unwrap_or("");
    if !tool_allowed(name) {
        return Ok(tool_error("That control is not available here."));
    }
    let mut request = Map::new();
    request.insert("method".into(), Value::String(name.into()));
    if let Some(args) = params.get("arguments").and_then(|item| item.as_object()) {
        for (key, value) in args {
            request.insert(key.clone(), value.clone());
        }
    }
    request.insert("method".into(), Value::String(name.into()));
    let line = serde_json::to_string(&Value::Object(request)).map_err(|_| (-32603, "The request could not be written.".into()))?;
    let reply = match ask(&line) {
        Ok(reply) => reply,
        Err(()) => return Ok(tool_error(&format!("{} is not running.", crate::APP_NAME))),
    };
    let value: Value = match serde_json::from_str(&reply) {
        Ok(value) => value,
        Err(_) => return Ok(tool_error(&format!("{} is not running.", crate::APP_NAME))),
    };
    if value.get("ok").and_then(|item| item.as_bool()) == Some(true) {
        let text = serde_json::to_string(value.get("result").unwrap_or(&Value::Null)).unwrap_or_else(|_| "{}".into());
        Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": false }))
    } else {
        let text = value.get("error").and_then(|item| item.as_str()).unwrap_or("That request is not available.");
        Ok(tool_error(text))
    }
}

fn tool_error(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

fn tool_allowed(name: &str) -> bool {
    matches!(
        name,
        "status" | "list_routes" | "connect_route" | "disconnect_route" | "list_programs"
            | "route_program" | "unroute_program" | "set_mix" | "set_bus" | "list_pages" | "show_page" | "ping"
    )
}

fn tool_list() -> Vec<Value> {
    vec![
        tool("status", "Report whether the desk is on the demo or the 828.", &[], &[]),
        tool("list_routes", "List each output and the source feeding it.", &[], &[]),
        tool(
            "connect_route",
            "Feed one source into one output. The same source can feed more than one output.",
            &[("slot", "integer", "Output slot."), ("source", "integer", "Source number.")],
            &["slot", "source"],
        ),
        tool(
            "disconnect_route",
            "Clear one output.",
            &[("slot", "integer", "Output slot.")],
            &["slot"],
        ),
        tool("list_programs", "List programs on this PC that are playing or recording.", &[], &[]),
        tool(
            "route_program",
            "Move one program to an 828 playback or recording device, or to another program.",
            &[("id", "string", "Program id from list_programs."), ("target", "string", "Device id, desk, or default.")],
            &["id", "target"],
        ),
        tool(
            "unroute_program",
            "Send one program back to the default device.",
            &[("id", "string", "Program id from list_programs.")],
            &["id"],
        ),
        tool(
            "set_mix",
            "Change one mixer channel. The level is decibels from -72 to 12. One channel per call.",
            &[
                ("bus", "integer", "Bus number."),
                ("input", "integer", "Input number."),
                ("db", "number", "Level in decibels."),
                ("pan", "number", "Pan from 0 (left) to 1 (right)."),
                ("mute", "boolean", "Mute this channel."),
                ("solo", "boolean", "Solo this channel."),
            ],
            &["bus", "input"],
        ),
        tool(
            "set_bus",
            "Change one bus master. The level is decibels from -72 to 12.",
            &[
                ("bus", "integer", "Bus number."),
                ("db", "number", "Level in decibels."),
                ("mute", "boolean", "Mute this bus."),
            ],
            &["bus"],
        ),
        tool("list_pages", "List the pages on this desk. This does not change a level, a route, or a lock.", &[], &[]),
        tool(
            "show_page",
            "Show one page. mixer accepts a bus from 0 to 15. This does not change a level, a route, or a lock.",
            &[
                ("page", "string", "home, device, control, input, output, routing, discovery, mixer, or pc."),
                ("bus", "integer", "Mixer bus, 0 through 15. Only used with mixer."),
            ],
            &["page"],
        ),
    ]
}

fn tool(name: &str, description: &str, fields: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let mut properties = Map::new();
    for (key, kind, description) in fields {
        properties.insert((*key).into(), json!({ "type": kind, "description": description }));
    }
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        }
    })
}

fn read_message(input: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut len = None;
    loop {
        let mut header = String::new();
        let n = input.read_line(&mut header).map_err(|err| err.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            len = rest.trim().parse::<usize>().ok();
        }
    }
    let Some(len) = len else {
        return Err("The MCP message had no length.".into());
    };
    if len > 1024 * 1024 {
        return Err("The MCP message was too long.".into());
    }
    let mut body = vec![0u8; len];
    input.read_exact(&mut body).map_err(|err| err.to_string())?;
    serde_json::from_slice(&body).map(Some).map_err(|_| "The MCP message was not valid.".into())
}

fn write_message(message: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(message).map_err(|err| err.to_string())?;
    let mut out = io::stdout().lock();
    write!(out, "Content-Length: {}\r\n\r\n", body.len()).map_err(|err| err.to_string())?;
    out.write_all(&body).map_err(|err| err.to_string())?;
    out.flush().map_err(|err| err.to_string())
}

struct Running {
    stop: Arc<AtomicBool>,
    join: thread::JoinHandle<()>,
    host: String,
    port: u16,
}

fn running_slot() -> &'static Mutex<Option<Running>> {
    static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
    &RUNNING
}

/// Open or close the address from Control. The desk socket stays either way.
pub fn sync(hub: Arc<Hub>) {
    let (enabled, host, port) = {
        let engine = hub.lock();
        (engine.settings.mcp_enabled, engine.settings.mcp_host.clone(), engine.settings.mcp_port)
    };
    let mut slot = running_slot().lock().unwrap_or_else(|err| err.into_inner());
    if let Some(running) = slot.as_ref() {
        let same = enabled && running.host == host && running.port == port && !running.stop.load(Ordering::Relaxed) && !running.join.is_finished();
        if same {
            return;
        }
    }
    if let Some(running) = slot.take() {
        running.stop.store(true, Ordering::Relaxed);
        let _ = running.join.join();
    }
    if !enabled {
        drop(slot);
        let mut engine = hub.lock();
        engine.mcp_listen = "MCP is off.".into();
        drop(engine);
        hub.poke();
        return;
    }
    let addr = match socket_addr(&host, port) {
        Ok(addr) => addr,
        Err(message) => {
            drop(slot);
            note(&hub, message);
            return;
        }
    };
    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(err) => {
            drop(slot);
            note(&hub, bind_message(&err));
            return;
        }
    };
    let _ = listener.set_nonblocking(true);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_thread = Arc::clone(&stop);
    let hub_for_thread = Arc::clone(&hub);
    let bound_host = host.clone();
    let join = thread::Builder::new()
        .name("crimson-mcp".into())
        .spawn(move || accept_http(listener, hub_for_thread, stop_for_thread, bound_host))
        .expect("mcp thread");
    *slot = Some(Running { stop, join, host: host.clone(), port });
    drop(slot);
    note(&hub, listen_line(&host, port));
}

fn note(hub: &Hub, text: String) {
    hub.lock().mcp_listen = text;
    hub.poke();
}

fn socket_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let text = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    text.parse().map_err(|_| "Type an IP address, or localhost.".to_string())
}

fn bind_message(err: &std::io::Error) -> String {
    if err.kind() == std::io::ErrorKind::AddrInUse || err.raw_os_error() == Some(98) {
        "That port is in use.".into()
    } else if err.raw_os_error() == Some(99) {
        "That address is not on this PC.".into()
    } else {
        "MCP could not listen.".into()
    }
}

fn listen_line(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("Listening on [{host}]:{port}.")
    } else {
        format!("Listening on {host}:{port}.")
    }
}

fn accept_http(listener: TcpListener, hub: Arc<Hub>, stop: Arc<AtomicBool>, host: String) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                handle_http(stream, &hub, &host);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

fn handle_http(stream: TcpStream, hub: &Hub, host: &str) {
    let mut reader = io::BufReader::new(stream);
    loop {
        let Some((request, headers)) = read_headers(&mut reader) else { return };
        let mut parts = request.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("/");
        let path = path.split('?').next().unwrap_or("/");
        if path != "/" && path != "/mcp" {
            let _ = write_http(&mut reader, 404, "application/json", b"{\"error\":\"not found\"}", true);
            return;
        }
        let mut length = 0usize;
        let mut accept = String::new();
        let mut origin = String::new();
        let mut close = false;
        for (key, value) in &headers {
            if key == "content-length" {
                length = value.parse().unwrap_or(usize::MAX);
            } else if key == "accept" {
                accept.clone_from(value);
            } else if key == "origin" {
                origin.clone_from(value);
            } else if key == "connection" && value.eq_ignore_ascii_case("close") {
                close = true;
            }
        }
        if !origin.is_empty() && settings::mcp_is_loopback(host) && !origin_ok(&origin) {
            let _ = write_http(&mut reader, 403, "application/json", b"{\"error\":\"refused\"}", true);
            return;
        }
        if length > 1024 * 1024 {
            let _ = write_http(&mut reader, 413, "application/json", b"{\"error\":\"too large\"}", true);
            return;
        }
        let mut body = vec![0u8; length];
        if length > 0 && reader.read_exact(&mut body).is_err() {
            return;
        }
        if method == "OPTIONS" {
            let _ = write_http(&mut reader, 204, "text/plain", b"", close);
            if close {
                return;
            }
            continue;
        }
        if method != "POST" {
            let _ = write_http(&mut reader, 405, "application/json", b"{\"error\":\"POST only\"}", true);
            return;
        }
        if !accept.is_empty() && !accept.contains("application/json") && !accept.contains("text/event-stream") && !accept.contains("*/*") {
            let _ = write_http(&mut reader, 406, "application/json", b"{\"error\":\"not acceptable\"}", true);
            return;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            let _ = write_http(&mut reader, 400, "application/json", b"{\"error\":\"bad json\"}", true);
            return;
        };
        if !message.is_object() {
            let _ = write_http(&mut reader, 400, "application/json", b"{\"error\":\"bad json\"}", true);
            return;
        }
        let sse = accept.contains("text/event-stream") && !accept.contains("application/json") && !accept.contains("*/*");
        let reply = dispatch(&message, &mut |line| {
            let mut engine = hub.lock();
            Ok(engine.request(line))
        });
        let Some(reply) = reply else {
            let _ = write_http(&mut reader, 202, "application/json", b"", close);
            if close {
                return;
            }
            continue;
        };
        let bytes = serde_json::to_vec(&reply).unwrap_or_else(|_| b"{}".to_vec());
        if sse {
            let mut event = Vec::new();
            event.extend_from_slice(b"event: message\ndata: ");
            event.extend_from_slice(&bytes);
            event.extend_from_slice(b"\n\n");
            let _ = write_http(&mut reader, 200, "text/event-stream", &event, true);
            return;
        }
        if write_http(&mut reader, 200, "application/json", &bytes, close).is_err() || close {
            return;
        }
    }
}

fn origin_ok(origin: &str) -> bool {
    let lower = origin.to_ascii_lowercase();
    lower == "null" || lower.contains("127.0.0.1") || lower.contains("localhost") || lower.contains("[::1]")
}

fn read_headers(reader: &mut io::BufReader<TcpStream>) -> Option<(String, Vec<(String, String)>)> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let request = line.trim().to_string();
    if request.is_empty() {
        return None;
    }
    let mut headers = Vec::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            headers.push((key.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
        if headers.len() > 64 {
            return None;
        }
    }
    Some((request, headers))
}

fn write_http(reader: &mut io::BufReader<TcpStream>, status: u16, kind: &str, body: &[u8], close: bool) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let connection = if close { "close" } else { "keep-alive" };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: {connection}\r\nAccess-Control-Allow-Origin: http://127.0.0.1\r\nAccess-Control-Allow-Headers: content-type, accept, mcp-session-id\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\n\r\n",
        body.len()
    );
    let stream = reader.get_mut();
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_and_the_tool_list_have_no_level_controls() {
        let message = json!({"jsonrpc":"2.0","id":1,"method":"initialize"});
        let mut asked = false;
        let reply = dispatch(&message, &mut |_| {
            asked = true;
            Err(())
        })
        .unwrap();
        assert!(!asked);
        assert_eq!(reply["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(reply["result"]["serverInfo"]["name"], crate::PROCESS_NAME);
        let list = dispatch(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), &mut |_| Err(())).unwrap();
        let text = list.to_string();
        for denied in ["set_monitor", "set_phones", "set_trim", "set_phantom", "kMainTrim", "koTrim", "5011", "5000"] {
            assert!(!text.contains(denied), "{denied} in {text}");
        }
        assert!(text.contains("connect_route"));
        assert!(text.contains("set_mix"));
        assert!(text.contains("show_page"));
        assert!(text.contains("list_pages"));
    }

    #[test]
    fn a_missing_desk_is_a_tool_error_and_a_notice_has_no_reply() {
        let call = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}});
        let reply = dispatch(&call, &mut |_| Err(())).unwrap();
        assert_eq!(reply["result"]["isError"], true);
        assert_eq!(reply["result"]["content"][0]["text"], format!("{} is not running.", crate::APP_NAME));
        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(dispatch(&note, &mut |_| Err(())).is_none());
    }

    #[test]
    fn a_private_tool_is_refused_before_the_socket() {
        let call = json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"set_monitor","arguments":{}}});
        let mut asked = false;
        let reply = dispatch(&call, &mut |_| {
            asked = true;
            Err(())
        })
        .unwrap();
        assert!(!asked);
        assert_eq!(reply["result"]["isError"], true);
        let text = reply["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "That control is not available here.");
    }

    struct Nop;

    impl crate::pipewire::Runner for Nop {
        fn run(&self, _args: &[String]) -> Result<crate::pipewire::CmdOut, String> {
            Ok(crate::pipewire::CmdOut { code: 0, stdout: "[]".into(), stderr: String::new() })
        }
    }

    fn test_hub(enabled: bool) -> Arc<Hub> {
        let mut settings = crate::settings::Settings::default();
        settings.mcp_enabled = enabled;
        Hub::new(crate::engine::Engine::from_parts(settings, None, Arc::new(Nop)))
    }

    fn post(port: u16, body: &str, accept: &str, origin: Option<&str>) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let origin = origin.map(|value| format!("Origin: {value}\r\n")).unwrap_or_default();
        write!(
            stream,
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: {accept}\r\n{origin}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn the_address_answers_initialize_and_refuses_a_foreign_page() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let hub = test_hub(true);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);
        let server = thread::spawn(move || accept_http(listener, hub, stop_for_thread, "127.0.0.1".into()));
        let mut response = String::new();
        for _ in 0..20 {
            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
                write!(
                    stream,
                    "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                stream.read_to_string(&mut response).unwrap();
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(response.contains("200"), "{response}");
        assert!(response.contains("application/json"), "{response}");
        assert!(response.contains(crate::PROCESS_NAME), "{response}");
        assert!(response.contains("2024-11-05"), "{response}");
        let foreign = post(port, r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#, "application/json", Some("http://evil.example"));
        assert!(foreign.contains("403"), "{foreign}");
        let tool = post(
            port,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"status","arguments":{}}}"#,
            "application/json",
            None,
        );
        assert!(tool.contains("\"isError\":false"), "{tool}");
        assert!(tool.contains("demo"), "{tool}");
        assert!(!tool.contains("set_monitor"), "{tool}");
        stop.store(true, Ordering::Relaxed);
        let _ = server.join();
    }

    #[test]
    fn sync_stays_quiet_until_mcp_is_on() {
        let hub = test_hub(false);
        sync(Arc::clone(&hub));
        assert_eq!(hub.lock().mcp_listen, "MCP is off.");
        assert!(!hub.lock().settings.mcp_enabled);
    }
}
