// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The canonical JSON text of the projection entries (`fixtures/legacy/README.md`
//! "Client `expected.json`": "Sorted" means sorted by the canonical JSON text
//! of each entry, keys in the order of the README, no white space).
//!
//! `serde_json` in this workspace keeps object members sorted by key, so the
//! text is written here with the member order of the README. A string is
//! written as `JSON.stringify` writes it (the `serde_json` string form is the
//! same: `"` and `\` escaped, `\b`, `\f`, `\n`, `\r`, `\t`, other control
//! characters as `\u00xx` in lowercase hex, every other character as is).

use serde_json::Value;

/// A JSON string.
pub(crate) fn string(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// A JSON string or `null`.
pub(crate) fn opt_string(text: Option<&str>) -> String {
    text.map_or_else(|| "null".to_owned(), string)
}

/// A JSON array of already written values.
pub(crate) fn array(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(item);
    }
    out.push(']');
    out
}

/// A JSON object with the members in the given order.
pub(crate) fn object(members: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (name, value)) in members.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&string(name));
        out.push(':');
        out.push_str(value);
    }
    out.push('}');
    out
}

/// `[[name, value], …]`.
pub(crate) fn pairs(list: &[(String, String)]) -> String {
    array(
        &list
            .iter()
            .map(|(n, v)| array(&[string(n), string(v)]))
            .collect::<Vec<_>>(),
    )
}

/// `[[name, value or null], …]`.
pub(crate) fn opt_pairs(list: &[(String, Option<String>)]) -> String {
    array(
        &list
            .iter()
            .map(|(n, v)| array(&[string(n), opt_string(v.as_deref())]))
            .collect::<Vec<_>>(),
    )
}

/// `[string, …]`.
pub(crate) fn strings(list: &[String]) -> String {
    array(&list.iter().map(|s| string(s)).collect::<Vec<_>>())
}

/// The compact JSON object text of `[key, value]` entries in their order:
/// the form of `data_sha256` (README value rule 3).
pub(crate) fn object_of_strings(entries: &[(String, String)]) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&string(key));
        out.push(':');
        out.push_str(&string(value));
    }
    out.push('}');
    out
}

/// The lowercase hex text of bytes.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0F] {
            if let Some(digit) = DIGITS.get(usize::from(nibble)) {
                out.push(char::from(*digit));
            }
        }
    }
    out
}

/// The lowercase hex SHA-256 of bytes.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(rivqen_integrity::sha256(bytes).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{object, object_of_strings, opt_pairs, pairs, sha256_hex, string};

    #[test]
    fn strings_are_written_as_json_stringify_writes_them() {
        assert_eq!(string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(string("\n\t\u{1}"), r#""\n\t\u0001""#);
        assert_eq!(string("/é"), "\"/é\"");
    }

    #[test]
    fn objects_keep_the_member_order() {
        assert_eq!(
            object(&[("z", "1".to_owned()), ("a", "true".to_owned())]),
            r#"{"z":1,"a":true}"#
        );
        assert_eq!(pairs(&[("b".to_owned(), "1".to_owned())]), r#"[["b","1"]]"#);
        assert_eq!(
            opt_pairs(&[("cookie".to_owned(), None)]),
            r#"[["cookie",null]]"#
        );
        assert_eq!(
            object_of_strings(&[("{b}".to_owned(), "x".to_owned())]),
            r#"{"{b}":"x"}"#
        );
    }

    #[test]
    fn sha256_of_the_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
