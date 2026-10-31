// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! `rivqen-reference`: the Rivqen reference server (WP-03).
//!
//! Exit codes: `2` usage error (the usage text goes to stderr); `1` the server
//! cannot start (listen, stdout); `0` `--help`. The server itself runs until it is
//! stopped. The first stdout line is `listening <port>`.

mod cli;

use std::io::{self, Write};
use std::net::TcpListener;
use std::process::ExitCode;

use rivqen_reference::control::{Server, Settings};
use rivqen_reference::http::server::{Timeouts, log, serve};

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

fn run(options: &cli::Options) -> ExitCode {
    // A panic message can hold input data; the log gets only the location.
    std::panic::set_hook(Box::new(|info| match info.location() {
        Some(at) => log(format_args!("panic at {}:{}", at.file(), at.line())),
        None => log(format_args!("panic at an unknown location")),
    }));
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
    let mut server = Server::new(Settings {
        conformance: true,
        lab_peer: options.lab_peer,
        origin: options.origin.clone(),
    });
    serve(&listener, &mut server, &Timeouts::default())
}
