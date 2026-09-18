//! Text metrics and escaping. Widths are estimated from per-glyph classes so
//! layout is deterministic without a font rasteriser; the estimate is
//! deliberately generous so real fonts fit inside the reserved space.

pub fn sans_width(s: &str, size: f64, bold: bool) -> f64 {
    let em: f64 = s
        .chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | 'I' => 0.28,
            'f' | 't' | 'r' | ' ' | '(' | ')' | '[' | ']' | '/' | '-' => 0.36,
            'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.86,
            c if c.is_ascii_uppercase() => 0.66,
            c if c.is_ascii_digit() => 0.57,
            c if c.is_ascii() => 0.54,
            _ => 0.9,
        })
        .sum();
    em * size * if bold { 1.06 } else { 1.0 }
}

pub fn mono_width(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * size * 0.61
}

/// Truncates with an ellipsis so `measure(result) <= max`.
pub fn fit(s: &str, max: f64, measure: impl Fn(&str) -> f64) -> String {
    if measure(s) <= max {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let candidate: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        if measure(&candidate) <= max {
            return candidate;
        }
    }
    "…".into()
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// JSON safe to embed inside `<script type="application/json">`.
pub fn script_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value)
        .expect("value serializes")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_truncates_to_width() {
        let m = |s: &str| mono_width(s, 10.0);
        assert_eq!(fit("short", 100.0, m), "short");
        let t = fit("a very long label indeed", 60.0, m);
        assert!(t.ends_with('…'));
        assert!(m(&t) <= 60.0);
    }

    #[test]
    fn escaping_blocks_markup_and_script_breakout() {
        assert_eq!(xml_escape("<a href=\"x\">&'"), "&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
        let v = serde_json::json!({"s": "</script><script>alert(1)</script>"});
        assert!(!script_json(&v).contains("</script"));
    }
}
