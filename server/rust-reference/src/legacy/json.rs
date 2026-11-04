// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

//! The data body of C-11 (legacy wire §5.4): one UTF-8 JSON object with the
//! members `data`, `template-tag`, `html-sha1` and `diff`, in this order, the
//! data keys in the order of C-08, and no HTML escaping (C-11, D-08, P-11).

/// The JSON text of the data response.
pub(crate) fn data_body(data: &[(String, String)], template_tag: &str, html_sha1: &str) -> Vec<u8> {
    let content: usize = data
        .iter()
        .map(|(k, v)| k.len().saturating_add(v.len()).saturating_add(6))
        .fold(64usize, usize::saturating_add);
    let mut out = String::with_capacity(content);
    out.push_str("{\"data\":{");
    for (i, (key, value)) in data.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_string(&mut out, key);
        out.push(':');
        push_string(&mut out, value);
    }
    out.push_str("},\"template-tag\":");
    push_string(&mut out, template_tag);
    out.push_str(",\"html-sha1\":");
    push_string(&mut out, html_sha1);
    out.push_str(",\"diff\":\"\"}");
    out.into_bytes()
}

/// A JSON string (RFC 8259 §7): `"` and `\` escaped, control characters as
/// short escapes or `\u00XX`, every other character as it is.
fn push_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' => {
                let n = u32::from(c);
                out.push_str("\\u00");
                out.push(hex_digit(n >> 4));
                out.push(hex_digit(n & 0xf));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The lowercase hex digit of `n` (`n < 16`).
pub(crate) fn hex_digit(n: u32) -> char {
    char::from_digit(n, 16).unwrap_or('0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_only_what_json_needs() {
        let mut s = String::new();
        push_string(&mut s, "a \"\\\n\r\t\u{8}\u{c}\u{0}\u{1f}</é>&='/");
        assert_eq!(s, "\"a \\\"\\\\\\n\\r\\t\\b\\f\\u0000\\u001f</é>&='/\"");
    }

    #[test]
    fn body_shape_and_order() {
        let data = vec![
            ("{title}".to_owned(), "<title>T</title>".to_owned()),
            ("{a}".to_owned(), "x".to_owned()),
        ];
        let body = data_body(&data, "tt", "hh");
        assert_eq!(
            String::from_utf8(body).ok().as_deref(),
            Some(
                "{\"data\":{\"{title}\":\"<title>T</title>\",\"{a}\":\"x\"},\"template-tag\":\"tt\",\"html-sha1\":\"hh\",\"diff\":\"\"}"
            )
        );
        assert_eq!(
            String::from_utf8(data_body(&[], "", "")).ok().as_deref(),
            Some("{\"data\":{},\"template-tag\":\"\",\"html-sha1\":\"\",\"diff\":\"\"}")
        );
    }

    #[test]
    fn hex_digits() {
        assert_eq!(hex_digit(0), '0');
        assert_eq!(hex_digit(10), 'a');
        assert_eq!(hex_digit(15), 'f');
    }
}
