//! Minimal, dependency-free Minecraft networking helpers.
//!
//! Right now this implements the Java Edition "Server List Ping"
//! (handshake + status request), which is what the server list screen uses.
//! Bedrock (RakNet over UDP) is a placeholder and returns `NotImplemented`.

use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

/// A Java Edition version and the protocol number servers expect for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JavaVersion {
    pub name: &'static str,
    pub protocol: i32,
}

/// Versions offered in the edition picker (newest first).
pub const JAVA_VERSIONS: &[JavaVersion] = &[
    JavaVersion { name: "26.1.2", protocol: 775 },
    JavaVersion { name: "1.21.4", protocol: 769 },
    JavaVersion { name: "1.21.1", protocol: 767 },
    JavaVersion { name: "1.20.4", protocol: 765 },
    JavaVersion { name: "1.20.1", protocol: 763 },
    JavaVersion { name: "1.19.4", protocol: 762 },
    JavaVersion { name: "1.16.5", protocol: 754 },
    JavaVersion { name: "1.12.2", protocol: 340 },
    JavaVersion { name: "1.8.9", protocol: 47 },
];

/// Which game edition (and for Java, which version) to talk to a server as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edition {
    Java(JavaVersion),
    Bedrock,
}

#[derive(Debug)]
pub enum NetError {
    Io(io::Error),
    Protocol(String),
    NotImplemented(&'static str),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Io(e) => write!(f, "network error: {}", e),
            NetError::Protocol(s) => write!(f, "protocol error: {}", s),
            NetError::NotImplemented(s) => write!(f, "not implemented yet: {}", s),
        }
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Wire primitives
// ---------------------------------------------------------------------------

pub fn write_varint(buf: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    loop {
        if v & !0x7F == 0 {
            buf.push(v as u8);
            return;
        }
        buf.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
}

pub fn read_varint<R: Read>(r: &mut R) -> io::Result<i32> {
    let mut result: u32 = 0;
    for i in 0..5 {
        let mut b = [0u8; 1];
        r.read_exact(&mut b)?;
        result |= ((b[0] & 0x7F) as u32) << (7 * i);
        if b[0] & 0x80 == 0 {
            return Ok(result as i32);
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "VarInt is too long"))
}

pub fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

/// Builds a length-prefixed Handshake packet (`next_state` 1 = status, 2 = login).
pub fn handshake_packet(protocol: i32, host: &str, port: u16, next_state: i32) -> Vec<u8> {
    let mut body = Vec::new();
    write_varint(&mut body, 0x00);
    write_varint(&mut body, protocol);
    write_string(&mut body, host);
    body.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut body, next_state);

    let mut out = Vec::new();
    write_varint(&mut out, body.len() as i32);
    out.extend_from_slice(&body);
    out
}

// ---------------------------------------------------------------------------
// Server status
// ---------------------------------------------------------------------------

/// The raw status JSON from a server plus a few best-effort accessors.
///
/// The accessors use simple text scanning, not a full JSON parser; they are
/// good enough for the server list and will be replaced when we add serde.
#[derive(Clone, Debug)]
pub struct StatusInfo {
    pub raw_json: String,
}

impl StatusInfo {
    pub fn players_online(&self) -> Option<i64> {
        parse_int_after(&self.raw_json, "\"online\":")
    }

    pub fn players_max(&self) -> Option<i64> {
        parse_int_after(&self.raw_json, "\"max\":")
    }

    pub fn version_name(&self) -> Option<String> {
        parse_string_after(&self.raw_json, "\"version\":{\"name\":\"")
            .or_else(|| parse_string_after(&self.raw_json, "\"name\":\""))
    }

    /// The server's message of the day with colour codes removed.
    pub fn motd(&self) -> Option<String> {
        let idx = self.raw_json.find("\"description\":")?;
        let rest = &self.raw_json[idx + "\"description\":".len()..];
        let text = if rest.trim_start().starts_with('"') {
            parse_string_after(rest.trim_start(), "\"")?
        } else {
            collect_texts(rest)
        };
        let cleaned = strip_color_codes(&text);
        let cleaned = cleaned.trim().to_string();
        if cleaned.is_empty() {
            None
        } else {
            Some(cleaned)
        }
    }
}

fn find_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    s.find(key).map(|i| &s[i + key.len()..])
}

fn parse_int_after(s: &str, key: &str) -> Option<i64> {
    let rest = find_after(s, key)?.trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Reads a JSON string that starts right after `key` (which must end with the
/// opening quote) and returns it unescaped.
fn parse_string_after(s: &str, key: &str) -> Option<String> {
    let rest = find_after(s, key)?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(ch) = u32::from_str_radix(&hex, 16)
                        .ok()
                        .and_then(std::char::from_u32)
                    {
                        out.push(ch);
                    }
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// Joins every `"text":"..."` value found in a chat-component JSON fragment.
fn collect_texts(s: &str) -> String {
    let key = "\"text\":\"";
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(key) {
        if let Some(t) = parse_string_after(&rest[i..], key) {
            out.push_str(&t);
        }
        rest = &rest[i + key.len()..];
    }
    out
}

fn strip_color_codes(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{00A7}' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Pinging
// ---------------------------------------------------------------------------

fn split_host_port(address: &str, default_port: u16) -> (String, u16) {
    if let Some((h, p)) = address.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return (h.to_string(), port);
        }
    }
    (address.to_string(), default_port)
}

/// Asks a server for its status. Blocking: call it from a background thread.
pub fn ping_server(address: &str, edition: Edition) -> Result<StatusInfo, NetError> {
    match edition {
        Edition::Java(v) => ping_java(address, v),
        Edition::Bedrock => Err(NetError::NotImplemented("Bedrock server ping (RakNet)")),
    }
}

fn ping_java(address: &str, version: JavaVersion) -> Result<StatusInfo, NetError> {
    let (host, port) = split_host_port(address, 25565);
    let addr = (host.as_str(), port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| NetError::Protocol("could not resolve host".to_string()))?;

    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;

    stream.write_all(&handshake_packet(version.protocol, &host, port, 1))?;
    stream.write_all(&[0x01, 0x00])?; // Status Request: length 1, id 0
    read_status_response(&mut stream)
}

fn read_status_response<R: Read>(r: &mut R) -> Result<StatusInfo, NetError> {
    let len = read_varint(r)?;
    if len <= 0 || len > (1 << 22) {
        return Err(NetError::Protocol(format!("bad packet length {}", len)));
    }
    let mut data = vec![0u8; len as usize];
    r.read_exact(&mut data)?;

    let mut cur = io::Cursor::new(data);
    let id = read_varint(&mut cur)?;
    if id != 0 {
        return Err(NetError::Protocol(format!("unexpected packet id {}", id)));
    }
    let slen = read_varint(&mut cur)?;
    let remaining = cur.get_ref().len() as u64 - cur.position();
    if slen < 0 || slen as u64 > remaining {
        return Err(NetError::Protocol("bad status string length".to_string()));
    }
    let mut buf = vec![0u8; slen as usize];
    cur.read_exact(&mut buf)?;
    let raw_json = String::from_utf8(buf)
        .map_err(|_| NetError::Protocol("status is not valid UTF-8".to_string()))?;
    Ok(StatusInfo { raw_json })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn varint_known_values() {
        let mut b = Vec::new();
        write_varint(&mut b, 0);
        assert_eq!(b, [0x00]);

        b.clear();
        write_varint(&mut b, 127);
        assert_eq!(b, [0x7F]);

        b.clear();
        write_varint(&mut b, 128);
        assert_eq!(b, [0x80, 0x01]);

        b.clear();
        write_varint(&mut b, -1);
        assert_eq!(b, [0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    }

    #[test]
    fn varint_roundtrip() {
        for v in [0, 1, 47, 127, 128, 255, 769, 25565, 2_097_151, i32::MAX, -1, i32::MIN] {
            let mut b = Vec::new();
            write_varint(&mut b, v);
            let got = read_varint(&mut io::Cursor::new(b)).unwrap();
            assert_eq!(got, v);
        }
    }

    #[test]
    fn varint_too_long_is_error() {
        let bytes = [0x80u8; 6];
        assert!(read_varint(&mut io::Cursor::new(&bytes[..])).is_err());
    }

    #[test]
    fn handshake_bytes() {
        let p = handshake_packet(769, "localhost", 25565, 1);
        let mut expected = vec![0x10, 0x00, 0x81, 0x06, 0x09];
        expected.extend_from_slice(b"localhost");
        expected.extend_from_slice(&[0x63, 0xDD, 0x01]);
        assert_eq!(p, expected);
    }

    #[test]
    fn host_port_split() {
        assert_eq!(split_host_port("mc.example.org", 25565), ("mc.example.org".to_string(), 25565));
        assert_eq!(split_host_port("mc.example.org:25570", 25565), ("mc.example.org".to_string(), 25570));
        assert_eq!(split_host_port("host:notaport", 1), ("host:notaport".to_string(), 1));
    }

    #[test]
    fn status_accessors() {
        let json = r#"{"version":{"name":"1.21.4","protocol":769},"players":{"max":20,"online":3},"description":{"text":"§aHello §lworld"}}"#;
        let info = StatusInfo { raw_json: json.to_string() };
        assert_eq!(info.version_name().as_deref(), Some("1.21.4"));
        assert_eq!(info.players_online(), Some(3));
        assert_eq!(info.players_max(), Some(20));
        assert_eq!(info.motd().as_deref(), Some("Hello world"));
    }

    #[test]
    fn motd_plain_string_and_components() {
        let a = StatusInfo { raw_json: r#"{"description":"Plain \"quoted\" motd"}"#.to_string() };
        assert_eq!(a.motd().as_deref(), Some("Plain \"quoted\" motd"));

        let b = StatusInfo {
            raw_json: r#"{"description":{"extra":[{"text":"Hello "},{"text":"there"}],"text":""}}"#.to_string(),
        };
        assert_eq!(b.motd().as_deref(), Some("Hello there"));
    }

    #[test]
    fn bedrock_is_not_implemented_yet() {
        match ping_server("127.0.0.1:19132", Edition::Bedrock) {
            Err(NetError::NotImplemented(_)) => {}
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn ping_against_fake_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();

            let len = read_varint(&mut s).unwrap();
            let mut body = vec![0u8; len as usize];
            s.read_exact(&mut body).unwrap();
            let mut cur = io::Cursor::new(&body);
            assert_eq!(read_varint(&mut cur).unwrap(), 0); // handshake id
            assert_eq!(read_varint(&mut cur).unwrap(), 769); // protocol

            let mut req = [0u8; 2];
            s.read_exact(&mut req).unwrap();
            assert_eq!(req, [0x01, 0x00]);

            let json = r#"{"version":{"name":"1.21.4","protocol":769},"players":{"max":20,"online":3},"description":"Fake server"}"#;
            let mut payload = Vec::new();
            write_varint(&mut payload, 0);
            write_string(&mut payload, json);
            let mut out = Vec::new();
            write_varint(&mut out, payload.len() as i32);
            out.extend_from_slice(&payload);
            s.write_all(&out).unwrap();
        });

        let v = *JAVA_VERSIONS.iter().find(|v| v.name == "1.21.4").unwrap();
        let info = ping_server(&format!("127.0.0.1:{}", port), Edition::Java(v)).unwrap();
        assert_eq!(info.players_online(), Some(3));
        assert_eq!(info.players_max(), Some(20));
        assert_eq!(info.version_name().as_deref(), Some("1.21.4"));
        assert_eq!(info.motd().as_deref(), Some("Fake server"));
        server.join().unwrap();
    }
}
