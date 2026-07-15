//! `howl` — the networking stdlib module. Raw TCP (`listen`/`accept`/`connect`/
//! `send`/`recv`/`recv_line`/`close`) plus a minimal HTTP(S) client
//! (`get`/`post`/`request`). Every network failure — a refused connection, a broken
//! pipe, a TLS or timeout error, an operation on a closed socket — is a catchable
//! IOError rather than a panic, and every socket carries text as one `Str` type:
//! `recv` never splits a multi-byte character, and genuinely invalid bytes are an
//! IOError, never a Rust panic.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::time::Duration;

use crate::error::{DogeError, DogeResult};
use crate::ordered_map::OrderedMap;
use crate::stdlib::{bytes_arg, int_arg, str_arg};
use crate::value::{SocketData, SocketState, Value};

/// How long an HTTP(S) request may run before it is a catchable IOError, so a
/// script can never hang forever on a stalled server. Raw TCP has no timeout.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// The HTTP methods `howl.request` accepts (case-insensitive). Anything else is a
/// catchable ValueError, so a typo never reaches the transport as a strange verb.
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// The chunk size `recv_line` reads with while scanning for a newline.
const LINE_CHUNK: usize = 1024;

/// A Socket argument as its shared handle, or a catchable type error. Every raw
/// TCP member takes a socket as its first argument.
fn socket_arg<'a>(fname: &str, v: &'a Value) -> DogeResult<&'a Rc<SocketData>> {
    match v {
        Value::Socket(s) => Ok(s),
        _ => Err(DogeError::type_error(format!(
            "howl.{fname} needs a Socket, got {}",
            v.describe()
        ))),
    }
}

/// A host/port pair from a Str host and an Int port, or a catchable error. A port
/// outside `0..=65535` is a `ValueError`.
fn host_port<'a>(fname: &str, host: &'a Value, port: &Value) -> DogeResult<(&'a str, u16)> {
    let host = str_arg("howl", fname, host)?;
    let port = int_arg("howl", fname, port)?;
    let port = u16::try_from(port).map_err(|_| {
        DogeError::value_error(format!("a port must be between 0 and 65535, got {port}"))
    })?;
    Ok((host, port))
}

/// `howl.listen(host, port)` — bind a TCP listener on `host:port` (port `0` lets
/// the OS choose a free one, readable back with `howl.port`). A bind failure is a
/// catchable IOError.
pub fn howl_listen(host: &Value, port: &Value) -> DogeResult {
    let (host, port) = host_port("listen", host, port)?;
    match TcpListener::bind((host, port)) {
        Ok(listener) => Ok(Value::socket(SocketState::Listener(listener))),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot listen on {host}:{port}: {err}"
        ))),
    }
}

/// `howl.connect(host, port)` — open a TCP connection to `host:port`. A refused
/// connection or unknown host is a catchable IOError.
pub fn howl_connect(host: &Value, port: &Value) -> DogeResult {
    let (host, port) = host_port("connect", host, port)?;
    match TcpStream::connect((host, port)) {
        Ok(stream) => Ok(Value::socket(SocketState::Conn {
            stream,
            buf: Vec::new(),
        })),
        Err(err) => Err(DogeError::io_error(format!(
            "cannot connect to {host}:{port}: {err}"
        ))),
    }
}

/// `howl.accept(listener)` — block until a client connects, then return the new
/// connection. A non-listener socket is a catchable TypeError; a closed one is an
/// IOError.
pub fn howl_accept(listener: &Value) -> DogeResult {
    let sock = socket_arg("accept", listener)?;
    let state = sock.state.borrow();
    match &*state {
        SocketState::Listener(l) => match l.accept() {
            Ok((stream, _)) => Ok(Value::socket(SocketState::Conn {
                stream,
                buf: Vec::new(),
            })),
            Err(err) => Err(DogeError::io_error(format!("cannot accept: {err}"))),
        },
        SocketState::Conn { .. } => Err(DogeError::type_error(
            "howl.accept needs a listening socket, not a connection",
        )),
        SocketState::Closed => Err(closed()),
    }
}

/// `howl.port(sock)` — the local port a listener or connection is bound to. A
/// closed socket is a catchable IOError.
pub fn howl_port(sock: &Value) -> DogeResult {
    let sock = socket_arg("port", sock)?;
    let state = sock.state.borrow();
    let addr = match &*state {
        SocketState::Listener(l) => l.local_addr(),
        SocketState::Conn { stream, .. } => stream.local_addr(),
        SocketState::Closed => return Err(closed()),
    };
    match addr {
        Ok(addr) => Ok(Value::int(addr.port())),
        Err(err) => Err(DogeError::io_error(format!("cannot read the port: {err}"))),
    }
}

/// `howl.send(conn, text)` — write `text` as UTF-8 to a connection. Returns
/// `none`. A broken pipe or a non-connection socket is a catchable error.
pub fn howl_send(conn: &Value, text: &Value) -> DogeResult {
    let sock = socket_arg("send", conn)?;
    let text = str_arg("howl", "send", text)?;
    let mut state = sock.state.borrow_mut();
    match &mut *state {
        SocketState::Conn { stream, .. } => match stream.write_all(text.as_bytes()) {
            Ok(()) => Ok(Value::None),
            Err(err) => Err(DogeError::io_error(format!("cannot send: {err}"))),
        },
        SocketState::Listener(_) => Err(DogeError::type_error(
            "howl.send needs a connection, not a listening socket",
        )),
        SocketState::Closed => Err(closed()),
    }
}

/// `howl.send_bytes(conn, bytes)` — write raw `bytes` to a connection, unchanged.
/// Returns `none`. The binary counterpart of [`howl_send`]: use it to send
/// arbitrary data (image, PDF, a framed HTTP body) that is not text. A broken pipe
/// or a non-connection socket is a catchable error.
pub fn howl_send_bytes(conn: &Value, bytes: &Value) -> DogeResult {
    let sock = socket_arg("send_bytes", conn)?;
    let bytes = bytes_arg("howl", "send_bytes", bytes)?;
    let mut state = sock.state.borrow_mut();
    match &mut *state {
        SocketState::Conn { stream, .. } => match stream.write_all(bytes) {
            Ok(()) => Ok(Value::None),
            Err(err) => Err(DogeError::io_error(format!("cannot send: {err}"))),
        },
        SocketState::Listener(_) => Err(DogeError::type_error(
            "howl.send_bytes needs a connection, not a listening socket",
        )),
        SocketState::Closed => Err(closed()),
    }
}

/// `howl.recv(conn, max_bytes)` — read up to `max_bytes` bytes from a connection
/// and return them as text, or `none` at end of input. Never splits a multi-byte
/// character: an incomplete trailing sequence is held for the next read, and a
/// call always yields at least one whole character (or `none`). `max_bytes` must
/// be a positive Int. Genuinely invalid bytes are a catchable IOError.
pub fn howl_recv(conn: &Value, max_bytes: &Value) -> DogeResult {
    let sock = socket_arg("recv", conn)?;
    let max = int_arg("howl", "recv", max_bytes)?;
    if max <= 0 {
        return Err(DogeError::value_error(format!(
            "howl.recv needs a positive byte count, got {max}"
        )));
    }
    let max = max as usize;
    let mut state = sock.state.borrow_mut();
    match &mut *state {
        SocketState::Conn { stream, buf } => {
            // Read until at least one whole character is buffered, or EOF. Only a
            // partial multi-byte sequence at the tail keeps the loop going, so it
            // runs at most a few times (a character is 4 bytes at most).
            loop {
                let valid_len = match std::str::from_utf8(buf) {
                    Ok(_) => buf.len(),
                    Err(e) => {
                        if e.error_len().is_some() {
                            return Err(DogeError::io_error("received bytes were not valid text"));
                        }
                        e.valid_up_to()
                    }
                };
                if valid_len > 0 {
                    let bytes: Vec<u8> = buf.drain(..valid_len).collect();
                    let text = String::from_utf8(bytes)
                        .expect("compiler bug: valid_up_to bytes are valid UTF-8");
                    return Ok(Value::str(text));
                }
                let mut chunk = vec![0u8; max];
                match stream.read(&mut chunk) {
                    Ok(0) => {
                        if buf.is_empty() {
                            return Ok(Value::None);
                        }
                        return Err(DogeError::io_error(
                            "connection closed in the middle of a character",
                        ));
                    }
                    Ok(got) => buf.extend_from_slice(&chunk[..got]),
                    Err(err) => return Err(DogeError::io_error(format!("cannot recv: {err}"))),
                }
            }
        }
        SocketState::Listener(_) => Err(DogeError::type_error(
            "howl.recv needs a connection, not a listening socket",
        )),
        SocketState::Closed => Err(closed()),
    }
}

/// `howl.recv_bytes(conn, max_bytes)` — read up to `max_bytes` raw bytes from a
/// connection and return them as `Bytes`, or `none` at end of input. The binary
/// counterpart of [`howl_recv`]: no UTF-8 reassembly, so bytes are returned exactly
/// as they arrive and non-text data is never an error — the way to read a binary or
/// byte-framed body. `max_bytes` must be a positive Int. Bytes buffered by an
/// earlier `recv`/`recv_line` are returned first so a mixed-use socket loses
/// nothing. A broken connection is a catchable IOError.
pub fn howl_recv_bytes(conn: &Value, max_bytes: &Value) -> DogeResult {
    let sock = socket_arg("recv_bytes", conn)?;
    let max = int_arg("howl", "recv_bytes", max_bytes)?;
    if max <= 0 {
        return Err(DogeError::value_error(format!(
            "howl.recv_bytes needs a positive byte count, got {max}"
        )));
    }
    let max = max as usize;
    let mut state = sock.state.borrow_mut();
    match &mut *state {
        SocketState::Conn { stream, buf } => {
            if !buf.is_empty() {
                let take = buf.len().min(max);
                let bytes: Vec<u8> = buf.drain(..take).collect();
                return Ok(Value::bytes(bytes));
            }
            let mut chunk = vec![0u8; max];
            match stream.read(&mut chunk) {
                Ok(0) => Ok(Value::None),
                Ok(got) => Ok(Value::bytes(&chunk[..got])),
                Err(err) => Err(DogeError::io_error(format!("cannot recv: {err}"))),
            }
        }
        SocketState::Listener(_) => Err(DogeError::type_error(
            "howl.recv_bytes needs a connection, not a listening socket",
        )),
        SocketState::Closed => Err(closed()),
    }
}

/// `howl.recv_line(conn)` — read one line from a connection, without the trailing
/// newline (a `\r\n` is trimmed too), or `none` at end of input. Invalid bytes
/// are a catchable IOError.
pub fn howl_recv_line(conn: &Value) -> DogeResult {
    let sock = socket_arg("recv_line", conn)?;
    let mut state = sock.state.borrow_mut();
    match &mut *state {
        SocketState::Conn { stream, buf } => loop {
            if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = buf.drain(..=pos).collect();
                line.pop(); // the '\n'
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return line_to_value(line);
            }
            let mut chunk = [0u8; LINE_CHUNK];
            match stream.read(&mut chunk) {
                Ok(0) => {
                    if buf.is_empty() {
                        return Ok(Value::None);
                    }
                    return line_to_value(std::mem::take(buf));
                }
                Ok(got) => buf.extend_from_slice(&chunk[..got]),
                Err(err) => return Err(DogeError::io_error(format!("cannot recv: {err}"))),
            }
        },
        SocketState::Listener(_) => Err(DogeError::type_error(
            "howl.recv_line needs a connection, not a listening socket",
        )),
        SocketState::Closed => Err(closed()),
    }
}

/// `howl.close(sock)` — close a listener or connection now. Idempotent: closing an
/// already-closed socket is fine. Returns `none`. Any later operation on it is a
/// catchable IOError.
pub fn howl_close(sock: &Value) -> DogeResult {
    let sock = socket_arg("close", sock)?;
    *sock.state.borrow_mut() = SocketState::Closed;
    Ok(Value::None)
}

/// `howl.get(url)` — HTTP(S) GET. Returns a Dict `{"status": Int, "body": Str}`.
/// A non-2xx response is returned like any other (its status and body); only a
/// transport, TLS, or timeout failure is a catchable IOError.
pub fn howl_get(url: &Value) -> DogeResult {
    let url = str_arg("howl", "get", url)?;
    request_result(url, agent().get(url).call())
}

/// `howl.post(url, body)` — HTTP(S) POST of `body` as `text/plain; charset=utf-8`.
/// Same return shape and error rule as [`howl_get`].
pub fn howl_post(url: &Value, body: &Value) -> DogeResult {
    let url = str_arg("howl", "post", url)?;
    let body = str_arg("howl", "post", body)?;
    request_result(
        url,
        agent()
            .post(url)
            .set("Content-Type", "text/plain; charset=utf-8")
            .send_string(body),
    )
}

/// `howl.request(method, url, opts)` — the general HTTP(S) client. `method` is one
/// of [`HTTP_METHODS`] (case-insensitive). `opts` is a Dict (or `none` for no
/// options) with optional keys `"headers"` (a Dict of Str→Str) and `"body"` (a Str,
/// sent UTF-8, or Bytes, sent raw); any other key is a ValueError. Same return
/// shape and transport-error rule as [`howl_get`], with response headers included.
pub fn howl_request(method: &Value, url: &Value, opts: &Value) -> DogeResult {
    let method = str_arg("howl", "request", method)?.to_ascii_uppercase();
    if !HTTP_METHODS.contains(&method.as_str()) {
        return Err(DogeError::value_error(format!(
            "unknown HTTP method {method:?}, expected one of {}",
            HTTP_METHODS.join(", ")
        )));
    }
    let url = str_arg("howl", "request", url)?;

    let opts = opts_dict(opts)?;
    let mut request = agent().request(&method, url);
    let mut body: Option<Value> = None;
    if let Some(opts) = &opts {
        let opts = opts.borrow();
        for (key, value) in opts.iter() {
            match key.as_str() {
                "headers" => {
                    for (name, header) in headers_dict(value)?.borrow().iter() {
                        let header = str_arg("howl", "request", header).map_err(|_| {
                            DogeError::type_error(format!(
                                "howl.request header {name:?} needs a Str value, got {}",
                                header.describe()
                            ))
                        })?;
                        request = request.set(name, header);
                    }
                }
                "body" => body = Some(value.clone()),
                other => {
                    return Err(DogeError::value_error(format!(
                        "howl.request got an unknown option {other:?}, expected \"headers\" or \"body\""
                    )));
                }
            }
        }
    }

    let sent = match &body {
        None | Some(Value::None) => request.call(),
        Some(Value::Str(text)) => request.send_string(text),
        Some(Value::Bytes(bytes)) => request.send_bytes(bytes),
        Some(other) => {
            return Err(DogeError::type_error(format!(
                "howl.request body needs a Str or Bytes, got {}",
                other.describe()
            )));
        }
    };
    request_result(url, sent)
}

/// The `opts` argument as a borrowable Dict, or `None` when options are omitted
/// (`none`). Any other type is a catchable TypeError.
fn opts_dict(opts: &Value) -> DogeResult<Option<Rc<std::cell::RefCell<OrderedMap>>>> {
    match opts {
        Value::None => Ok(None),
        Value::Dict(entries) => Ok(Some(Rc::clone(entries))),
        _ => Err(DogeError::type_error(format!(
            "howl.request options need a Dict, got {}",
            opts.describe()
        ))),
    }
}

/// The `"headers"` option as a borrowable Dict, or a catchable TypeError.
fn headers_dict(value: &Value) -> DogeResult<&Rc<std::cell::RefCell<OrderedMap>>> {
    match value {
        Value::Dict(entries) => Ok(entries),
        _ => Err(DogeError::type_error(format!(
            "howl.request headers need a Dict, got {}",
            value.describe()
        ))),
    }
}

/// The shared HTTP agent: rustls TLS with a fixed request timeout.
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build()
}

/// Turn a ureq call result into the `{"status", "body"}` Dict. A non-2xx status
/// (`Error::Status`) is a normal result, not an error; every other ureq error is a
/// catchable IOError worded without any Rust type names leaking through.
fn request_result(url: &str, result: Result<ureq::Response, ureq::Error>) -> DogeResult {
    match result {
        Ok(response) => response_dict(response),
        Err(ureq::Error::Status(_, response)) => response_dict(response),
        Err(ureq::Error::Transport(transport)) => Err(DogeError::io_error(format!(
            "cannot fetch {url}: {}",
            transport.message().unwrap_or("the request failed")
        ))),
    }
}

/// Build the response Dict `{"status", "body", "headers"}`, reading the body as
/// text. Header names are lowercased so a script reads them case-insensitively. A
/// body that is not valid text is a catchable IOError.
fn response_dict(response: ureq::Response) -> DogeResult {
    let status = response.status() as i64;
    let mut headers = OrderedMap::new();
    for name in response.headers_names() {
        let value = response.header(&name).unwrap_or("");
        headers.insert(name.to_ascii_lowercase(), Value::str(value));
    }
    let body = response
        .into_string()
        .map_err(|err| DogeError::io_error(format!("cannot read the response: {err}")))?;
    let mut entries = OrderedMap::new();
    entries.insert("status".to_string(), Value::int(status));
    entries.insert("body".to_string(), Value::str(body));
    entries.insert("headers".to_string(), Value::dict(headers));
    Ok(Value::dict(entries))
}

/// The error every operation on a closed socket raises.
fn closed() -> DogeError {
    DogeError::io_error("socket is closed")
}

/// A received line's bytes as a Str value, or a catchable IOError when they are
/// not valid text.
fn line_to_value(bytes: Vec<u8>) -> DogeResult {
    String::from_utf8(bytes)
        .map(Value::str)
        .map_err(|_| DogeError::io_error("received bytes were not valid text"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use bigdecimal::ToPrimitive;
    use std::thread;

    /// A listener bound to an OS-assigned loopback port, plus that port.
    fn loopback() -> (Value, u16) {
        let listener = howl_listen(&Value::str("127.0.0.1"), &Value::int(0)).unwrap();
        let port = match howl_port(&listener).unwrap() {
            Value::Int(p) => p.to_u16().unwrap(),
            _ => panic!("port is an Int"),
        };
        (listener, port)
    }

    fn recv_str(conn: &Value, n: i64) -> Option<String> {
        match howl_recv(conn, &Value::int(n)).unwrap() {
            Value::Str(s) => Some(s.to_string()),
            Value::None => None,
            other => panic!("recv gave {}", other.type_name()),
        }
    }

    fn recv_bytes(conn: &Value, n: i64) -> Option<Vec<u8>> {
        match howl_recv_bytes(conn, &Value::int(n)).unwrap() {
            Value::Bytes(b) => Some(b.to_vec()),
            Value::None => None,
            other => panic!("recv_bytes gave {}", other.type_name()),
        }
    }

    #[test]
    fn tcp_round_trip_and_close() {
        let (listener, port) = loopback();
        // connect() succeeds into the backlog before accept(), so one thread can
        // drive both ends.
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        let server = howl_accept(&listener).unwrap();

        howl_send(&client, &Value::str("much hello\n")).unwrap();
        assert_eq!(howl_recv_line(&server).unwrap().to_string(), "much hello");

        howl_send(&server, &Value::str("wow\n")).unwrap();
        assert_eq!(howl_recv_line(&client).unwrap().to_string(), "wow");

        // After the server closes, the client reads end-of-input.
        howl_close(&server).unwrap();
        assert!(matches!(howl_recv_line(&client).unwrap(), Value::None));

        // Every op on a closed socket is a catchable IOError.
        assert_eq!(
            howl_send(&server, &Value::str("x")).unwrap_err().kind,
            ErrorKind::IOError
        );
        // close is idempotent.
        howl_close(&server).unwrap();
    }

    #[test]
    fn recv_reassembles_a_split_multibyte_character() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        let server = howl_accept(&listener).unwrap();

        // "é" is two UTF-8 bytes; reading one byte at a time must still yield the
        // whole character, never a split.
        howl_send(&client, &Value::str("é")).unwrap();
        assert_eq!(recv_str(&server, 1).as_deref(), Some("é"));
    }

    #[test]
    fn recv_reports_eof_as_none() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        let server = howl_accept(&listener).unwrap();
        howl_close(&client).unwrap();
        assert_eq!(recv_str(&server, 16), None);
    }

    #[test]
    fn bytes_round_trip_preserves_non_text_data() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        let server = howl_accept(&listener).unwrap();

        // Bytes that are not valid UTF-8 (0xff, 0x00) survive send/recv untouched,
        // where recv would raise an IOError.
        let payload = vec![0xffu8, 0x00, 0x50, 0x44, 0x46];
        howl_send_bytes(&client, &Value::bytes(&payload)).unwrap();
        assert_eq!(recv_bytes(&server, 16).as_deref(), Some(&payload[..]));
    }

    #[test]
    fn recv_bytes_reports_eof_as_none() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        let server = howl_accept(&listener).unwrap();
        howl_close(&client).unwrap();
        assert_eq!(recv_bytes(&server, 16), None);
    }

    #[test]
    fn recv_bytes_drains_buffer_left_by_recv_line() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        let server = howl_accept(&listener).unwrap();

        // A header line then a byte body arriving in one write: recv_line consumes
        // the line and over-reads the body into the buffer, which recv_bytes must
        // hand back rather than drop.
        howl_send(&client, &Value::str("Head: v\r\nBODY")).unwrap();
        assert_eq!(howl_recv_line(&server).unwrap().to_string(), "Head: v");
        assert_eq!(recv_bytes(&server, 4).as_deref(), Some(&b"BODY"[..]));
    }

    #[test]
    fn recv_bytes_and_send_bytes_reject_bad_args() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        // send_bytes on a listener is a TypeError; a non-Bytes payload too.
        assert_eq!(
            howl_send_bytes(&listener, &Value::bytes(b"x"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            howl_send_bytes(&client, &Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        // A non-positive recv_bytes size is a ValueError.
        assert_eq!(
            howl_recv_bytes(&client, &Value::int(0)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn wrong_socket_role_and_types_are_catchable() {
        let (listener, port) = loopback();
        let client = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap();
        // accept on a connection, send on a listener: both TypeErrors.
        assert_eq!(howl_accept(&client).unwrap_err().kind, ErrorKind::TypeError);
        assert_eq!(
            howl_send(&listener, &Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        // Non-Str host, non-Socket receiver, zero recv size.
        assert_eq!(
            howl_connect(&Value::int(1), &Value::int(port as i64))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            howl_send(&Value::int(1), &Value::str("x"))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            howl_recv(&client, &Value::int(0)).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            howl_listen(&Value::str("127.0.0.1"), &Value::int(99999))
                .unwrap_err()
                .kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn connection_refused_is_a_catchable_io_error() {
        // Bind, read the port, then drop the listener so nothing is listening.
        let port = {
            let (listener, port) = loopback();
            howl_close(&listener).unwrap();
            port
        };
        let err = howl_connect(&Value::str("127.0.0.1"), &Value::int(port as i64)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::IOError);
    }

    #[test]
    fn http_get_returns_status_and_body() {
        // A one-shot HTTP/1.1 server on loopback, so the test never touches the
        // network. It replies 404 to prove a non-2xx status is a normal result.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let body = "much not found";
                let response = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let url = Value::str(format!("http://127.0.0.1:{port}/"));
        let result = howl_get(&url).unwrap();
        handle.join().unwrap();

        match result {
            Value::Dict(entries) => {
                let entries = entries.borrow();
                assert!(entries
                    .get("status")
                    .is_some_and(|v| crate::values_equal(v, &Value::int(404))));
                assert!(
                    matches!(entries.get("body"), Some(Value::Str(s)) if &**s == "much not found")
                );
            }
            other => panic!("expected a Dict, got {}", other.type_name()),
        }
    }

    #[test]
    fn http_get_on_a_dead_port_is_a_catchable_io_error() {
        // Bind to grab a free port, then drop the listener so nothing answers.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let err = howl_get(&Value::str(format!("http://127.0.0.1:{port}/"))).unwrap_err();
        assert_eq!(err.kind, ErrorKind::IOError);
    }

    /// A one-shot HTTP/1.1 loopback server that captures the whole request (headers
    /// plus any `Content-Length` body) and replies `200 OK` with a JSON content
    /// type. Returns the listener's port and a handle yielding the raw request text.
    fn capture_server() -> (u16, thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            loop {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if let Some(head_end) = find_subslice(&request, b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..head_end]).to_lowercase();
                    let content_length = head
                        .split("\r\n")
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= head_end + 4 + content_length {
                        break;
                    }
                }
            }
            let body = "wow ok";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            String::from_utf8_lossy(&request).into_owned()
        });
        (port, handle)
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    /// A Dict `Value` from string keys and values, for building `request` options.
    fn dict_of(pairs: &[(&str, Value)]) -> Value {
        let mut entries = OrderedMap::new();
        for (key, value) in pairs {
            entries.insert((*key).to_string(), value.clone());
        }
        Value::dict(entries)
    }

    #[test]
    fn http_request_sends_method_headers_and_body() {
        let (port, handle) = capture_server();
        let url = Value::str(format!("http://127.0.0.1:{port}/invoices"));
        let opts = dict_of(&[
            (
                "headers",
                dict_of(&[
                    ("Authorization", Value::str("Bearer secret")),
                    ("Content-Type", Value::str("application/json")),
                ]),
            ),
            ("body", Value::str("{\"much\":\"json\"}")),
        ]);
        let result = howl_request(&Value::str("post"), &url, &opts).unwrap();
        let request = handle.join().unwrap();

        assert!(request.starts_with("POST /invoices HTTP/1.1\r\n"));
        assert!(request.contains("Authorization: Bearer secret\r\n"));
        assert!(request.contains("Content-Type: application/json\r\n"));
        assert!(request.ends_with("{\"much\":\"json\"}"));

        match result {
            Value::Dict(entries) => {
                let entries = entries.borrow();
                assert!(entries
                    .get("status")
                    .is_some_and(|v| crate::values_equal(v, &Value::int(200))));
                let headers = match entries.get("headers") {
                    Some(Value::Dict(h)) => h.borrow(),
                    other => panic!("expected a headers Dict, got {other:?}"),
                };
                assert!(matches!(
                    headers.get("content-type"),
                    Some(Value::Str(s)) if &**s == "application/json"
                ));
            }
            other => panic!("expected a Dict, got {}", other.type_name()),
        }
    }

    #[test]
    fn http_request_sends_a_bytes_body_raw() {
        let (port, handle) = capture_server();
        let url = Value::str(format!("http://127.0.0.1:{port}/"));
        let opts = dict_of(&[("body", Value::bytes([0x50, 0x44, 0x46]))]);
        howl_request(&Value::str("PUT"), &url, &opts).unwrap();
        let request = handle.join().unwrap();
        assert!(request.starts_with("PUT / HTTP/1.1\r\n"));
        assert!(request.ends_with("PDF"));
    }

    #[test]
    fn http_request_without_options_is_a_bare_request() {
        let (port, handle) = capture_server();
        let url = Value::str(format!("http://127.0.0.1:{port}/"));
        // An omitted `opts` reaches the runtime as `none`.
        let result = howl_request(&Value::str("GET"), &url, &Value::None).unwrap();
        let request = handle.join().unwrap();
        assert!(request.starts_with("GET / HTTP/1.1\r\n"));
        assert!(matches!(result, Value::Dict(_)));
    }

    #[test]
    fn http_request_rejects_an_unknown_method() {
        let err =
            howl_request(&Value::str("FETCH"), &Value::str("http://x/"), &Value::None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ValueError);
    }

    #[test]
    fn http_request_rejects_an_unknown_option() {
        let opts = dict_of(&[("query", Value::str("nope"))]);
        let err = howl_request(&Value::str("GET"), &Value::str("http://x/"), &opts).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ValueError);
    }

    #[test]
    fn http_request_rejects_a_non_text_body() {
        let opts = dict_of(&[("body", Value::int(42))]);
        let err = howl_request(&Value::str("POST"), &Value::str("http://x/"), &opts).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    #[test]
    fn http_request_rejects_a_non_str_header_value() {
        let opts = dict_of(&[("headers", dict_of(&[("X-Count", Value::int(3))]))]);
        let err = howl_request(&Value::str("GET"), &Value::str("http://x/"), &opts).unwrap_err();
        assert_eq!(err.kind, ErrorKind::TypeError);
    }

    #[test]
    fn http_get_response_includes_headers() {
        let (port, handle) = capture_server();
        let result = howl_get(&Value::str(format!("http://127.0.0.1:{port}/"))).unwrap();
        handle.join().unwrap();
        match result {
            Value::Dict(entries) => {
                let entries = entries.borrow();
                let headers = match entries.get("headers") {
                    Some(Value::Dict(h)) => h.borrow(),
                    other => panic!("expected a headers Dict, got {other:?}"),
                };
                assert!(headers.get("content-type").is_some());
            }
            other => panic!("expected a Dict, got {}", other.type_name()),
        }
    }
}
