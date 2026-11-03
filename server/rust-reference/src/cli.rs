// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Command line of `rivqen-reference` (DL-086; WP-03 S2 §2 "Command line",
//! DL-096).
//!
//! ```text
//! rivqen-reference --conformance [--listen <ip>] [--port <n>] [--origin <url>] [--lab-peer --control-token-file <file>]
//! rivqen-reference --site <dir> --protocol rqp|legacy [--profile <name>] [--config <file>]
//!                  --origin <url> [--listen <ip>] [--port <n>] [--workers <n>]
//! ```
//!
//! The parse reads no file: the site directory and the configuration file are
//! read at start (`main.rs`). The error texts are fixed: they never repeat an
//! argument (DL-078 I1).

use std::net::{IpAddr, Ipv4Addr};

use rivqen_reference::control::DEFAULT_ORIGIN;
use rivqen_reference::model::Profile;

/// The usage text.
pub const USAGE: &str = "usage: rivqen-reference --conformance [--listen <ip>] [--port <n>] [--origin <url>] [--lab-peer --control-token-file <file>]
       rivqen-reference --site <dir> --protocol rqp|legacy [--profile <name>] [--config <file>]
                        --origin <url> [--listen <ip>] [--port <n>] [--workers <n>]
  --conformance  serve the conformance mode of server/conformance/CONTRACT.md
  --site <dir>   serve the regular files of <dir> with one protocol (excludes --conformance, --lab-peer)
  --protocol     rqp or legacy (required with --site)
  --profile      legacy only: legacy-canonical, legacy-java, legacy-node or legacy-php (required)
  --config       rqp only: a JSON file with the server configuration (default {})
  --workers <n>  site mode: 1 to 64 worker threads (default 8)
  --listen <ip>  listen address (default 127.0.0.1); conformance: not loopback needs --lab-peer
  --port <n>     port 0 to 65535 (default 8080; 0 = any free port)
  --origin <url> origin of the request URL that the engines see; required with --site
                 (conformance default https://shop.example.com)
  --lab-peer     accept control requests also from private peers (10/8, 172.16/12, 192.168/16,
                 fc00::/7, fe80::/10): a container with its port published on 127.0.0.1 only;
                 needs --control-token-file
  --control-token-file <file>  conformance: every control request needs this token (16 to 256
                 base64url characters) in Rivqen-Conformance-Token; required with --lab-peer
The first stdout line is `listening <port>`.";

/// The default port.
pub const DEFAULT_PORT: u16 = 8080;
/// The default number of workers of the site mode.
pub const DEFAULT_WORKERS: usize = 8;
/// The most workers of the site mode.
pub const MAX_WORKERS: usize = 64;
/// The usage error of `--site` on a target that is not Unix (I-2).
pub const UNIX_ONLY: &str = "the site mode needs a Unix file system";

/// The protocol of the site mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protocol {
    /// `--protocol legacy --profile <name>`.
    Legacy(Profile),
    /// `--protocol rqp [--config <file>]`.
    Rqp {
        /// The configuration file, as given.
        config: Option<String>,
    },
}

/// The mode of the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// `--conformance`.
    Conformance {
        /// `--lab-peer`.
        lab_peer: bool,
        /// `--control-token-file`, as given.
        token_file: Option<String>,
    },
    /// `--site <dir>`.
    Site {
        /// The directory, as given.
        dir: String,
        /// The protocol.
        protocol: Protocol,
        /// `--workers`.
        workers: usize,
    },
}

/// The options of a valid command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// `--listen`.
    pub listen: IpAddr,
    /// `--port`.
    pub port: u16,
    /// `--origin` without trailing `/`.
    pub origin: String,
    /// The mode and its options.
    pub mode: Mode,
}

/// A command line that does not start the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliError {
    /// `--help`: print the usage text, exit 0.
    Help,
    /// A usage error: print the text and the usage text, exit 2.
    Usage(&'static str),
}

/// The options as they are read, before the rules between them.
#[derive(Default)]
struct Raw {
    conformance: bool,
    lab_peer: bool,
    listen: Option<IpAddr>,
    port: Option<u16>,
    origin: Option<String>,
    site: Option<String>,
    protocol: Option<&'static str>,
    profile: Option<Profile>,
    config: Option<String>,
    workers: Option<usize>,
    token_file: Option<String>,
}

/// Read the arguments (without the program name).
///
/// # Errors
/// [`CliError`].
pub fn parse<S: AsRef<str>>(args: &[S]) -> Result<Options, CliError> {
    let usage = CliError::Usage;
    let mut raw = Raw::default();
    let mut iter = args.iter().map(AsRef::as_ref);
    while let Some(arg) = iter.next() {
        let mut value = |missing| iter.next().ok_or(usage(missing));
        match arg {
            "--help" | "-h" => return Err(CliError::Help),
            "--conformance" => set_flag(&mut raw.conformance)?,
            "--lab-peer" => set_flag(&mut raw.lab_peer)?,
            "--listen" => {
                let v = value("--listen needs a value")?;
                set(
                    &mut raw.listen,
                    parse_ip(v).ok_or(usage("--listen needs an IP address"))?,
                )?;
            }
            "--port" => {
                let v = value("--port needs a value")?;
                set(
                    &mut raw.port,
                    parse_port(v).ok_or(usage("--port needs a number 0 to 65535"))?,
                )?;
            }
            "--origin" => {
                let v = value("--origin needs a value")?;
                set(
                    &mut raw.origin,
                    parse_origin(v).ok_or(usage(
                        "--origin needs http:// or https:// and a host, without path, query or user information",
                    ))?,
                )?;
            }
            "--site" => {
                let v = value("--site needs a value")?;
                if v.is_empty() {
                    return Err(usage("--site needs a directory"));
                }
                set(&mut raw.site, v.to_owned())?;
            }
            "--protocol" => {
                let v = value("--protocol needs a value")?;
                let protocol = match v {
                    "rqp" => "rqp",
                    "legacy" => "legacy",
                    _ => return Err(usage("--protocol needs rqp or legacy")),
                };
                set(&mut raw.protocol, protocol)?;
            }
            "--profile" => {
                let v = value("--profile needs a value")?;
                set(
                    &mut raw.profile,
                    Profile::from_name(v).ok_or(usage(
                        "--profile needs legacy-canonical, legacy-java, legacy-node or legacy-php",
                    ))?,
                )?;
            }
            "--config" => {
                let v = value("--config needs a value")?;
                if v.is_empty() {
                    return Err(usage("--config needs a file"));
                }
                set(&mut raw.config, v.to_owned())?;
            }
            "--workers" => {
                let v = value("--workers needs a value")?;
                set(
                    &mut raw.workers,
                    parse_workers(v).ok_or(usage("--workers needs a number 1 to 64"))?,
                )?;
            }
            "--control-token-file" => {
                let v = value("--control-token-file needs a value")?;
                if v.is_empty() {
                    return Err(usage("--control-token-file needs a file"));
                }
                set(&mut raw.token_file, v.to_owned())?;
            }
            _ => return Err(usage("unknown option")),
        }
    }
    let mode = match raw.site.take() {
        Some(dir) => site_mode(&mut raw, dir)?,
        None => conformance_mode(&raw)?,
    };
    Ok(Options {
        listen: raw.listen.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        port: raw.port.unwrap_or(DEFAULT_PORT),
        origin: raw.origin.unwrap_or_else(|| DEFAULT_ORIGIN.to_owned()),
        mode,
    })
}

fn site_mode(raw: &mut Raw, dir: String) -> Result<Mode, CliError> {
    let usage = CliError::Usage;
    // The path checks of the site need Unix (links, device and inode; I-2).
    if cfg!(not(unix)) {
        return Err(usage(UNIX_ONLY));
    }
    if raw.conformance || raw.lab_peer {
        return Err(usage("--site excludes --conformance and --lab-peer"));
    }
    if raw.token_file.is_some() {
        return Err(usage("--control-token-file is for --conformance only"));
    }
    let protocol = match (raw.protocol, raw.profile, raw.config.take()) {
        (None, _, _) => return Err(usage("--site needs --protocol rqp or legacy")),
        (Some("legacy"), None, _) => return Err(usage("--protocol legacy needs --profile")),
        (Some("legacy"), Some(_), Some(_)) => {
            return Err(usage("--config is for --protocol rqp only"));
        }
        (Some("legacy"), Some(profile), None) => Protocol::Legacy(profile),
        (Some(_), Some(_), _) => return Err(usage("--protocol rqp refuses --profile")),
        (Some(_), None, config) => Protocol::Rqp { config },
    };
    // A client checks the manifest origin against the page URL: a default
    // origin would give every page an invalid manifest (DL-096 addendum).
    if raw.origin.is_none() {
        return Err(usage("--site needs --origin <url>"));
    }
    Ok(Mode::Site {
        dir,
        protocol,
        workers: raw.workers.unwrap_or(DEFAULT_WORKERS),
    })
}

fn conformance_mode(raw: &Raw) -> Result<Mode, CliError> {
    let usage = CliError::Usage;
    if !raw.conformance {
        return Err(usage("--conformance or --site <dir> is required"));
    }
    if raw.protocol.is_some()
        || raw.profile.is_some()
        || raw.config.is_some()
        || raw.workers.is_some()
    {
        return Err(usage(
            "--protocol, --profile, --config and --workers need --site",
        ));
    }
    let loopback = raw.listen.is_none_or(|ip| ip.is_loopback());
    if !loopback && !raw.lab_peer {
        return Err(usage(
            "--conformance with a listen address that is not loopback needs --lab-peer (DL-086)",
        ));
    }
    // A peer of a private network must also know the per-run token (H-117).
    if raw.lab_peer && raw.token_file.is_none() {
        return Err(usage("--lab-peer needs --control-token-file <file>"));
    }
    Ok(Mode::Conformance {
        lab_peer: raw.lab_peer,
        token_file: raw.token_file.clone(),
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

/// Decimal digits only, 1 to [`MAX_WORKERS`].
fn parse_workers(value: &str) -> Option<usize> {
    if value.is_empty() || value.len() > 2 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|n| (1..=MAX_WORKERS).contains(n))
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

    fn conformance(lab_peer: bool, token_file: Option<&str>) -> Mode {
        Mode::Conformance {
            lab_peer,
            token_file: token_file.map(str::to_owned),
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
                mode: conformance(false, None),
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
            "--control-token-file",
            "t.txt",
            "--conformance",
        ])
        .unwrap();
        assert_eq!(o.port, 0);
        assert_eq!(o.listen, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(o.origin, "http://127.0.0.1:9000");
        assert_eq!(o.mode, conformance(true, Some("t.txt")));
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
    fn cli_rejects_missing_mode() {
        for args in [&[][..], &["--port", "0"], &["--lab-peer"]] {
            assert_eq!(
                usage_of(args),
                "--conformance or --site <dir> is required",
                "{args:?}"
            );
        }
    }

    #[test]
    fn cli_rejects_non_loopback_listen_without_lab_peer() {
        for ip in ["0.0.0.0", "192.0.2.1", "::", "[::]"] {
            assert_eq!(
                usage_of(&["--conformance", "--listen", ip]),
                "--conformance with a listen address that is not loopback needs --lab-peer (DL-086)"
            );
            assert!(
                parse(&[
                    "--conformance",
                    "--listen",
                    ip,
                    "--lab-peer",
                    "--control-token-file",
                    "t"
                ])
                .is_ok()
            );
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
        for (option, text) in [
            ("--port", "--port needs a value"),
            ("--listen", "--listen needs a value"),
            ("--origin", "--origin needs a value"),
            ("--site", "--site needs a value"),
            ("--protocol", "--protocol needs a value"),
            ("--profile", "--profile needs a value"),
            ("--config", "--config needs a value"),
            ("--workers", "--workers needs a value"),
        ] {
            assert_eq!(usage_of(&["--conformance", option]), text);
        }
        assert_eq!(usage_of(&["--conformance", "--port=0"]), "unknown option");
        assert_eq!(usage_of(&["--conformance", "--secret"]), "unknown option");
        for args in [
            &["--conformance", "--conformance"][..],
            &["--conformance", "--lab-peer", "--lab-peer"],
            &["--conformance", "--port", "1", "--port", "2"],
            &["--conformance", "--listen", "::1", "--listen", "::1"],
            &[
                "--conformance",
                "--origin",
                "http://a",
                "--origin",
                "http://a",
            ],
            &["--site", "d", "--site", "d"],
            &["--site", "d", "--protocol", "rqp", "--protocol", "rqp"],
            &[
                "--site",
                "d",
                "--profile",
                "legacy-java",
                "--profile",
                "legacy-java",
            ],
            &["--site", "d", "--config", "c", "--config", "c"],
            &["--site", "d", "--workers", "1", "--workers", "1"],
        ] {
            assert_eq!(usage_of(args), "an option is repeated", "{args:?}");
        }
    }

    fn site(protocol: Protocol, workers: usize) -> Mode {
        Mode::Site {
            dir: "pages".to_owned(),
            protocol,
            workers,
        }
    }

    #[test]
    fn cli_site_rqp_and_legacy() {
        let o = parse(&[
            "--site",
            "pages",
            "--protocol",
            "rqp",
            "--origin",
            "https://app.example/",
        ])
        .unwrap();
        assert_eq!(o.mode, site(Protocol::Rqp { config: None }, 8));
        assert_eq!(o.listen, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!((o.port, o.origin.as_str()), (8080, "https://app.example"));
        let o = parse(&[
            "--site",
            "pages",
            "--protocol",
            "rqp",
            "--config",
            "c.json",
            "--workers",
            "64",
            "--listen",
            "0.0.0.0",
            "--port",
            "0",
            "--origin",
            "http://127.0.0.1:8080",
        ])
        .unwrap();
        assert_eq!(
            o.mode,
            site(
                Protocol::Rqp {
                    config: Some("c.json".to_owned())
                },
                64
            )
        );
        // The site mode may listen on any address: it has no control endpoint.
        assert_eq!(o.listen, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(o.origin, "http://127.0.0.1:8080");
        for profile in Profile::ALL {
            let o = parse(&[
                "--protocol",
                "legacy",
                "--profile",
                profile.name(),
                "--site",
                "pages",
                "--workers",
                "1",
                "--origin",
                "http://a",
            ])
            .unwrap();
            assert_eq!(o.mode, site(Protocol::Legacy(profile), 1));
        }
    }

    #[test]
    fn cli_site_needs_origin() {
        for args in [
            &["--site", "pages", "--protocol", "rqp"][..],
            &["--site", "pages", "--protocol", "rqp", "--workers", "2"],
            &[
                "--site",
                "pages",
                "--protocol",
                "legacy",
                "--profile",
                "legacy-php",
            ],
        ] {
            assert_eq!(usage_of(args), "--site needs --origin <url>", "{args:?}");
        }
        // The conformance mode keeps its default origin.
        assert_eq!(
            parse(&["--conformance"]).unwrap().origin,
            "https://shop.example.com"
        );
    }

    #[test]
    fn cli_site_excludes_conformance_and_lab_peer() {
        for args in [
            &["--site", "pages", "--protocol", "rqp", "--conformance"][..],
            &["--site", "pages", "--protocol", "rqp", "--lab-peer"],
            &["--conformance", "--lab-peer", "--site", "pages"],
        ] {
            assert_eq!(
                usage_of(args),
                "--site excludes --conformance and --lab-peer",
                "{args:?}"
            );
        }
    }

    #[test]
    fn cli_site_protocol_rules() {
        assert_eq!(
            usage_of(&["--site", "pages"]),
            "--site needs --protocol rqp or legacy"
        );
        assert_eq!(
            usage_of(&["--site", "pages", "--profile", "legacy-java"]),
            "--site needs --protocol rqp or legacy"
        );
        for p in ["RQP", "http", ""] {
            assert_eq!(
                usage_of(&["--site", "pages", "--protocol", p]),
                "--protocol needs rqp or legacy"
            );
        }
        assert_eq!(
            usage_of(&["--site", "pages", "--protocol", "legacy"]),
            "--protocol legacy needs --profile"
        );
        assert_eq!(
            usage_of(&[
                "--site",
                "pages",
                "--protocol",
                "rqp",
                "--profile",
                "legacy-java"
            ]),
            "--protocol rqp refuses --profile"
        );
        assert_eq!(
            usage_of(&[
                "--site",
                "pages",
                "--protocol",
                "legacy",
                "--profile",
                "legacy-php",
                "--config",
                "c.json"
            ]),
            "--config is for --protocol rqp only"
        );
        for p in ["legacy-ruby", "LEGACY-JAVA", ""] {
            assert!(usage_of(&["--site", "d", "--profile", p]).starts_with("--profile needs"));
        }
        assert_eq!(usage_of(&["--site", ""]), "--site needs a directory");
        assert_eq!(
            usage_of(&["--site", "d", "--protocol", "rqp", "--config", ""]),
            "--config needs a file"
        );
    }

    #[test]
    fn cli_site_workers_range() {
        for w in ["0", "65", "100", "", "-1", "+8", "8a", "008"] {
            assert_eq!(
                usage_of(&["--site", "pages", "--protocol", "rqp", "--workers", w]),
                "--workers needs a number 1 to 64",
                "{w}"
            );
        }
        for (w, n) in [("1", 1), ("08", 8), ("64", 64)] {
            let o = parse(&[
                "--site",
                "pages",
                "--protocol",
                "rqp",
                "--workers",
                w,
                "--origin",
                "http://a",
            ])
            .unwrap();
            assert_eq!(o.mode, site(Protocol::Rqp { config: None }, n));
        }
    }

    /// On a target that is not Unix the site mode is a usage error (I-2); on
    /// Unix the same command line is valid.
    #[test]
    fn cli_site_needs_unix() {
        let args = ["--site", "d", "--protocol", "rqp", "--origin", "http://a"];
        if cfg!(unix) {
            assert!(parse(&args).is_ok());
        } else {
            assert_eq!(usage_of(&args), UNIX_ONLY);
        }
        assert_eq!(UNIX_ONLY, "the site mode needs a Unix file system");
    }

    #[test]
    fn control_token_cli_rules() {
        // Required with --lab-peer.
        for args in [
            &["--conformance", "--lab-peer"][..],
            &["--conformance", "--listen", "0.0.0.0", "--lab-peer"],
        ] {
            assert_eq!(
                usage_of(args),
                "--lab-peer needs --control-token-file <file>",
                "{args:?}"
            );
        }
        // Allowed without --lab-peer too.
        let o = parse(&["--conformance", "--control-token-file", "/run/t"]).unwrap();
        assert_eq!(o.mode, conformance(false, Some("/run/t")));
        // Not with --site.
        assert_eq!(
            usage_of(&[
                "--site",
                "d",
                "--protocol",
                "rqp",
                "--origin",
                "http://a",
                "--control-token-file",
                "t"
            ]),
            "--control-token-file is for --conformance only"
        );
        assert_eq!(
            usage_of(&["--conformance", "--control-token-file"]),
            "--control-token-file needs a value"
        );
        assert_eq!(
            usage_of(&["--conformance", "--control-token-file", ""]),
            "--control-token-file needs a file"
        );
        assert_eq!(
            usage_of(&[
                "--conformance",
                "--control-token-file",
                "a",
                "--control-token-file",
                "a"
            ]),
            "an option is repeated"
        );
    }

    #[test]
    fn cli_site_options_need_site() {
        for args in [
            &["--conformance", "--protocol", "rqp"][..],
            &["--conformance", "--profile", "legacy-java"],
            &["--conformance", "--config", "c.json"],
            &["--conformance", "--workers", "4"],
        ] {
            assert_eq!(
                usage_of(args),
                "--protocol, --profile, --config and --workers need --site",
                "{args:?}"
            );
        }
    }
}
