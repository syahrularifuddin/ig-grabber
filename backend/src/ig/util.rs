use serde_json::Value;

/// Style a GET as a top-level document navigation. The default client carries
/// XHR headers (sec-fetch-dest: empty, mode: cors, x-requested-with) — sent on
/// an HTML page request, that navigation/fetch mismatch makes Instagram serve
/// a stripped bot-variant page without the embedded relay data.
pub fn as_navigation(rb: wreq::RequestBuilder) -> wreq::RequestBuilder {
    rb.header("sec-fetch-dest", "document")
        .header("sec-fetch-mode", "navigate")
        .header("sec-fetch-site", "none")
        .header("sec-fetch-user", "?1")
        .header(
            "accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("upgrade-insecure-requests", "1")
}

/// Decode the string escapes Instagram uses inside embedded JSON/HTML
/// (`\u0026`, `\/`, ...).
pub fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{0008}'),
            Some('f') => out.push('\u{000C}'),
            Some('/') => out.push('/'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                let code = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
                if (0xD800..0xDC00).contains(&code) && chars.peek() == Some(&'\\') {
                    let lookahead: String = chars.clone().take(6).collect();
                    if let Some(low) = parse_low_surrogate(&lookahead) {
                        let combined = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                        if let Some(ch) = char::from_u32(combined) {
                            out.push(ch);
                            chars.by_ref().take(6).for_each(drop);
                            continue;
                        }
                    }
                }
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_low_surrogate(s: &str) -> Option<u32> {
    if s.len() == 6 && s.starts_with("\\u") {
        if let Ok(low) = u32::from_str_radix(&s[2..6], 16) {
            if (0xDC00..0xE000).contains(&low) {
                return Some(low);
            }
        }
    }
    None
}

/// Find the value of a `"key":"..."` pair in raw (escaped) JSON text and unescape it.
pub fn find_json_string(hay: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = hay.find(&needle)? + needle.len();
    let mut raw = String::new();
    let mut chars = hay[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => {
                if let Some(n) = chars.next() {
                    raw.push('\\');
                    raw.push(n);
                }
            }
            _ => raw.push(c),
        }
    }
    Some(json_unescape(&raw))
}

/// Extract a complete JSON object that follows `marker`, using string-aware
/// brace matching.
pub fn extract_json_object<'a>(hay: &'a str, marker: &str) -> Option<&'a str> {
    let pos = hay.find(marker)? + marker.len();
    let rest = &hay[pos..];
    let start = rest.find('{')?;
    let bytes = &rest[start..];

    let mut depth = 0usize;
    let mut in_str = false;
    let mut esc = false;
    for (i, ch) in bytes.char_indices() {
        if esc {
            esc = false;
            continue;
        }
        match ch {
            '\\' if in_str => esc = true,
            '"' => in_str = !in_str,
            '{' if !in_str => depth += 1,
            '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(&bytes[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Read the `content="..."` of an og: meta tag.
pub fn meta_content(html: &str, property: &str) -> Option<String> {
    let needle = format!("<meta property=\"{property}\" content=\"");
    let start = html.find(&needle)? + needle.len();
    let end = html[start..].find('"')? + start;
    Some(html_unescape(&html[start..end]))
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Convert a serde_json Value that may be a string or integer into a string.
pub fn value_to_string(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(i) = v.as_i64() {
        return Some(i.to_string());
    }
    if let Some(u) = v.as_u64() {
        return Some(u.to_string());
    }
    None
}

/// Pick the highest-resolution URL from an array of `{url,width,height}` candidates.
pub fn best_candidate(v: &Value) -> Option<(String, Option<u32>, Option<u32>)> {
    v.as_array()?
        .iter()
        .filter_map(|c| {
            let url = c["url"].as_str()?.to_string();
            let w = c["width"].as_u64().map(|x| x as u32);
            let h = c["height"].as_u64().map(|x| x as u32);
            Some((w.unwrap_or(0), url, w, h))
        })
        .max_by_key(|(w, ..)| *w)
        .map(|(_, url, w, h)| (url, w, h))
}

/// Percent-encode a string for use as a `x-www-form-urlencoded` component.
pub fn form_urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Validate & sanitize an Instagram username (strips a leading @).
pub fn sanitize_username(raw: &str) -> Option<String> {
    let u = raw.trim().trim_start_matches('@').trim();
    let ok = !u.is_empty()
        && u.len() <= 30
        && u.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
    ok.then(|| u.to_string())
}

pub fn valid_shortcode(s: &str) -> bool {
    (5..=40).contains(&s.len())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Decode an Instagram shortcode (base64url, no padding) into its numeric media id.
pub fn shortcode_to_pk(shortcode: &str) -> Option<u64> {
    let mut id: u64 = 0;
    for c in shortcode.chars() {
        let v = match c {
            'A'..='Z' => c as u64 - 'A' as u64,
            'a'..='z' => c as u64 - 'a' as u64 + 26,
            '0'..='9' => c as u64 - '0' as u64 + 52,
            '-' => 62,
            '_' => 63,
            _ => return None,
        };
        id = id.checked_mul(64)?.checked_add(v)?;
    }
    Some(id)
}
