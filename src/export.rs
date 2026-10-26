//! Export a buffer as a standalone, syntax-highlighted HTML document (`:TOhtml`).
//!
//! The output is a self-contained page: the current theme's colors become inline
//! styles, so the file opens in any browser with the same look as the editor.

use crate::syntax::{Language, LineState, Registry};
use crate::theme::Theme;
use crossterm::style::Color;

/// A crossterm color as a CSS hex string (`#rrggbb`). Non-RGB colors (which the
/// built-in themes do not use) fall back to black.
fn hex(c: Color) -> String {
    match c {
        Color::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => "#000000".to_string(),
    }
}

/// Escape the five characters that are unsafe in HTML text content.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render `lines` as a complete HTML document, highlighted for `lang` using
/// `theme`'s colors. `title` names the page (and is shown as a heading comment).
pub fn to_html(
    title: &str,
    lines: &[String],
    lang: Language,
    theme: &Theme,
    syntax: &Registry,
) -> String {
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(title)));
    out.push_str("<style>\n");
    out.push_str(&format!(
        "body {{ background: {}; }}\n",
        hex(theme.bg)
    ));
    out.push_str(&format!(
        "pre.rvim {{ background: {}; color: {}; font-family: ui-monospace, Consolas, \
         \"DejaVu Sans Mono\", monospace; font-size: 13px; line-height: 1.4; padding: 16px; \
         border-radius: 6px; overflow-x: auto; }}\n",
        hex(theme.bg),
        hex(theme.fg)
    ));
    out.push_str("</style>\n</head>\n<body>\n<pre class=\"rvim\">");

    let mut state = LineState::Normal;
    for line in lines {
        let (tokens, next) = syntax.highlight_stateful(lang, line, state);
        state = next;
        let mut col = 0usize;
        for tok in &tokens {
            // Plain (unclassified) text before this token.
            if tok.start > col {
                out.push_str(&escape(&line[col..tok.start]));
            }
            let color = hex(theme.token_color(tok.kind));
            out.push_str(&format!("<span style=\"color:{color}\">"));
            out.push_str(&escape(&line[tok.start..tok.end]));
            out.push_str("</span>");
            col = tok.end;
        }
        if col < line.len() {
            out.push_str(&escape(&line[col..]));
        }
        out.push('\n');
    }

    out.push_str("</pre>\n</body>\n</html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeRegistry;

    #[test]
    fn escapes_html_special_characters() {
        assert_eq!(escape("a < b && c > d \"x\""), "a &lt; b &amp;&amp; c &gt; d &quot;x&quot;");
    }

    #[test]
    fn hex_formats_rgb() {
        assert_eq!(hex(Color::Rgb { r: 255, g: 0, b: 16 }), "#ff0010");
    }

    #[test]
    fn to_html_wraps_document_and_highlights_tokens() {
        let syntax = Registry::with_builtins();
        let theme = ThemeRegistry::with_builtins().current().clone();
        let lines = vec!["fn main() {}".to_string()];
        let html = to_html("demo.rs", &lines, Language::Rust, &theme, &syntax);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>demo.rs</title>"));
        assert!(html.contains("<pre class=\"rvim\">"));
        assert!(html.contains("</pre>"));
        // `fn` is a keyword, so at least one colored span is emitted.
        assert!(html.contains("<span style=\"color:#"));
    }

    #[test]
    fn to_html_escapes_buffer_content() {
        let syntax = Registry::with_builtins();
        let theme = ThemeRegistry::with_builtins().current().clone();
        let lines = vec!["let x = a < b && c;".to_string()];
        let html = to_html("t.rs", &lines, Language::Rust, &theme, &syntax);
        // Raw angle brackets from the source never appear unescaped in the body.
        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;&amp;"));
    }
}
