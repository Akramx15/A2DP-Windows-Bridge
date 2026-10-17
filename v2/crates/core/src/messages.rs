//! Status and error messages as translation keys with arguments: the UI
//! translates them (app/lang/*.json, shared with v1); logs and the CLI use
//! the English text.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

/// A message of app/lang (key, printf-style arguments)
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Msg {
    pub key: &'static str,
    pub args: Vec<String>,
}

impl Msg {
    pub fn new(key: &'static str) -> Msg {
        Msg { key, args: Vec::new() }
    }

    pub fn with(key: &'static str, args: &[&dyn fmt::Display]) -> Msg {
        Msg { key, args: args.iter().map(|a| a.to_string()).collect() }
    }

    /// The English text
    pub fn english(&self) -> String {
        let template = english_strings().get(self.key).map(String::as_str).unwrap_or(self.key);
        format_printf(template, &self.args)
    }
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.english())
    }
}

fn english_strings() -> &'static HashMap<String, String> {
    static STRINGS: OnceLock<HashMap<String, String>> = OnceLock::new();
    STRINGS.get_or_init(|| serde_json::from_str(include_str!("../../../../app/lang/en.json")).unwrap_or_default())
}

/// Fills %s / %d / %u / %zu / %.1f placeholders in order (the formats the
/// language files use); %% is a percent sign
pub fn format_printf(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        // Flags, width, precision, length: up to the conversion letter
        let mut spec = String::new();
        while let Some(&n) = chars.peek() {
            chars.next();
            if n.is_ascii_alphabetic() && !matches!(n, 'z' | 'l' | 'h') {
                break;
            }
            spec.push(n);
        }
        out.push_str(args.next().map(String::as_str).unwrap_or(""));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_placeholders() {
        let s = format_printf(
            "try %d/%d, %zu found, %.1f kHz, 100%%",
            &["2".into(), "3".into(), "4".into(), "48.0".into()],
        );
        assert_eq!(s, "try 2/3, 4 found, 48.0 kHz, 100%");
    }

    #[test]
    fn english_text() {
        let m = Msg::with("status.connecting_retry", &[&2, &3]);
        assert_eq!(m.english(), "Connection failed, trying again (2/3)...");
    }
}
