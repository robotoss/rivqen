// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Command line of `rivqen-reference` (DL-086).
//!
//! ```text
//! rivqen-reference --conformance [--listen <ip>] [--port <n>] [--origin <url>] [--lab-peer]
//! ```
//!
//! The error texts are fixed: they never repeat an argument (DL-078 I1).

use std::net::{IpAddr, Ipv4Addr};

use rivqen_reference::control::DEFAULT_ORIGIN;

/// The usage text.
pub const USAGE: &str = "usage: rivqen-reference --conformance [--listen <ip>] [--port <n>] [--origin <url>] [--lab-peer]
  --conformance  serve the conformance mode of server/conformance/CONTRACT.md (required in this version)
  --listen <ip>  listen address (default 127.0.0.1); not loopback needs --lab-peer
  --port <n>     port 0 to 65535 (default 8080; 0 = any free port)
  --origin <url> origin of the case request URL (default https://shop.example.com)
  --lab-peer     do not check the peer address (container with a port published on 127.0.0.1 only)
The first stdout line is `listening <port>`.";

/// The default port.
pub const DEFAULT_PORT: u16 = 8080;

/// The options of a valid command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `--listen`.
    pub listen: IpAddr,
    /// `--port`.
    pub port: u16,
    /// `--origin` without trailing `/`.
    pub origin: String,
    /// `--lab-peer`.
    pub lab_peer: bool,
}

/// A command line that does not start the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliError {
    /// `--help`: print the usage text, exit 0.
    Help,
    /// A usage error: print the text and the usage text, exit 2.
    Usage(&'static str),
}

/// Read the arguments (without the program name).
///
/// # Errors
/// [`CliError`].
pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Options, CliError> {
    let usage = CliError::Usage;
    let mut conformance = false;
    let mut lab_peer = false;
    let mut listen: Option<IpAddr> = None;
    let mut port: Option<u16> = None;
    let mut origin: Option<String> = None;
    let mut iter = args.iter().map(AsRef::as_ref);
    while let Some(arg) = iter.next() {
        match arg {
            "--help" | "-h" => return Err(CliError::Help),
            "--conformance" => set_flag(&mut conformance)?,
            "--lab-peer" => set_flag(&mut lab_peer)?,
            "--listen" => {
                let value = iter.next().ok_or(usage("--listen needs a value"))?;
                set(
                    &mut listen,
                    parse_ip(value).ok_or(usage("--listen needs an IP address"))?,
                )?;
            }
            "--port" => {
                let value = iter.next().ok_or(usage("--port needs a value"))?;
                set(
                    &mut port,
                    parse_port(value).ok_or(usage("--port needs a number 0 to 65535"))?,
                )?;
            }
            "--origin" => {
                let value = iter.next().ok_or(usage("--origin needs a value"))?;
                set(
                    &mut origin,
                    parse_origin(value).ok_or(usage(
                        "--origin needs http:// or https:// and a host, without path, query or user information",
                    ))?,
                )?;
            }
            _ => return Err(usage("unknown option")),
        }
    }
    if !conformance {
        return Err(usage(
            "--conformance is required (the site mode comes in a later version)",
        ));
    }
    let listen = listen.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    if !listen.is_loopback() && !lab_peer {
        return Err(usage(
            "--conformance with a listen address that is not loopback needs --lab-peer (DL-086)",
        ));
    }
    Ok(Options {
        listen,
        port: port.unwrap_or(DEFAULT_PORT),
        origin: origin.unwrap_or_else(|| DEFAULT_ORIGIN.to_owned()),
        lab_peer,
    })
}

fn set_flag(flag: &mut bool) -> Result<(), CliError> {
    if *flag {
        return Err(CliError::Usage("an option is repeated"));
    }
    *flag = true;
    Ok(())
}

fn set<T>(slot: &mut Option<T>, value: T) -> Result<(), CliError> {
    if slot.is_some() {
        return Err(CliError::Usage("an option is repeated"));
    }
    *slot = Some(value);
    Ok(())
}

/// An IPv4 or IPv6 address; IPv6 also in brackets.
fn parse_ip(value: &str) -> Option<IpAddr> {
    let bare = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(value);
    bare.parse().ok()
}

/// Decimal digits only, 0 to 65535.
fn parse_port(value: &str) -> Option<u16> {
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

/// `http://` or `https://`, then a host with an optional port: visible ASCII
/// without `/`, `?`, `#`, `@` and `\`. Trailing `/` are removed.
fn parse_origin(value: &str) -> Option<String> {
    let trimmed = value.trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    let authority_at = if lower.starts_with("https://") {
        "https://".len()
    } else if lower.starts_with("http://") {
        "http://".len()
    } else {
        return None;
    };
    let authority = trimmed.get(authority_at..)?;
    let valid = !authority.is_empty()
        && authority
            .bytes()
            .all(|b| (0x21..=0x7e).contains(&b) && !b"/?#@\\".contains(&b));
    valid.then(|| trimmed.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn usage_of(args: &[&str]) -> &'static str {
        match parse(args) {
            Err(CliError::Usage(text)) => text,
            other => panic!("expected a usage error, got {other:?}"),
        }
    }

    #[test]
    fn cli_defaults() {
        assert_eq!(
            parse(&["--conformance"]),
            Ok(Options {
                listen: IpAddr::V4(Ipv4Addr::LOCALHOST),
                port: 8080,
                origin: "https://shop.example.com".to_owned(),
                lab_peer: false,
            })
        );
    }

    #[test]
    fn cli_all_options() {
        let o = parse(&[
            "--port",
            "0",
            "--listen",
            "[::1]",
            "--origin",
            "http://127.0.0.1:9000//",
            "--lab-peer",
            "--conformance",
        ])
        .unwrap();
        assert_eq!(o.port, 0);
        assert_eq!(o.listen, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(o.origin, "http://127.0.0.1:9000");
        assert!(o.lab_peer);
        assert_eq!(
            parse(&["--conformance", "--port", "65535"]).unwrap().port,
            65535
        );
        assert_eq!(
            parse(&["--conformance", "--origin", "HTTPS://Shop.Example.com/"])
                .unwrap()
                .origin,
            "HTTPS://Shop.Example.com"
        );
    }

    #[test]
    fn cli_help() {
        assert_eq!(parse(&["--conformance", "--help"]), Err(CliError::Help));
        assert_eq!(parse(&["-h"]), Err(CliError::Help));
    }

    #[test]
    fn cli_rejects_missing_conformance() {
        assert_eq!(
            usage_of(&[]),
            "--conformance is required (the site mode comes in a later version)"
        );
        assert_eq!(
            usage_of(&["--port", "0"]),
            "--conformance is required (the site mode comes in a later version)"
        );
    }

    #[test]
    fn cli_rejects_non_loopback_listen_without_lab_peer() {
        for ip in ["0.0.0.0", "192.0.2.1", "::", "[::]"] {
            assert_eq!(
                usage_of(&["--conformance", "--listen", ip]),
                "--conformance with a listen address that is not loopback needs --lab-peer (DL-086)"
            );
            assert!(parse(&["--conformance", "--listen", ip, "--lab-peer"]).is_ok());
        }
        assert!(parse(&["--conformance", "--listen", "127.0.0.1"]).is_ok());
        assert!(parse(&["--conformance", "--listen", "::1"]).is_ok());
    }

    #[test]
    fn cli_rejects_bad_values() {
        for args in [
            &["--conformance", "--port", "65536"][..],
            &["--conformance", "--port", "-1"],
            &["--conformance", "--port", "+80"],
            &["--conformance", "--port", ""],
            &["--conformance", "--port", "008080"],
            &["--conformance", "--port", "80a"],
        ] {
            assert_eq!(
                usage_of(args),
                "--port needs a number 0 to 65535",
                "{args:?}"
            );
        }
        for ip in ["localhost", "127.0.0.1:80", "", "[127.0.0.1"] {
            assert_eq!(
                usage_of(&["--conformance", "--listen", ip]),
                "--listen needs an IP address"
            );
        }
        for origin in [
            "shop.example.com",
            "ftp://shop.example.com",
            "https://",
            "https:///",
            "https://shop.example.com/base",
            "https://shop.example.com?x",
            "https://shop.example.com#x",
            "https://user@shop.example.com",
            "https://shop example.com",
            "https://shop.example.com\\x",
            "https://é.example",
        ] {
            assert!(
                usage_of(&["--conformance", "--origin", origin]).starts_with("--origin needs"),
                "{origin}"
            );
        }
    }

    #[test]
    fn cli_rejects_missing_values_unknown_and_repeated_options() {
        assert_eq!(
            usage_of(&["--conformance", "--port"]),
            "--port needs a value"
        );
        assert_eq!(
            usage_of(&["--conformance", "--listen"]),
            "--listen needs a value"
        );
        assert_eq!(
            usage_of(&["--conformance", "--origin"]),
            "--origin needs a value"
        );
        assert_eq!(usage_of(&["--conformance", "--site"]), "unknown option");
        assert_eq!(usage_of(&["--conformance", "--port=0"]), "unknown option");
        assert_eq!(
            usage_of(&["--conformance", "--conformance"]),
            "an option is repeated"
        );
        assert_eq!(
            usage_of(&["--conformance", "--lab-peer", "--lab-peer"]),
            "an option is repeated"
        );
        assert_eq!(
            usage_of(&["--conformance", "--port", "1", "--port", "2"]),
            "an option is repeated"
        );
        assert_eq!(
            usage_of(&["--conformance", "--listen", "::1", "--listen", "::1"]),
            "an option is repeated"
        );
        assert_eq!(
            usage_of(&[
                "--conformance",
                "--origin",
                "http://a",
                "--origin",
                "http://a"
            ]),
            "an option is repeated"
        );
    }
}
