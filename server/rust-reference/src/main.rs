// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rivqen-reference`: the Rivqen reference server (WP-03).
//!
//! Exit codes: `2` usage error (the usage text goes to stderr); `1` the server
//! cannot start (site directory, configuration file, listen, stdout, workers);
//! `0` `--help`. The server itself runs until it is stopped. The first stdout
//! line is `listening <port>`. With `--lab-peer` the first stderr line is
//! [`LAB_PEER_WARNING`].

mod cli;
#[cfg(unix)]
mod site;

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::io::Read;
use std::io::{self, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::path::Path;
use std::process::ExitCode;

use rivqen_reference::control::{Server, Settings};
use rivqen_reference::http::handler::Handler;
use rivqen_reference::http::server::{ServeOptions, Timeouts, log, serve};
#[cfg(unix)]
use rivqen_reference::rqp::config::RqpConfig;

/// The warning line of `--lab-peer` (DL-097): one line on stderr at start.
pub const LAB_PEER_WARNING: &str = "rivqen-reference: warning: --lab-peer is on: control requests are accepted from loopback and private peers (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, fc00::/7, fe80::/10); publish the port on 127.0.0.1 only";

/// The largest `--config` file.
#[cfg(unix)]
pub const MAX_CONFIG_BYTES: u64 = 64 * 1024;

fn main() -> ExitCode {
    let mut args = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(arg) => args.push(arg),
            Err(_) => return usage_error("an argument is not UTF-8"),
        }
    }
    match cli::parse(&args) {
        Ok(options) => run(&options),
        Err(cli::CliError::Help) => {
            let mut out = io::stdout().lock();
            if writeln!(out, "{}", cli::USAGE)
                .and_then(|()| out.flush())
                .is_err()
            {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(cli::CliError::Usage(text)) => usage_error(text),
    }
}

fn usage_error(text: &str) -> ExitCode {
    log(format_args!("rivqen-reference: {text}\n{}", cli::USAGE));
    ExitCode::from(2)
}

/// A start failure: one line on stderr, exit 1. `text` is fixed (no argument).
fn start_error(text: &str) -> ExitCode {
    log(format_args!("rivqen-reference: {text}"));
    ExitCode::FAILURE
}

fn run(options: &cli::Options) -> ExitCode {
    // A panic message can hold input data; the log gets only the location.
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(at) => log(format_args!("panic at {}:{}", at.file(), at.line())),
        None => log(format_args!("panic at an unknown location")),
    }));
    match &options.mode {
        cli::Mode::Conformance { lab_peer } => {
            let server = Server::new(Settings {
                conformance: true,
                lab_peer: *lab_peer,
                origin: options.origin.clone(),
            });
            // The conformance mode runs one worker: the runner sends one case at
            // a time, and the connections are answered in accept order (DL-097).
            let warning = lab_peer.then_some(LAB_PEER_WARNING);
            listen_and_serve(options, &server, 1, warning)
        }
        cli::Mode::Site {
            dir,
            protocol,
            workers,
        } => run_site(options, dir, protocol, *workers),
    }
}

/// The site mode (Unix only, I-2).
#[cfg(unix)]
fn run_site(
    options: &cli::Options,
    dir: &str,
    protocol: &cli::Protocol,
    workers: usize,
) -> ExitCode {
    let engine = match protocol {
        cli::Protocol::Legacy(profile) => site::Engine::Legacy(*profile),
        cli::Protocol::Rqp { config: None } => site::Engine::Rqp(RqpConfig::default()),
        cli::Protocol::Rqp { config: Some(file) } => match read_config(Path::new(file)) {
            Ok(config) => site::Engine::Rqp(config),
            Err(text) => return start_error(text),
        },
    };
    let site = match site::Site::new(Path::new(dir), engine, options.origin.clone()) {
        Ok(site) => site,
        Err(e) => {
            log(format_args!(
                "rivqen-reference: the --site directory cannot be used: {:?}",
                e.kind()
            ));
            return ExitCode::FAILURE;
        }
    };
    listen_and_serve(options, &site, workers, None)
}

/// The command line refuses `--site` on a target that is not Unix (I-2).
#[cfg(not(unix))]
fn run_site(_: &cli::Options, _: &str, _: &cli::Protocol, _: usize) -> ExitCode {
    usage_error(cli::UNIX_ONLY)
}

/// Read the `--config` file: a JSON object of at most [`MAX_CONFIG_BYTES`] for
/// [`RqpConfig::from_json`].
#[cfg(unix)]
fn read_config(path: &Path) -> Result<RqpConfig, &'static str> {
    let file = File::open(path).map_err(|_| "the --config file cannot be read")?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "the --config file cannot be read")?;
    if u64::try_from(bytes.len()).map_or(true, |n| n > MAX_CONFIG_BYTES) {
        return Err("the --config file is larger than 64 KiB");
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "the --config file is not JSON")?;
    RqpConfig::from_json(&value).map_err(|e| e.reason)
}

fn listen_and_serve<H: Handler>(
    options: &cli::Options,
    handler: &H,
    workers: usize,
    warning: Option<&str>,
) -> ExitCode {
    let listener = match TcpListener::bind((options.listen, options.port)) {
        Ok(listener) => listener,
        Err(e) => {
            log(format_args!(
                "rivqen-reference: cannot listen on {}:{}: {:?}",
                options.listen,
                options.port,
                e.kind()
            ));
            return ExitCode::FAILURE;
        }
    };
    if let Some(warning) = warning {
        log(format_args!("{warning}"));
    }
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            log(format_args!(
                "rivqen-reference: no local address: {:?}",
                e.kind()
            ));
            return ExitCode::FAILURE;
        }
    };
    let mut out = io::stdout().lock();
    if let Err(e) = writeln!(out, "listening {port}").and_then(|()| out.flush()) {
        log(format_args!(
            "rivqen-reference: cannot write to stdout: {:?}",
            e.kind()
        ));
        return ExitCode::FAILURE;
    }
    drop(out);
    let serve_options = ServeOptions {
        timeouts: Timeouts::default(),
        workers,
    };
    let error = serve(&listener, handler, &serve_options);
    log(format_args!(
        "rivqen-reference: the workers cannot run: {:?}",
        error.kind()
    ));
    ExitCode::FAILURE
}
