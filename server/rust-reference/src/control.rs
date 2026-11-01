// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Conformance control endpoint (CONTRACT.md §2; DL-066, DL-078, DL-081, DL-086).
//!
//! The server keeps one case at a time. `POST /__rivqen/conformance/case` stores
//! it; every other request goes to the engine of the stored case
//! ([`crate::legacy::respond`] or [`crate::rqp::respond`]) with the application
//! response of the case. The checks run in the order of CONTRACT.md §2: the flag
//! (`404`), the peer (`403`), `Host` (`403`), the path (`404`), the method
//! (`405`), `Content-Type` (`415`), the size (`413`), the case (`400`, `422`).
//!
//! The functions here do no I/O: the server loop ([`crate::http::server`]) reads
//! the head, asks [`Server::route`], reads the body when the route needs it, and
//! writes the answer of [`Server::respond`].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::panic::{AssertUnwindSafe, catch_unwind};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value};

use crate::http::reader::{MAX_BODY_BYTES, RequestHead, is_field_value_byte, is_token};
use crate::http::writer::check_response;
use crate::model::{AppResponse, Body, Field, Profile, Request, Response};
use crate::rqp::config::RqpConfig;

/// The path prefix of the control endpoint.
pub const CONTROL_PREFIX: &str = "/__rivqen/conformance/";
/// `GET`: the suites and profiles of the server.
pub const INFO_PATH: &str = "/__rivqen/conformance/info";
/// `POST`: store a case.
pub const CASE_PATH: &str = "/__rivqen/conformance/case";
/// The largest `POST /case` body (CONTRACT.md §2).
pub const MAX_CASE_BYTES: u64 = 8 * 1024 * 1024;
/// The default origin of the case request URL (CONTRACT.md §2; DL-060).
pub const DEFAULT_ORIGIN: &str = "https://shop.example.com";

/// The settings of the server from the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `--conformance`: the control endpoint and the case requests are on. Off:
    /// every path is `404` (the site mode comes in S2, H-114).
    pub conformance: bool,
    /// `--lab-peer`: do not check the peer address (DL-081). The `Host` and
    /// `Content-Type` checks stay.
    pub lab_peer: bool,
    /// The origin of the case request URL, without a trailing `/`.
    pub origin: String,
}

/// The connection of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conn {
    /// The peer address.
    pub peer: IpAddr,
    /// The local port of the connection (the port in an allowed `Host`).
    pub local_port: u16,
}

/// The protocol engine of a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engine {
    /// The legacy engine in this profile.
    Legacy(Profile),
    /// The RQP engine with this configuration.
    Rqp(RqpConfig),
}

/// A stored case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// `case_id` (for diagnostics only; never logged unescaped).
    pub case_id: String,
    /// The engine and its configuration.
    pub engine: Engine,
    /// The application response: `app` with the body `page_base64`.
    pub app: AppResponse,
}

/// Why a case body is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseError {
    /// `400 BAD_CASE`: invalid JSON or member. The text says which rule, without
    /// input values.
    Bad(&'static str),
    /// `422 UNSUPPORTED`: the profile is not supported.
    Unsupported,
}

/// What a request needs after its head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `GET /__rivqen/conformance/info`.
    Info,
    /// `POST /__rivqen/conformance/case`.
    StoreCase,
    /// A case request: the engine of the stored case.
    CaseRequest,
}

/// The next step of a request after its head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// Answer now; the body is not read.
    Respond(Outcome),
    /// Read the body (at most [`MAX_BODY_BYTES`]), then call [`Server::respond`].
    ReadBody(Route),
}

/// An answer and a fixed note for the log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The answer.
    pub response: Response,
    /// Why, when it is not the normal answer. A fixed text: never an input value.
    pub note: Option<&'static str>,
}

impl Outcome {
    fn plain(response: Response) -> Self {
        Self {
            response,
            note: None,
        }
    }

    fn noted(response: Response, note: &'static str) -> Self {
        Self {
            response,
            note: Some(note),
        }
    }
}

/// A JSON answer `{"error": code}`.
#[must_use]
pub fn error_response(status: u16, code: &str) -> Response {
    json_response(status, &serde_json::json!({ "error": code }))
}

fn json_response(status: u16, value: &Value) -> Response {
    Response {
        status,
        fields: vec![Field::new("Content-Type", "application/json")],
        body: Body::Bytes(value.to_string().into_bytes()),
    }
}

/// The body of `GET /info`: both suites and the four legacy profiles.
#[must_use]
pub fn info_json() -> Value {
    serde_json::json!({
        "contract": 1,
        "suites": ["rqp", "legacy"],
        "profiles": Profile::ALL.map(Profile::name),
    })
}

/// The peer addresses of CONTRACT.md §2: `127.0.0.1`, `::1`, and `127.0.0.1`
/// mapped into IPv6.
#[must_use]
pub fn is_loopback_peer(peer: IpAddr) -> bool {
    match peer {
        IpAddr::V4(a) => a == Ipv4Addr::LOCALHOST,
        IpAddr::V6(a) => {
            a == Ipv6Addr::LOCALHOST || a.to_ipv4_mapped() == Some(Ipv4Addr::LOCALHOST)
        }
    }
}

/// `Host` of a control request: `127.0.0.1:<port>`, `localhost:<port>` or
/// `[::1]:<port>`, ASCII case-insensitive (DL-078).
#[must_use]
pub fn is_control_host(host: &str, port: u16) -> bool {
    ["127.0.0.1", "localhost", "[::1]"]
        .iter()
        .any(|name| host.eq_ignore_ascii_case(&format!("{name}:{port}")))
}

/// `Content-Type` of a `POST`: the media type `application/json` (ASCII
/// case-insensitive), parameters allowed.
#[must_use]
pub fn is_json_content_type(value: &str) -> bool {
    let media = value.split(';').next().unwrap_or_default();
    media
        .trim_matches([' ', '\t'])
        .eq_ignore_ascii_case("application/json")
}

/// The single value of the field `name`, or `None` when there is no line or more
/// than one.
fn single<'a>(head: &'a RequestHead, name: &'a str) -> Option<&'a str> {
    let mut values = head.values(name);
    match (values.next(), values.next()) {
        (Some(value), None) => Some(value),
        _ => None,
    }
}

/// Run `f` and catch a panic: the last guard around an engine (ERR-08). The
/// panic hook of the binary writes only the location.
///
/// # Errors
/// `Err(())` when `f` panicked.
#[allow(clippy::result_unit_err)]
pub fn guard<T, F: FnOnce() -> T>(f: F) -> Result<T, ()> {
    // AssertUnwindSafe: the engines are pure functions of shared references; a
    // panic leaves no state half-changed.
    catch_unwind(AssertUnwindSafe(f)).map_err(drop)
}

/// The server state: the settings and the stored case.
#[derive(Debug, Clone)]
pub struct Server {
    settings: Settings,
    case: Option<Case>,
}

impl Server {
    /// A server without a case.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            case: None,
        }
    }

    /// The stored case, if any.
    #[must_use]
    pub fn case(&self) -> Option<&Case> {
        self.case.as_ref()
    }

    /// The checks that need only the head, in the order of CONTRACT.md §2.
    #[must_use]
    pub fn route(&self, conn: &Conn, head: &RequestHead) -> Next {
        let refuse =
            |status, code, note| Next::Respond(Outcome::noted(error_response(status, code), note));
        if !self.settings.conformance {
            return refuse(404, "NOT_FOUND", "the conformance mode is off");
        }
        let path = head.path();
        if !path.starts_with(CONTROL_PREFIX) {
            if head.content_length > MAX_BODY_BYTES {
                return refuse(413, "TOO_LARGE", "the body is larger than 8 MiB");
            }
            return Next::ReadBody(Route::CaseRequest);
        }
        if !self.settings.lab_peer && !is_loopback_peer(conn.peer) {
            return refuse(403, "FORBIDDEN", "the peer is not loopback");
        }
        if !single(head, "host").is_some_and(|h| is_control_host(h, conn.local_port)) {
            return refuse(
                403,
                "FORBIDDEN",
                "the Host is not a loopback name with the port",
            );
        }
        let (route, method) = match path {
            INFO_PATH => (Route::Info, "GET"),
            CASE_PATH => (Route::StoreCase, "POST"),
            _ => return refuse(404, "NOT_FOUND", "an unknown control path"),
        };
        if head.method != method {
            let mut response = error_response(405, "METHOD_NOT_ALLOWED");
            response.fields.push(Field::new("Allow", method));
            return Next::Respond(Outcome::noted(response, "a wrong method"));
        }
        if route == Route::StoreCase
            && !single(head, "content-type").is_some_and(is_json_content_type)
        {
            return refuse(
                415,
                "UNSUPPORTED_MEDIA_TYPE",
                "the Content-Type is not application/json",
            );
        }
        if head.content_length > MAX_CASE_BYTES {
            return refuse(413, "TOO_LARGE", "the body is larger than 8 MiB");
        }
        Next::ReadBody(route)
    }

    /// The answer of a request after [`Server::route`] gave `route` and the body was
    /// read.
    pub fn respond(&mut self, route: Route, head: &RequestHead, body: &[u8]) -> Outcome {
        match route {
            Route::Info => Outcome::plain(json_response(200, &info_json())),
            Route::StoreCase => match parse_case(body) {
                Ok(case) => {
                    self.case = Some(case);
                    Outcome::plain(Response {
                        status: 204,
                        fields: Vec::new(),
                        body: Body::None,
                    })
                }
                Err(CaseError::Bad(note)) => Outcome::noted(error_response(400, "BAD_CASE"), note),
                Err(CaseError::Unsupported) => Outcome::noted(
                    error_response(422, "UNSUPPORTED"),
                    "the profile is not supported",
                ),
            },
            Route::CaseRequest => self.case_request(head),
        }
    }

    /// [`Server::route`], then [`Server::respond`] with a body that is already read.
    pub fn handle(&mut self, conn: &Conn, head: &RequestHead, body: &[u8]) -> Outcome {
        match self.route(conn, head) {
            Next::Respond(outcome) => outcome,
            Next::ReadBody(route) => self.respond(route, head, body),
        }
    }

    fn case_request(&self, head: &RequestHead) -> Outcome {
        let Some(case) = &self.case else {
            return Outcome::noted(error_response(503, "NO_CASE"), "no case is stored");
        };
        let request = Request {
            method: head.method.clone(),
            url: format!("{}{}", self.settings.origin, head.target),
            fields: head.fields.clone(),
        };
        let result = guard(|| match &case.engine {
            Engine::Legacy(profile) => {
                (crate::legacy::respond(*profile, &request, &case.app), None)
            }
            Engine::Rqp(config) => {
                let (response, outcome) = crate::rqp::select::select(config, &request, &case.app);
                (response, passthrough_note(outcome))
            }
        });
        match result {
            Ok((response, note)) => match check_response(&response) {
                Ok(()) => Outcome { response, note },
                Err(reason) => Outcome::noted(error_response(500, "HANDLE_FAILED"), reason),
            },
            Err(()) => Outcome::noted(error_response(500, "HANDLE_FAILED"), "the engine panicked"),
        }
    }
}

/// Read a `POST /case` body (CONTRACT.md §2). Every `400` rule is checked before
/// the `422` rule, as the harness does.
///
/// # Errors
/// [`CaseError`].
pub fn parse_case(body: &[u8]) -> Result<Case, CaseError> {
    let bad = CaseError::Bad;
    let json: Value = serde_json::from_slice(body).map_err(|_| bad("the body is not JSON"))?;
    let obj = json.as_object().ok_or(bad("the body is not an object"))?;
    let legacy = match obj.get("suite").and_then(Value::as_str) {
        Some("legacy") => true,
        Some("rqp") => false,
        _ => return Err(bad("suite is not rqp or legacy")),
    };
    let case_id = obj
        .get("case_id")
        .and_then(Value::as_str)
        .ok_or(bad("case_id is not a string"))?;
    let profile = if legacy {
        Some(
            obj.get("profile")
                .and_then(Value::as_str)
                .ok_or(bad("profile is not a string"))?,
        )
    } else {
        None
    };
    let page = obj
        .get("page_base64")
        .and_then(Value::as_str)
        .and_then(|text| STANDARD.decode(text).ok())
        .ok_or(bad("page_base64 is not standard base64"))?;
    let app = obj
        .get("app")
        .and_then(Value::as_object)
        .ok_or(bad("app is not an object"))?;
    let status = app
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|s| u16::try_from(s).ok())
        .filter(|s| (100..=599).contains(s))
        .ok_or(bad("app.status is not an integer 100 to 599"))?;
    let fields = app_fields(app).ok_or(bad("app.headers is not a list of field lines"))?;
    let empty = Value::Object(Map::new());
    let server_config = match obj.get("server_config") {
        None => &empty,
        Some(v) if v.is_object() => v,
        Some(_) => return Err(bad("server_config is not an object")),
    };
    let engine = match profile {
        Some(name) => Engine::Legacy(Profile::from_name(name).ok_or(CaseError::Unsupported)?),
        None => Engine::Rqp(RqpConfig::from_json(server_config).map_err(|e| bad(e.reason))?),
    };
    Ok(Case {
        case_id: case_id.to_owned(),
        engine,
        app: AppResponse {
            status,
            fields,
            body: page,
        },
    })
}

/// `app.headers`: `[[name, value], …]`, the name a token and the value only
/// field-value characters (DL-078).
fn app_fields(app: &Map<String, Value>) -> Option<Vec<Field>> {
    let list = app.get("headers")?.as_array()?;
    let mut fields = Vec::with_capacity(list.len());
    for pair in list {
        let [name, value] = pair.as_array()?.as_slice() else {
            return None;
        };
        let (name, value) = (name.as_str()?, value.as_str()?);
        if !is_token(name.as_bytes()) || !value.bytes().all(is_field_value_byte) {
            return None;
        }
        fields.push(Field::new(name, value));
    }
    Some(fields)
}

/// The log note of an RQP answer that hides an internal defect of the server
/// (errors-logging ERR-01, ERR-02): a fixed text, never an input value.
fn passthrough_note(outcome: crate::rqp::select::Outcome) -> Option<&'static str> {
    use crate::rqp::select::{Full, Outcome as Rqp};
    match outcome {
        Rqp::Internal => Some("SERVER_PASSTHROUGH: internal defect, A sent"),
        Rqp::Full(Full::Defect) => Some("SERVER_PASSTHROUGH: internal defect, F sent"),
        _ => None,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines
)]
mod tests {
    use super::*;
    use crate::http::reader::Version;

    const PORT: u16 = 18080;

    fn settings() -> Settings {
        Settings {
            conformance: true,
            lab_peer: false,
            origin: DEFAULT_ORIGIN.to_owned(),
        }
    }

    fn local() -> Conn {
        Conn {
            peer: IpAddr::V4(Ipv4Addr::LOCALHOST),
            local_port: PORT,
        }
    }

    fn head(method: &str, target: &str, fields: &[(&str, &str)], length: u64) -> RequestHead {
        RequestHead {
            method: method.to_owned(),
            target: target.to_owned(),
            version: Version::Http11,
            fields: fields.iter().map(|(n, v)| Field::new(n, v)).collect(),
            content_length: length,
        }
    }

    fn control(method: &str, path: &str, extra: &[(&str, &str)], length: u64) -> RequestHead {
        let mut fields = vec![("Host", "127.0.0.1:18080")];
        fields.extend_from_slice(extra);
        head(method, path, &fields, length)
    }

    fn post_case(body: &str) -> RequestHead {
        control(
            "POST",
            CASE_PATH,
            &[("Content-Type", "application/json")],
            body.len() as u64,
        )
    }

    fn status_of(next: &Next) -> u16 {
        match next {
            Next::Respond(o) => o.response.status,
            Next::ReadBody(_) => 0,
        }
    }

    fn json_error(outcome: &Outcome) -> String {
        let Body::Bytes(bytes) = &outcome.response.body else {
            panic!("no body");
        };
        let v: Value = serde_json::from_slice(bytes).unwrap();
        v["error"].as_str().unwrap().to_owned()
    }

    fn legacy_case(profile: &str) -> String {
        serde_json::json!({
            "suite": "legacy",
            "case_id": "FX-LEG-T",
            "profile": profile,
            "page_base64": "PHA+aGk8L3A+",
            "app": { "status": 200, "headers": [["content-type", "text/html; charset=utf-8"], ["X-App", "1"], ["x-app", "2"]] },
            "server_config": {}
        })
        .to_string()
    }

    fn store(server: &mut Server, body: &str) -> Outcome {
        server.handle(&local(), &post_case(body), body.as_bytes())
    }

    #[test]
    fn control_info_lists_both_suites_and_four_profiles() {
        let mut server = Server::new(settings());
        let o = server.handle(&local(), &control("GET", INFO_PATH, &[], 0), b"");
        assert_eq!(o.response.status, 200);
        assert_eq!(
            o.response.fields,
            vec![Field::new("Content-Type", "application/json")]
        );
        let Body::Bytes(bytes) = &o.response.body else {
            panic!("no body");
        };
        assert_eq!(
            serde_json::from_slice::<Value>(bytes).unwrap(),
            serde_json::json!({
                "contract": 1,
                "suites": ["rqp", "legacy"],
                "profiles": ["legacy-canonical", "legacy-java", "legacy-node", "legacy-php"]
            })
        );
        // A query string does not change the path.
        let o = server.handle(
            &local(),
            &control("GET", "/__rivqen/conformance/info?x", &[], 0),
            b"",
        );
        assert_eq!(o.response.status, 200);
    }

    #[test]
    fn control_without_conformance_is_404_everywhere() {
        let mut server = Server::new(Settings {
            conformance: false,
            ..settings()
        });
        for h in [
            control("GET", INFO_PATH, &[], 0),
            post_case(&legacy_case("legacy-java")),
            head("GET", "/catalog", &[("Host", "shop.example.com")], 0),
        ] {
            let o = server.handle(&local(), &h, b"");
            assert_eq!(o.response.status, 404);
            assert_eq!(json_error(&o), "NOT_FOUND");
        }
        assert!(server.case().is_none());
    }

    #[test]
    fn control_peer_not_loopback_is_403() {
        let server = Server::new(settings());
        let remote = Conn {
            peer: "192.0.2.7".parse().unwrap(),
            local_port: PORT,
        };
        assert_eq!(
            status_of(&server.route(&remote, &control("GET", INFO_PATH, &[], 0))),
            403
        );
        // --lab-peer relaxes only the peer check.
        let lab = Server::new(Settings {
            lab_peer: true,
            ..settings()
        });
        assert_eq!(
            lab.route(&remote, &control("GET", INFO_PATH, &[], 0)),
            Next::ReadBody(Route::Info)
        );
        let bad_host = head("GET", INFO_PATH, &[("Host", "shop.example.com")], 0);
        assert_eq!(status_of(&lab.route(&remote, &bad_host)), 403);
        let mut no_type = post_case("{}");
        no_type.fields.truncate(1);
        assert_eq!(status_of(&lab.route(&remote, &no_type)), 415);
    }

    #[test]
    fn control_loopback_peers() {
        assert!(is_loopback_peer("127.0.0.1".parse().unwrap()));
        assert!(is_loopback_peer("::1".parse().unwrap()));
        assert!(is_loopback_peer("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback_peer("127.0.0.2".parse().unwrap()));
        assert!(!is_loopback_peer("10.0.0.1".parse().unwrap()));
        assert!(!is_loopback_peer("::2".parse().unwrap()));
        assert!(!is_loopback_peer("::ffff:10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn control_host_must_be_a_loopback_name_with_the_port() {
        assert!(is_control_host("127.0.0.1:18080", PORT));
        assert!(is_control_host("LOCALHOST:18080", PORT));
        assert!(is_control_host("[::1]:18080", PORT));
        assert!(!is_control_host("127.0.0.1", PORT));
        assert!(!is_control_host("127.0.0.1:18081", PORT));
        assert!(!is_control_host("evil.example:18080", PORT));
        assert!(!is_control_host("localhost:18080.evil", PORT));
        let server = Server::new(settings());
        let o = |fields: &[(&str, &str)]| {
            status_of(&server.route(&local(), &head("GET", INFO_PATH, fields, 0)))
        };
        assert_eq!(o(&[("Host", "localhost:18080")]), 0);
        assert_eq!(o(&[("Host", "rebind.example:18080")]), 403);
        assert_eq!(o(&[]), 403);
        assert_eq!(
            o(&[("Host", "localhost:18080"), ("Host", "localhost:18080")]),
            403
        );
    }

    #[test]
    fn control_unknown_path_is_404_after_the_host_check() {
        let server = Server::new(settings());
        let n = server.route(
            &local(),
            &control("GET", "/__rivqen/conformance/other", &[], 0),
        );
        assert_eq!(status_of(&n), 404);
        let n = server.route(&local(), &control("GET", "/__rivqen/conformance/", &[], 0));
        assert_eq!(status_of(&n), 404);
        let n = server.route(
            &local(),
            &head("GET", "/__rivqen/conformance/other", &[("Host", "x:1")], 0),
        );
        assert_eq!(status_of(&n), 403);
    }

    #[test]
    fn control_wrong_method_is_405_with_allow() {
        let server = Server::new(settings());
        for (h, allow) in [
            (control("POST", INFO_PATH, &[], 0), "GET"),
            (control("HEAD", INFO_PATH, &[], 0), "GET"),
            (control("GET", CASE_PATH, &[], 0), "POST"),
            (
                control(
                    "post",
                    CASE_PATH,
                    &[("Content-Type", "application/json")],
                    0,
                ),
                "POST",
            ),
        ] {
            let Next::Respond(o) = server.route(&local(), &h) else {
                panic!("expected an answer");
            };
            assert_eq!(o.response.status, 405);
            assert_eq!(json_error(&o), "METHOD_NOT_ALLOWED");
            assert_eq!(o.response.fields.last(), Some(&Field::new("Allow", allow)));
        }
    }

    #[test]
    fn control_case_needs_json_content_type() {
        let server = Server::new(settings());
        let route = |ct: &[(&str, &str)]| {
            status_of(&server.route(&local(), &control("POST", CASE_PATH, ct, 2)))
        };
        assert_eq!(route(&[]), 415);
        assert_eq!(route(&[("Content-Type", "text/plain")]), 415);
        assert_eq!(route(&[("Content-Type", "application/jsonx")]), 415);
        assert_eq!(
            route(&[
                ("Content-Type", "application/json"),
                ("Content-Type", "application/json")
            ]),
            415
        );
        assert_eq!(
            route(&[("Content-Type", "Application/JSON ; charset=utf-8")]),
            0
        );
        assert!(is_json_content_type("application/json"));
        assert!(is_json_content_type("\tapplication/json\t;x=y"));
        assert!(!is_json_content_type(""));
    }

    #[test]
    fn control_case_over_8_mib_is_413() {
        let server = Server::new(settings());
        let mut h = post_case("");
        h.content_length = MAX_CASE_BYTES + 1;
        let n = server.route(&local(), &h);
        assert_eq!(status_of(&n), 413);
        h.content_length = MAX_CASE_BYTES;
        assert_eq!(server.route(&local(), &h), Next::ReadBody(Route::StoreCase));
        // The same limit for a case request.
        let mut h = head(
            "POST",
            "/catalog",
            &[("Host", "shop.example.com")],
            MAX_BODY_BYTES + 1,
        );
        assert_eq!(status_of(&server.route(&local(), &h)), 413);
        h.content_length = MAX_BODY_BYTES;
        assert_eq!(
            server.route(&local(), &h),
            Next::ReadBody(Route::CaseRequest)
        );
        // Info with a large body too.
        let h = control("GET", INFO_PATH, &[], MAX_CASE_BYTES + 1);
        assert_eq!(status_of(&server.route(&local(), &h)), 413);
    }

    #[test]
    fn control_case_is_stored_with_204() {
        let mut server = Server::new(settings());
        let o = store(&mut server, &legacy_case("legacy-java"));
        assert_eq!(o.response.status, 204);
        assert_eq!(o.response.body, Body::None);
        assert_eq!(o.response.fields, Vec::new());
        let case = server.case().unwrap();
        assert_eq!(case.case_id, "FX-LEG-T");
        assert_eq!(case.engine, Engine::Legacy(Profile::Java));
        assert_eq!(case.app.status, 200);
        assert_eq!(case.app.body, b"<p>hi</p>");
        assert_eq!(
            case.app.fields,
            vec![
                Field::new("content-type", "text/html; charset=utf-8"),
                Field::new("X-App", "1"),
                Field::new("x-app", "2"),
            ]
        );
        // A new case replaces the old one.
        let rqp = r#"{"suite":"rqp","case_id":"FX-RQ-P01","page_base64":"","app":{"status":404,"headers":[]},"server_config":{}}"#;
        assert_eq!(store(&mut server, rqp).response.status, 204);
        let case = server.case().unwrap();
        assert_eq!(case.engine, Engine::Rqp(RqpConfig::default()));
        assert_eq!(case.app.status, 404);
        assert_eq!(case.app.body, b"");
    }

    #[test]
    fn control_case_rqp_ignores_profile_and_defaults_server_config() {
        let body = r#"{"suite":"rqp","case_id":"x","profile":7,"page_base64":"YQ==","app":{"status":200,"headers":[]}}"#;
        let case = parse_case(body.as_bytes()).unwrap();
        assert_eq!(case.engine, Engine::Rqp(RqpConfig::default()));
        assert_eq!(case.app.body, b"a");
    }

    #[test]
    fn control_bad_case_is_400() {
        let ok = serde_json::from_str::<Value>(&legacy_case("legacy-canonical")).unwrap();
        let with = |pointer: &str, value: Value| {
            let mut v = ok.clone();
            *v.pointer_mut(pointer).unwrap() = value;
            v.to_string()
        };
        let without = |key: &str| {
            let mut v = ok.clone();
            v.as_object_mut().unwrap().remove(key);
            v.to_string()
        };
        let cases = [
            ("not json".to_owned(), "the body is not JSON"),
            ("[1]".to_owned(), "the body is not an object"),
            (with("/suite", "other".into()), "suite is not rqp or legacy"),
            (with("/suite", Value::Null), "suite is not rqp or legacy"),
            (without("suite"), "suite is not rqp or legacy"),
            (with("/case_id", 1.into()), "case_id is not a string"),
            (without("case_id"), "case_id is not a string"),
            (with("/profile", Value::Null), "profile is not a string"),
            (without("profile"), "profile is not a string"),
            (
                with("/page_base64", "PHA+aGk8L3A".into()),
                "page_base64 is not standard base64",
            ),
            (
                with("/page_base64", "PHA-aGk8L3A_".into()),
                "page_base64 is not standard base64",
            ),
            (
                with("/page_base64", 5.into()),
                "page_base64 is not standard base64",
            ),
            (with("/app", "x".into()), "app is not an object"),
            (
                with("/app/status", 99.into()),
                "app.status is not an integer 100 to 599",
            ),
            (
                with("/app/status", 600.into()),
                "app.status is not an integer 100 to 599",
            ),
            (
                with("/app/status", 200.5.into()),
                "app.status is not an integer 100 to 599",
            ),
            (
                with("/app/status", 70_000.into()),
                "app.status is not an integer 100 to 599",
            ),
            (
                with("/app/status", "200".into()),
                "app.status is not an integer 100 to 599",
            ),
            (
                with("/app/headers", serde_json::json!({})),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["a"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["a", "b", "c"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["a", 1]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([[1, "a"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!(["ab"])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["a b", "1"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["", "1"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["X", "a\r\nY: b"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/app/headers", serde_json::json!([["X", "a\u{7f}"]])),
                "app.headers is not a list of field lines",
            ),
            (
                with("/server_config", Value::Null),
                "server_config is not an object",
            ),
            (
                with("/server_config", serde_json::json!([])),
                "server_config is not an object",
            ),
        ];
        for (body, note) in cases {
            let mut server = Server::new(settings());
            let o = store(&mut server, &body);
            assert_eq!(o.response.status, 400, "{body}");
            assert_eq!(json_error(&o), "BAD_CASE");
            assert_eq!(o.note, Some(note), "{body}");
            assert!(server.case().is_none());
        }
        // Allowed: obs-text and HTAB in a value; status 100 and 599.
        let body = with("/app/headers", serde_json::json!([["X", "é\tx"]]));
        assert!(parse_case(body.as_bytes()).is_ok());
        assert!(parse_case(with("/app/status", 100.into()).as_bytes()).is_ok());
        assert!(parse_case(with("/app/status", 599.into()).as_bytes()).is_ok());
    }

    #[test]
    fn control_bad_case_comes_before_unsupported() {
        let ok = serde_json::from_str::<Value>(&legacy_case("legacy-ruby")).unwrap();
        let mut server = Server::new(settings());
        assert_eq!(store(&mut server, &ok.to_string()).response.status, 422);
        let mut bad = ok.clone();
        bad["server_config"] = Value::Null;
        assert_eq!(store(&mut server, &bad.to_string()).response.status, 400);
    }

    #[test]
    fn control_unknown_profile_is_422() {
        let mut server = Server::new(settings());
        for profile in ["legacy-ruby", "LEGACY-JAVA", ""] {
            let o = store(&mut server, &legacy_case(profile));
            assert_eq!(o.response.status, 422);
            assert_eq!(json_error(&o), "UNSUPPORTED");
        }
        assert!(server.case().is_none());
        for p in Profile::ALL {
            assert_eq!(
                store(&mut server, &legacy_case(p.name())).response.status,
                204
            );
            assert_eq!(server.case().unwrap().engine, Engine::Legacy(p));
        }
    }

    #[test]
    fn control_rqp_server_config_that_from_json_refuses_is_400() {
        // The stub refuses only a non-object; the real config refuses more. Both
        // are BAD_CASE: the error of from_json is mapped, not checked here again.
        let mut server = Server::new(settings());
        let o = store(
            &mut server,
            r#"{"suite":"rqp","case_id":"x","page_base64":"","app":{"status":200,"headers":[]},"server_config":"no"}"#,
        );
        assert_eq!(o.response.status, 400);
        assert_eq!(json_error(&o), "BAD_CASE");
    }

    #[test]
    fn control_case_request_before_a_case_is_503() {
        let mut server = Server::new(settings());
        let o = server.handle(
            &local(),
            &head("GET", "/catalog", &[("Host", "shop.example.com")], 0),
            b"",
        );
        assert_eq!(o.response.status, 503);
        assert_eq!(json_error(&o), "NO_CASE");
    }

    #[test]
    fn control_case_reaches_engine() {
        for profile in ["legacy-canonical", "legacy-php"] {
            let mut server = Server::new(Settings {
                origin: "https://shop.example.com".to_owned(),
                ..settings()
            });
            assert_eq!(
                store(&mut server, &legacy_case(profile)).response.status,
                204
            );
            let h = head(
                "GET",
                "/catalog?x=1",
                &[("Host", "shop.example.com"), ("Accept-Diff", "true")],
                0,
            );
            let o = server.handle(&local(), &h, b"");
            // With the stub engines: the pass-through.
            let case = server.case().unwrap();
            let request = Request {
                method: "GET".to_owned(),
                url: "https://shop.example.com/catalog?x=1".to_owned(),
                fields: h.fields.clone(),
            };
            let Engine::Legacy(p) = case.engine else {
                panic!("legacy");
            };
            assert_eq!(o.response, crate::legacy::respond(p, &request, &case.app));
            assert_eq!(o.response.status, 200);
            assert_eq!(o.response.body, Body::Bytes(b"<p>hi</p>".to_vec()));
            assert_eq!(o.note, None);
        }
        let mut server = Server::new(settings());
        let rqp = r#"{"suite":"rqp","case_id":"x","page_base64":"YWI=","app":{"status":200,"headers":[["ETag","\"1\""]]},"server_config":{}}"#;
        assert_eq!(store(&mut server, rqp).response.status, 204);
        let o = server.handle(&local(), &head("HEAD", "/", &[("Host", "a")], 0), b"");
        assert_eq!(o.response.status, 200);
        assert_eq!(o.response.fields, vec![Field::new("ETag", "\"1\"")]);
        assert_eq!(o.response.body, Body::None);
    }

    #[test]
    fn control_engine_result_that_cannot_be_written_is_500() {
        // pass_through keeps every app field except the framing fields; an app
        // status 100 is a valid case but not a final response.
        let mut server = Server::new(settings());
        let body = legacy_case("legacy-canonical").replace("\"status\":200", "\"status\":101");
        assert_eq!(store(&mut server, &body).response.status, 204);
        let o = server.handle(&local(), &head("GET", "/", &[("Host", "a")], 0), b"");
        assert_eq!(o.response.status, 500);
        assert_eq!(json_error(&o), "HANDLE_FAILED");
        assert_eq!(o.note, Some("the status is not 200 to 599"));
    }

    #[test]
    fn control_guard_catches_a_panic() {
        assert_eq!(guard(|| 7), Ok(7));
        // The default hook prints the message to the captured test output.
        let caught = guard(|| -> u8 { panic!("engine bug") });
        assert_eq!(caught, Err(()));
        // The guard works again after a panic.
        assert_eq!(guard(|| "ok"), Ok("ok"));
    }

    #[test]
    fn control_error_bodies_are_json() {
        let r = error_response(503, "NO_CASE");
        assert_eq!(r.status, 503);
        assert_eq!(r.body, Body::Bytes(br#"{"error":"NO_CASE"}"#.to_vec()));
        assert_eq!(
            r.fields,
            vec![Field::new("Content-Type", "application/json")]
        );
    }
}

#[cfg(test)]
mod passthrough_tests {
    use super::passthrough_note;
    use crate::rqp::select::{Full, Invalid, Outcome};

    #[test]
    fn only_internal_defects_get_a_note() {
        assert!(passthrough_note(Outcome::Internal).is_some());
        assert!(passthrough_note(Outcome::Full(Full::Defect)).is_some());
        assert_eq!(passthrough_note(Outcome::Full(Full::PatchContent)), None);
        assert_eq!(passthrough_note(Outcome::Invalid(Invalid::Markup)), None);
        assert_eq!(passthrough_note(Outcome::Delta), None);
        assert_eq!(passthrough_note(Outcome::NotModified), None);
    }
}
