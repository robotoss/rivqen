// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! Loader of the RQP markup fixtures `fixtures/rqp/markup` (WP-06 S1, T-02):
//! `input.html` and `expected.json` of each fixture, as data (DL-146).
//!
//! Each sub-directory of the root is one fixture; its name is the fixture
//! id. Files in the root (the `README.md`) are not fixtures. The fixtures
//! come in the byte order of their ids. `expected.json` is the result object
//! of `tools/rqp-markup/CONTRACT.md` section 3, written by
//! `tools/rqp-markup/gen.py`; [`MarkupFixture::expected_result`] reads it
//! into typed values. Every failure names the path of the file.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::fixtures::{FixtureError, load_json};
use crate::fixtures_root;

/// One markup fixture.
#[derive(Clone, PartialEq, Eq)]
pub struct MarkupFixture {
    /// The fixture id: the directory name, for example
    /// `FX-RQ-MARKUP-SIMPLE`.
    pub id: String,
    /// The fixture directory.
    pub dir: PathBuf,
    /// The exact bytes of `input.html`.
    pub input: Vec<u8>,
    /// `expected.json` as JSON.
    pub expected: Value,
}

/// Writes the id, the directory and the input length only: an input can be
/// 5 MiB long.
impl fmt::Debug for MarkupFixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkupFixture")
            .field("id", &self.id)
            .field("dir", &self.dir)
            .field("input_len", &self.input.len())
            .finish_non_exhaustive()
    }
}

/// The result object of `expected.json` (`CONTRACT.md` section 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedMarkup {
    /// `valid`.
    pub valid: bool,
    /// `error`: the `RQP_MARKUP_*` code, or `None` for a valid document.
    pub error: Option<String>,
    /// `blocks`, in document order (empty for an invalid document).
    pub blocks: Vec<ExpectedBlock>,
    /// `template_revision` (`t1.`), or `None` for an invalid document.
    pub template_revision: Option<String>,
    /// `page_revision` (`r1.`, no policy line), or `None` for an invalid
    /// document.
    pub page_revision: Option<String>,
}

/// One member of `blocks` in `expected.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedBlock {
    /// `id`.
    pub id: String,
    /// `format`: `html` or `json`.
    pub format: String,
    /// `start`: offset of the first content byte.
    pub start: usize,
    /// `end`: offset of the `<` of the block end tag.
    pub end: usize,
    /// `sha256`: `B64U(SHA-256(content))`.
    pub sha256: String,
}

impl MarkupFixture {
    fn shape(&self, what: impl Into<String>) -> FixtureError {
        FixtureError::Shape {
            path: self.dir.join("expected.json"),
            what: what.into(),
        }
    }

    fn string(&self, value: &Value, key: &str) -> Result<String, FixtureError> {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| self.shape(format!("`{key}` is not a string")))
    }

    fn string_or_null(&self, key: &str) -> Result<Option<String>, FixtureError> {
        match self.expected.get(key) {
            Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.clone())),
            _ => Err(self.shape(format!("`{key}` is not a string or null"))),
        }
    }

    fn offset(&self, value: &Value, key: &str) -> Result<usize, FixtureError> {
        value
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| self.shape(format!("`{key}` is not an offset")))
    }

    /// `expected.json` as typed values.
    ///
    /// # Errors
    /// [`FixtureError::Shape`] with the path when a member is missing or has
    /// another type.
    pub fn expected_result(&self) -> Result<ExpectedMarkup, FixtureError> {
        let valid = self
            .expected
            .get("valid")
            .and_then(Value::as_bool)
            .ok_or_else(|| self.shape("`valid` is not a boolean"))?;
        let blocks = self
            .expected
            .get("blocks")
            .and_then(Value::as_array)
            .ok_or_else(|| self.shape("`blocks` is not an array"))?
            .iter()
            .map(|block| {
                Ok(ExpectedBlock {
                    id: self.string(block, "id")?,
                    format: self.string(block, "format")?,
                    start: self.offset(block, "start")?,
                    end: self.offset(block, "end")?,
                    sha256: self.string(block, "sha256")?,
                })
            })
            .collect::<Result<Vec<_>, FixtureError>>()?;
        Ok(ExpectedMarkup {
            valid,
            error: self.string_or_null("error")?,
            blocks,
            template_revision: self.string_or_null("template_revision")?,
            page_revision: self.string_or_null("page_revision")?,
        })
    }
}

fn io_err(path: &Path) -> impl Fn(io::Error) -> FixtureError + '_ {
    move |source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The fixtures of `fixtures/rqp/markup` in the byte order of their ids.
///
/// # Errors
/// [`FixtureError`] with the path of the first file that cannot be read.
pub fn load() -> Result<Vec<MarkupFixture>, FixtureError> {
    load_at(&fixtures_root().join("rqp").join("markup"))
}

/// The fixtures of the directory `root` (one sub-directory per fixture) in
/// the byte order of their ids.
///
/// # Errors
/// [`FixtureError::Io`] when a directory or a file cannot be read (also a
/// fixture without `input.html` or `expected.json`),
/// [`FixtureError::Json`] when `expected.json` is not JSON, and
/// [`FixtureError::Shape`] when a directory name is not UTF-8.
pub fn load_at(root: &Path) -> Result<Vec<MarkupFixture>, FixtureError> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root).map_err(io_err(root))? {
        let entry = entry.map_err(io_err(root))?;
        if !entry.file_type().map_err(io_err(root))?.is_dir() {
            continue;
        }
        let path = entry.path();
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| FixtureError::Shape {
                path: path.clone(),
                what: "the directory name is not UTF-8".to_owned(),
            })?;
        dirs.push((id, path));
    }
    // `String` compares by bytes.
    dirs.sort();
    dirs.into_iter()
        .map(|(id, dir)| {
            let input_path = dir.join("input.html");
            let input = fs::read(&input_path).map_err(io_err(&input_path))?;
            let expected = load_json(&dir.join("expected.json"))?;
            Ok(MarkupFixture {
                id,
                dir,
                input,
                expected,
            })
        })
        .collect()
}
