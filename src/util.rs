/// Stable 64-bit FNV-1a hash (std's DefaultHasher is not guaranteed stable across releases).
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Human readable relative time ("3 minutes ago"), in Paste's style.
pub fn relative_time(created_ms: i64, now_ms: i64) -> String {
    let secs = ((now_ms - created_ms) / 1000).max(0);
    if secs < 45 {
        return "Just now".into();
    }
    let mins = secs / 60;
    if mins < 1 {
        return "1 minute ago".into();
    }
    if mins < 60 {
        return if mins == 1 { "1 minute ago".into() } else { format!("{mins} minutes ago") };
    }
    let hours = mins / 60;
    if hours < 24 {
        return if hours == 1 { "1 hour ago".into() } else { format!("{hours} hours ago") };
    }
    let days = hours / 24;
    if days == 1 {
        return "Yesterday".into();
    }
    if days < 7 {
        return format!("{days} days ago");
    }
    if days < 30 {
        let w = days / 7;
        return if w == 1 { "1 week ago".into() } else { format!("{w} weeks ago") };
    }
    use chrono::{Datelike, Local, TimeZone};
    let then = Local.timestamp_millis_opt(created_ms).single();
    match then {
        Some(t) => {
            let now = Local.timestamp_millis_opt(now_ms).single();
            if now.map(|n| n.year() == t.year()).unwrap_or(false) {
                t.format("%b %-d").to_string()
            } else {
                t.format("%b %-d, %Y").to_string()
            }
        }
        None => format!("{days} days ago"),
    }
}

pub fn absolute_time(created_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(created_ms)
        .single()
        .map(|t| t.format("%b %-d, %Y at %-I:%M %p").to_string())
        .unwrap_or_default()
}

pub fn format_bytes(n: i64) -> String {
    const KB: f64 = 1024.0;
    let n = n as f64;
    if n < KB {
        format!("{n} B")
    } else if n < KB * KB {
        format!("{:.0} KB", n / KB)
    } else if n < KB * KB * KB {
        format!("{:.1} MB", n / KB / KB)
    } else {
        format!("{:.2} GB", n / KB / KB / KB)
    }
}

pub fn count_label(n: i64, singular: &str, plural: &str) -> String {
    if n == 1 { format!("1 {singular}") } else { format!("{n} {plural}") }
}

/// Parses a CSS-like color string. Returns normalized "#RRGGBB" or "#RRGGBBAA".
pub fn parse_color(text: &str) -> Option<String> {
    let s = text.trim();
    if s.len() > 48 || s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        // "#123" / "#1234" / "#20240917" are far more likely issue numbers or dates than colors.
        let all_digits = hex.chars().all(|c| c.is_ascii_digit());
        if all_digits && hex.len() != 6 {
            return None;
        }
        return match hex.len() {
            3 => {
                let c: Vec<char> = hex.chars().collect();
                Some(format!("#{0}{0}{1}{1}{2}{2}", c[0], c[1], c[2]).to_uppercase())
            }
            4 => {
                let c: Vec<char> = hex.chars().collect();
                Some(format!("#{0}{0}{1}{1}{2}{2}{3}{3}", c[0], c[1], c[2], c[3]).to_uppercase())
            }
            6 | 8 => Some(format!("#{}", hex.to_uppercase())),
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    let (func, alpha_ok) = if lower.starts_with("rgba(") {
        ("rgba(", true)
    } else if lower.starts_with("rgb(") {
        ("rgb(", false)
    } else if lower.starts_with("hsla(") {
        ("hsla(", true)
    } else if lower.starts_with("hsl(") {
        ("hsl(", false)
    } else {
        return None;
    };
    let inner = lower.strip_prefix(func)?.strip_suffix(')')?;
    let parts: Vec<&str> = inner
        .split(|c| c == ',' || c == ' ' || c == '/')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() < 3 || parts.len() > 4 || (parts.len() == 4 && !alpha_ok && !inner.contains('/')) {
        return None;
    }
    let num = |p: &str, max: f32| -> Option<f32> {
        if let Some(pct) = p.strip_suffix('%') {
            pct.parse::<f32>().ok().map(|v| v / 100.0 * max)
        } else {
            p.parse::<f32>().ok()
        }
    };
    let alpha = if parts.len() == 4 { num(parts[3], 1.0)?.clamp(0.0, 1.0) } else { 1.0 };
    let (r, g, b) = if func.starts_with("rgb") {
        (
            num(parts[0], 255.0)?.clamp(0.0, 255.0),
            num(parts[1], 255.0)?.clamp(0.0, 255.0),
            num(parts[2], 255.0)?.clamp(0.0, 255.0),
        )
    } else {
        let h = parts[0].trim_end_matches("deg").parse::<f32>().ok()?.rem_euclid(360.0);
        let sat = num(parts[1], 1.0)?.clamp(0.0, 1.0);
        let l = num(parts[2], 1.0)?.clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, sat, l);
        (r * 255.0, g * 255.0, b * 255.0)
    };
    let mut out = format!("#{:02X}{:02X}{:02X}", r.round() as u8, g.round() as u8, b.round() as u8);
    if alpha < 1.0 {
        out.push_str(&format!("{:02X}", (alpha * 255.0).round() as u8));
    }
    Some(out)
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

/// Parses "#RRGGBB" / "#RRGGBBAA" into (r, g, b, a) floats 0..1.
pub fn hex_to_rgba(hex: &str) -> Option<(f32, f32, f32, f32)> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 && h.len() != 8 {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|b| b as f32 / 255.0);
    let a = if h.len() == 8 { v(6)? } else { 1.0 };
    Some((v(0)?, v(2)?, v(4)?, a))
}

pub fn looks_like_url(text: &str) -> bool {
    let t = text.trim();
    if t.contains(char::is_whitespace) || t.len() > 2048 {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return false;
    }
    url::Url::parse(t).map(|u| u.host_str().is_some()).unwrap_or(false)
}

pub fn decode_html_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(end) = rest.find(';') {
            let ent = &rest[1..end];
            let decoded = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some(' '),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "hellip" => Some('…'),
                "laquo" => Some('«'),
                "raquo" => Some('»'),
                _ => {
                    if let Some(num) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                        u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
                    } else if let Some(num) = ent.strip_prefix('#') {
                        num.parse::<u32>().ok().and_then(char::from_u32)
                    } else {
                        None
                    }
                }
            };
            if let Some(c) = decoded {
                if end < 12 {
                    out.push(c);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff").as_deref(), Some("#FFFFFF"));
        assert_eq!(parse_color("#1a2B3c").as_deref(), Some("#1A2B3C"));
        assert_eq!(parse_color("rgb(255, 0, 0)").as_deref(), Some("#FF0000"));
        assert_eq!(parse_color("rgba(0, 0, 255, 0.5)").as_deref(), Some("#0000FF80"));
        assert_eq!(parse_color("hsl(120, 100%, 50%)").as_deref(), Some("#00FF00"));
        assert_eq!(parse_color("hello"), None);
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("#1234"), None);
        assert_eq!(parse_color("#123"), None);
        assert_eq!(parse_color("#333333").as_deref(), Some("#333333"));
        assert_eq!(parse_color("#abc").as_deref(), Some("#AABBCC"));
    }

    #[test]
    fn urls() {
        assert!(looks_like_url("https://example.com/path?q=1"));
        assert!(!looks_like_url("https://example.com and more"));
        assert!(!looks_like_url("example.com"));
    }

    #[test]
    fn times() {
        assert_eq!(relative_time(0, 10_000), "Just now");
        assert_eq!(relative_time(0, 5 * 60_000), "5 minutes ago");
        assert_eq!(relative_time(0, 3 * 3_600_000), "3 hours ago");
        assert_eq!(relative_time(0, 26 * 3_600_000), "Yesterday");
    }

    #[test]
    fn entities() {
        assert_eq!(decode_html_entities("A &amp; B &#x27;c&#x27; &lt;3"), "A & B 'c' <3");
    }
}
