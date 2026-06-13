//! Hand-written syntax highlighter for Rust / bash / TOML / SQL.
//!
//! Emits HTML with `<span class="kw">`, `<span class="st">`,
//! `<span class="cm">`, `<span class="nm">` wrappings matching the
//! CSS classes defined in [`super::css`]. **Not** a full lexer —
//! covers keywords + string literals + comments + simple numbers
//! to make code blocks readable; unknown tokens fall through as
//! plain HTML-escaped text.
//!
//! Trade-off: no `syntect` / `tree-sitter` dependency. Misses are
//! acceptable — the worst case is missing color, not malformed HTML.

// Pre-T8 the only callers live in `#[cfg(test)]`; once `pages.rs`
// (T8+) wires lessons into the renderer the public surface is hit
// from non-test code and these allows become inert.
#![allow(dead_code)]

/// Languages supported by [`highlight`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Rust source — `//` line comments, double-quoted strings, [`RUST_KW`] keywords.
    Rust,
    /// POSIX shell / bash — `#` line comments, single + double-quoted strings, [`BASH_KW`] keywords.
    Bash,
    /// TOML configuration — `[section]` headers, `#` comments, strings, integer values.
    Toml,
    /// SQL — `--` line comments, [`SQL_KW`] uppercase keywords.
    Sql,
}

const RUST_KW: &[&str] = &[
    "fn", "let", "mut", "pub", "mod", "use", "struct", "enum", "impl", "trait", "match", "if",
    "else", "for", "while", "loop", "return", "async", "await", "Self", "self", "where", "type",
    "const", "static", "ref", "move", "as", "in", "break", "continue", "true", "false",
];

const BASH_KW: &[&str] = &[
    "if", "then", "fi", "else", "elif", "for", "in", "do", "done", "while", "case", "esac",
    "function", "return", "exit", "echo",
];

const SQL_KW: &[&str] = &[
    "SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AND", "OR", "NOT",
    "NULL", "CREATE", "TABLE", "INDEX", "UPDATE", "DELETE", "INSERT", "VALUES", "PRAGMA", "ORDER",
    "BY", "GROUP", "HAVING", "LIMIT", "OFFSET", "DISTINCT", "AS",
];

/// HTML-escape a substring (only `&`, `<`, `>`; quotes are fine in PCDATA).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render `src` (any of the 4 languages) as HTML with syntax-class spans.
///
/// Output is always well-formed HTML — unknown tokens fall through
/// HTML-escaped without a wrapping `<span>`.
///
/// # Examples
///
/// ```text
/// let html = highlight(Lang::Rust, "fn main() {}");
/// assert!(html.contains(r#"<span class="kw">fn</span>"#));
/// ```
#[must_use]
pub fn highlight(lang: Lang, src: &str) -> String {
    match lang {
        Lang::Rust => highlight_curly(src, RUST_KW),
        Lang::Bash => highlight_shell(src, BASH_KW, '#'),
        Lang::Toml => highlight_toml(src),
        Lang::Sql => highlight_shell(src, SQL_KW, '-'),
    }
}

/// Rust-style: `//` line comments, double-quoted strings, keywords from `kws`, integer literals.
fn highlight_curly(src: &str, kws: &[&str]) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() * 2);
    let mut i = 0;
    while i < bytes.len() {
        let c = char::from(bytes[i]);
        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let end = bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |p| i + p);
            out.push_str(r#"<span class="cm">"#);
            out.push_str(&esc(&src[i..end]));
            out.push_str("</span>");
            i = end;
            continue;
        }
        if c == '"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push_str(r#"<span class="st">"#);
            out.push_str(&esc(&src[start..i]));
            out.push_str("</span>");
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &src[start..i];
            if kws.contains(&ident) {
                out.push_str(r#"<span class="kw">"#);
                out.push_str(ident);
                out.push_str("</span>");
            } else {
                out.push_str(&esc(ident));
            }
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            out.push_str(r#"<span class="nm">"#);
            out.push_str(&src[start..i]);
            out.push_str("</span>");
            continue;
        }
        let ch_len = src[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&esc(&src[i..i + ch_len]));
        i += ch_len;
    }
    out
}

/// Shell / SQL: `#`-or-`--` line comments, double + single string,
/// keyword list. `comment_lead` is the first char of the comment
/// marker (`#` for bash, `-` for sql which then requires a second `-`).
fn highlight_shell(src: &str, kws: &[&str], comment_lead: char) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len() * 2);
    let mut i = 0;
    while i < bytes.len() {
        let c = char::from(bytes[i]);
        let is_comment = if comment_lead == '#' {
            c == '#'
        } else {
            c == '-' && i + 1 < bytes.len() && bytes[i + 1] == b'-'
        };
        if is_comment {
            let end = bytes[i..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |p| i + p);
            out.push_str(r#"<span class="cm">"#);
            out.push_str(&esc(&src[i..end]));
            out.push_str("</span>");
            i = end;
            continue;
        }
        if c == '"' || c == '\'' {
            let quote = bytes[i];
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            out.push_str(r#"<span class="st">"#);
            out.push_str(&esc(&src[start..i]));
            out.push_str("</span>");
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &src[start..i];
            if kws.contains(&ident) {
                out.push_str(r#"<span class="kw">"#);
                out.push_str(ident);
                out.push_str("</span>");
            } else {
                out.push_str(&esc(ident));
            }
            continue;
        }
        let ch_len = src[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&esc(&src[i..i + ch_len]));
        i += ch_len;
    }
    out
}

/// TOML: `[section]` headers (whole-line keyword), `# comment`,
/// double-quoted strings, integer numbers.
fn highlight_toml(src: &str) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    for line in src.split_inclusive('\n') {
        let trim = line.trim_start();
        let lead = line.len() - trim.len();
        out.push_str(&line[..lead]);
        if trim.starts_with('[') {
            if let Some(close) = trim.find(']') {
                out.push_str(r#"<span class="kw">"#);
                out.push_str(&esc(&trim[..=close]));
                out.push_str("</span>");
                out.push_str(&esc(&trim[close + 1..]));
                continue;
            }
        }
        // Delegate the rest of the line to a stripped-down shell-ish pass
        // for comments + strings (numbers handled in a second pass below).
        let tail = highlight_shell(trim, &[], '#');
        out.push_str(&tail);
    }
    // Numbers: a coarse pass — wrap standalone integer values right of '='
    // (good-enough for our doc examples; not a real TOML parser).
    let mut numbered = String::with_capacity(out.len());
    let bytes = out.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            numbered.push('=');
            i += 1;
            while i < bytes.len() && bytes[i] == b' ' {
                numbered.push(' ');
                i += 1;
            }
            if i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'-') {
                let start = i;
                if bytes[i] == b'-' {
                    i += 1;
                }
                while i < bytes.len()
                    && (bytes[i].is_ascii_digit() || bytes[i] == b'_' || bytes[i] == b'.')
                {
                    i += 1;
                }
                numbered.push_str(r#"<span class="nm">"#);
                numbered.push_str(&out[start..i]);
                numbered.push_str("</span>");
                continue;
            }
        }
        // ASCII-safe: tag bytes we emit are ASCII; non-ASCII bytes from `src`
        // are multibyte UTF-8 and require careful handling.
        let b = bytes[i];
        if b.is_ascii() {
            numbered.push(char::from(b));
            i += 1;
        } else {
            let ch_len = out[i..].chars().next().map_or(1, char::len_utf8);
            numbered.push_str(&out[i..i + ch_len]);
            i += ch_len;
        }
    }
    numbered
}
