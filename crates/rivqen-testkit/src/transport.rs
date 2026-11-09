// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! A fake transport: scripted responses in order and a record of every request.
//! No sleep, no thread, no network.

use std::collections::VecDeque;
use std::fmt;

/// The fake transport got a request that the script does not cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnexpectedRequest<Req> {
    /// The request that came.
    pub request: Req,
    /// How many requests the transport had answered before this one.
    pub answered: usize,
}

impl<Req> fmt::Display for UnexpectedRequest<Req> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unexpected request: the script has {} response(s) and all are used",
            self.answered
        )
    }
}

impl<Req: fmt::Debug> std::error::Error for UnexpectedRequest<Req> {}

/// A scripted transport. The n-th request gets the n-th scripted response.
#[derive(Debug)]
pub struct FakeTransport<Req, Resp> {
    script: VecDeque<Resp>,
    requests: Vec<Req>,
    answered: usize,
}

impl<Req, Resp> FakeTransport<Req, Resp> {
    /// A transport that answers with `script`, in order.
    #[must_use]
    pub fn new(script: impl IntoIterator<Item = Resp>) -> Self {
        Self {
            script: script.into_iter().collect(),
            requests: Vec::new(),
            answered: 0,
        }
    }

    /// Record `request` and return the next scripted response.
    ///
    /// # Errors
    /// [`UnexpectedRequest`] when the script is used up. The request is still recorded.
    pub fn send(&mut self, request: Req) -> Result<Resp, UnexpectedRequest<Req>>
    where
        Req: Clone,
    {
        self.requests.push(request.clone());
        match self.script.pop_front() {
            Some(resp) => {
                self.answered = self.answered.saturating_add(1);
                Ok(resp)
            }
            None => Err(UnexpectedRequest {
                request,
                answered: self.answered,
            }),
        }
    }

    /// Every request received so far, in order, also the unexpected ones.
    #[must_use]
    pub fn requests(&self) -> &[Req] {
        &self.requests
    }

    /// Scripted responses not used yet.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.script.len()
    }
}
