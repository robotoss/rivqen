// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Contract types shared by all lanes (WP-03 S1). Fixed by the architect; a lane
//! that needs a change returns `BLOCKED` with the question.
//!
//! The protocol engines ([`crate::legacy`], [`crate::rqp`]) are pure functions of
//! these types: no I/O, no clock, no shared state between calls.

/// One field line, in the order of the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The name as received or as it is sent: its case is kept (D-01: the
    /// `legacy-php` profile compares the request name byte for byte).
    pub name: String,
    /// The value. From the HTTP reader: without leading and trailing spaces and
    /// tabs (RFC 9110 §5.5). From a control case: as given. An engine trims
    /// spaces and tabs again when its rules say "trimmed".
    pub value: String,
}

impl Field {
    /// A field line with this name and value.
    #[must_use]
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_owned(),
            value: value.to_owned(),
        }
    }

    /// True when the name equals `name` by ASCII case-insensitive comparison.
    #[must_use]
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

/// The request that reaches a protocol engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The method token as received (case-sensitive, RFC 9110 §9.1).
    pub method: String,
    /// The absolute URL: the configured origin (default `https://shop.example.com`)
    /// followed by the path and query of the request target (CONTRACT.md §2,
    /// "Case requests"; DL-060).
    pub url: String,
    /// Every request field line, in order. Repeated lines stay separate lines.
    pub fields: Vec<Field>,
}

/// The response of the application before the protocol step ("A").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppResponse {
    /// Status code.
    pub status: u16,
    /// Field lines in order.
    pub fields: Vec<Field>,
    /// The exact body bytes.
    pub body: Vec<u8>,
}

/// The body of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// No body: `304`, the `HEAD` responses.
    None,
    /// These bytes.
    Bytes(Vec<u8>),
}

/// The response that the HTTP writer sends.
///
/// Rules for the engines: `fields` never has `content-length`, `transfer-encoding`
/// or `connection` (the writer owns the framing: it writes `fields` in order,
/// exactly as given, then `Content-Length` for [`Body::Bytes`]). The writer never
/// sends a body for a `HEAD` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Field lines in order, exactly as they are sent.
    pub fields: Vec<Field>,
    /// The body.
    pub body: Body,
}

/// A legacy profile (ADR-005; legacy wire §10 and §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// `legacy-canonical`: rules C-01…C-16.
    Canonical,
    /// `legacy-java`: §10.1, column `legacy-java`.
    Java,
    /// `legacy-node`: §10.1, column `legacy-node`.
    Node,
    /// `legacy-php`: §10.1, column `legacy-php`.
    Php,
}

impl Profile {
    /// All profiles, in the order of `GET /__rivqen/conformance/info`.
    pub const ALL: [Profile; 4] = [
        Profile::Canonical,
        Profile::Java,
        Profile::Node,
        Profile::Php,
    ];

    /// The profile name of the fixtures and the runner.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Profile::Canonical => "legacy-canonical",
            Profile::Java => "legacy-java",
            Profile::Node => "legacy-node",
            Profile::Php => "legacy-php",
        }
    }

    /// The profile with this exact name, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// The application response unchanged: the pass-through of both protocols.
///
/// For a `HEAD` request the body is [`Body::None`].
#[must_use]
pub fn pass_through(request: &Request, app: &AppResponse) -> Response {
    Response {
        status: app.status,
        fields: app
            .fields
            .iter()
            .filter(|f| {
                !f.is("content-length") && !f.is("transfer-encoding") && !f.is("connection")
            })
            .cloned()
            .collect(),
        body: if request.method == "HEAD" {
            Body::None
        } else {
            Body::Bytes(app.body.clone())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_round_trip() {
        for p in Profile::ALL {
            assert_eq!(Profile::from_name(p.name()), Some(p));
        }
        assert_eq!(Profile::from_name("LEGACY-JAVA"), None);
        assert_eq!(Profile::from_name(""), None);
    }

    #[test]
    fn field_is_compares_ascii_case_insensitively() {
        let f = Field::new("Accept-Diff", "true");
        assert!(f.is("accept-diff"));
        assert!(!f.is("accept-dif"));
    }

    #[test]
    fn pass_through_drops_framing_fields_and_head_body() {
        let app = AppResponse {
            status: 200,
            fields: vec![
                Field::new("Content-Type", "text/html"),
                Field::new("Content-Length", "3"),
                Field::new("X-App", "1"),
            ],
            body: b"abc".to_vec(),
        };
        let get = Request {
            method: "GET".into(),
            url: "https://shop.example.com/catalog".into(),
            fields: vec![],
        };
        let r = pass_through(&get, &app);
        assert_eq!(r.status, 200);
        assert_eq!(
            r.fields,
            vec![
                Field::new("Content-Type", "text/html"),
                Field::new("X-App", "1")
            ]
        );
        assert_eq!(r.body, Body::Bytes(b"abc".to_vec()));
        let head = Request {
            method: "HEAD".into(),
            ..get
        };
        assert_eq!(pass_through(&head, &app).body, Body::None);
    }
}
