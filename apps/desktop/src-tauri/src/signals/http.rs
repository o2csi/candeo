//! The signals API: `POST /signals` and `GET /signals`, behind a token (§2.3).
//!
//! [`handle`] decides every answer and is pure — a request's parts in, a status
//! and a body out — so authentication, routes and bounds are unit tests. The
//! server around it only reads a socket and writes the answer back.

use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::{json, Value};

use super::store::{parse_ttl, Store};

/// A body larger than this is refused unread: 64 signals of 64-character names
/// and 256-character values fit in about 21 KB of JSON.
pub const MAX_BODY: usize = 32 * 1024;

/// What [`handle`] reads of a request.
pub struct Request<'a> {
    pub method: &'a str,
    /// The path and the query string, as sent: `/signals?ttl=0`.
    pub url: &'a str,
    pub authorization: Option<&'a str>,
    /// Whether the request carries an `Origin` header: a browser's does.
    pub from_browser: bool,
    pub body: &'a [u8],
}

/// An answer: its status and its JSON body.
#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub body: Value,
}

impl Response {
    fn error(status: u16, message: impl std::fmt::Display) -> Self {
        Self {
            status,
            body: json!({ "error": message.to_string() }),
        }
    }
}

/// The answer to `request`, and whether it changed what is held.
pub fn handle(request: &Request, token: &str, store: &mut Store, now: i64) -> (Response, bool) {
    // A web page can send a request to any port of this machine; a browser says
    // where it comes from, and no sender this API is for does. Refused before
    // the token is even looked at, and no CORS preflight is ever answered.
    if request.from_browser {
        return (
            Response::error(403, "requests from a web page are refused"),
            false,
        );
    }
    if !authorized(request.authorization, token) {
        return (
            Response::error(401, "missing or wrong token: Authorization: Bearer <token>"),
            false,
        );
    }

    let (path, query) = request.url.split_once('?').unwrap_or((request.url, ""));
    if path.trim_end_matches('/') != "/signals" {
        return (Response::error(404, "the only path is /signals"), false);
    }

    match request.method {
        "GET" => {
            let changed = store.prune(now);
            let signals: Vec<Value> = store
                .views(now)
                .into_iter()
                .map(|view| {
                    json!({
                        "name": view.name,
                        "value": view.value,
                        "expiresIn": view.expires.map(|at| seconds_left(at, now)),
                    })
                })
                .collect();
            (
                Response {
                    status: 200,
                    body: json!({ "signals": signals }),
                },
                changed,
            )
        }
        "POST" => {
            let ttl = match parse_ttl(query_value(query, "ttl")) {
                Ok(ttl) => ttl,
                Err(refusal) => return (Response::error(400, refusal), false),
            };
            let body: Value = match serde_json::from_slice(request.body) {
                Ok(body) => body,
                Err(_) => return (Response::error(400, "the body is not JSON"), false),
            };
            match store.apply(&body, ttl, now) {
                Ok(change) => (
                    Response {
                        status: 200,
                        body: json!({
                            "accepted": change.set,
                            "erased": change.erased,
                            "expiresIn": change.expires.map(|at| seconds_left(at, now)),
                        }),
                    },
                    true,
                ),
                Err(refusal) => (Response::error(400, refusal), false),
            }
        }
        _ => (Response::error(405, "use GET or POST"), false),
    }
}

/// Whether the `Authorization` header carries the token, compared in constant
/// time: how long a refusal takes says nothing of how much of it was right.
fn authorized(header: Option<&str>, token: &str) -> bool {
    let Some(sent) = header.and_then(|h| h.strip_prefix("Bearer ")) else {
        return false;
    };
    if token.is_empty() || sent.len() != token.len() {
        return false;
    }
    sent.bytes()
        .zip(token.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn query_value<'q>(query: &'q str, key: &str) -> Option<&'q str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

fn seconds_left(at: i64, now: i64) -> i64 {
    ((at - now).max(0) + 999) / 1000
}

/// A server listening on one address, and the thread serving it.
pub struct Listener {
    pub addr: SocketAddr,
    server: Arc<tiny_http::Server>,
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// Listens on `addr`, and answers each request with `serve`.
    pub fn start(
        addr: SocketAddr,
        serve: impl Fn(&Request) -> Response + Send + 'static,
    ) -> Result<Self, String> {
        let server = Arc::new(tiny_http::Server::http(addr).map_err(|e| e.to_string())?);
        let serving = Arc::clone(&server);
        let thread = std::thread::Builder::new()
            .name(format!("candeo-signals-{addr}"))
            .spawn(move || {
                for request in serving.incoming_requests() {
                    answer(request, &serve);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            addr,
            server,
            thread: Some(thread),
        })
    }
}

impl Drop for Listener {
    /// Stops listening, and waits for the request under way to be answered.
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn answer(mut request: tiny_http::Request, serve: &impl Fn(&Request) -> Response) {
    let header = |name: &'static str| {
        request
            .headers()
            .iter()
            .find(|h| h.field.equiv(name))
            .map(|h| h.value.as_str().to_string())
    };
    let authorization = header("Authorization");
    let from_browser = header("Origin").is_some();
    let method = request.method().as_str().to_string();
    let url = request.url().to_string();

    let mut body = Vec::new();
    let read = request
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut body);
    let response = match read {
        Err(_) => Response::error(400, "the body could not be read"),
        Ok(_) if body.len() > MAX_BODY => Response::error(413, "the body is too large"),
        Ok(_) => serve(&Request {
            method: &method,
            url: &url,
            authorization: authorization.as_deref(),
            from_browser,
            body: &body,
        }),
    };

    let json =
        tiny_http::Header::from_bytes("Content-Type", "application/json").expect("a static header");
    let reply = tiny_http::Response::from_string(response.body.to_string())
        .with_status_code(response.status)
        .with_header(json);
    // A client gone before the answer is nobody's problem to report.
    let _ = request.respond(reply);
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef";
    const NOW: i64 = 1_790_000_000_000;

    fn request<'a>(method: &'a str, url: &'a str, body: &'a str) -> Request<'a> {
        Request {
            method,
            url,
            authorization: Some("Bearer 0123456789abcdef"),
            from_browser: false,
            body: body.as_bytes(),
        }
    }

    #[test]
    fn a_post_sets_values_and_says_when_they_expire() {
        let mut store = Store::default();
        let (response, changed) = handle(
            &request("POST", "/signals", r#"{"build":"failed"}"#),
            TOKEN,
            &mut store,
            NOW,
        );
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body,
            json!({ "accepted": ["build"], "erased": [], "expiresIn": 60 })
        );
        assert!(changed);
        assert!(store.held().contains_key("build"));
    }

    #[test]
    fn ttl_zero_holds_until_erased() {
        let mut store = Store::default();
        let (response, _) = handle(
            &request("POST", "/signals?ttl=0", r#"{"build":"failed"}"#),
            TOKEN,
            &mut store,
            NOW,
        );
        assert_eq!(response.body["expiresIn"], Value::Null);
    }

    #[test]
    fn a_get_lists_what_is_held() {
        let mut store = Store::default();
        handle(
            &request("POST", "/signals", r#"{"volume":0.4}"#),
            TOKEN,
            &mut store,
            NOW,
        );
        let (response, changed) = handle(
            &request("GET", "/signals", ""),
            TOKEN,
            &mut store,
            NOW + 30_000,
        );
        assert_eq!(
            response.body,
            json!({ "signals": [{ "name": "volume", "value": 0.4, "expiresIn": 30 }] })
        );
        assert!(!changed);
    }

    #[test]
    fn without_the_token_nothing_is_read_or_set() {
        let mut store = Store::default();
        for authorization in [
            None,
            Some("Bearer wrong-token-0000"),
            Some("0123456789abcdef"),
        ] {
            let mut sent = request("POST", "/signals", r#"{"build":"failed"}"#);
            sent.authorization = authorization;
            let (response, changed) = handle(&sent, TOKEN, &mut store, NOW);
            assert_eq!(response.status, 401, "{authorization:?}");
            assert!(!changed);
        }
        assert!(store.held().is_empty());
    }

    #[test]
    fn an_empty_token_opens_nothing() {
        let mut store = Store::default();
        let mut sent = request("GET", "/signals", "");
        sent.authorization = Some("Bearer ");
        assert_eq!(handle(&sent, "", &mut store, NOW).0.status, 401);
    }

    #[test]
    fn a_web_page_is_refused_even_with_the_token() {
        let mut store = Store::default();
        let mut sent = request("POST", "/signals", r#"{"build":"failed"}"#);
        sent.from_browser = true;
        assert_eq!(handle(&sent, TOKEN, &mut store, NOW).0.status, 403);
        assert!(store.held().is_empty());
    }

    #[test]
    fn a_bad_request_says_why() {
        let mut store = Store::default();
        let cases = [
            (request("POST", "/signals", "not json"), 400),
            (request("POST", "/signals", r#"{"two words":1}"#), 400),
            (
                request("POST", "/signals?ttl=soon", r#"{"build":"ok"}"#),
                400,
            ),
            (request("POST", "/devices", r#"{"build":"ok"}"#), 404),
            (request("DELETE", "/signals", ""), 405),
            (request("OPTIONS", "/signals", ""), 405),
        ];
        for (sent, status) in cases {
            let (response, changed) = handle(&sent, TOKEN, &mut store, NOW);
            assert_eq!(response.status, status, "{} {}", sent.method, sent.url);
            assert!(response.body["error"].is_string());
            assert!(!changed);
        }
    }

    /// The server around [`handle`], on a real socket: the path a sender takes.
    #[test]
    fn the_listener_answers_over_a_socket() {
        use std::io::Write;
        use std::net::TcpStream;

        let listener = Listener::start("127.0.0.1:0".parse().unwrap(), |request| Response {
            status: 200,
            body: json!({ "method": request.method, "browser": request.from_browser }),
        })
        .unwrap();
        let addr = listener.server.server_addr().to_ip().unwrap();

        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST /signals HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
        )
        .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert!(
            reply.contains(r#"{"method":"POST","browser":false}"#),
            "{reply}"
        );
        drop(listener);
    }
}
