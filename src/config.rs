//! Startup configuration via an `rvimrc` file of ex-commands.
//!
//! Each non-blank, non-comment line is treated as an ex-command (with any
//! leading `:` stripped) and run through the normal command pipeline, so the
//! config file supports exactly the same commands as the `:` line — e.g.:
//!
//! ```text
//! " ~/.rvimrc
//! theme cobalt
//! set number
//! set mouse
//! ```
//!
//! Comments start with `"` (vim-style) or `#`.

use std::path::PathBuf;

/// Parse config text into a list of ex-command strings (without leading `:`),
/// skipping blank lines and comments.
pub fn parse_config(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('"') && !l.starts_with('#'))
        .map(|l| l.trim_start_matches(':').trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// The default config path: `$RVIMRC` if set, else `~/.rvimrc`
/// (`$HOME` on Unix, `%USERPROFILE%` on Windows). `None` if no home is known.
pub fn default_config_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("RVIMRC") {
        return Some(PathBuf::from(explicit));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".rvimrc"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_comments_and_blanks() {
        let text = "\
            \" a vim-style comment\n\
            # a hash comment\n\
            \n\
            theme cobalt\n\
            set number\n";
        assert_eq!(parse_config(text), vec!["theme cobalt", "set number"]);
    }

    #[test]
    fn strips_leading_colon_and_whitespace() {
        let text = "   :set nonumber  \n:theme matrix";
        assert_eq!(parse_config(text), vec!["set nonumber", "theme matrix"]);
    }

    #[test]
    fn empty_text_yields_no_commands() {
        assert!(parse_config("").is_empty());
        assert!(parse_config("\n  \n\" only a comment\n").is_empty());
    }

    #[test]
    fn env_override_is_respected() {
        // RVIMRC takes precedence when set.
        std::env::set_var("RVIMRC", "/tmp/custom-rvimrc");
        assert_eq!(
            default_config_path(),
            Some(PathBuf::from("/tmp/custom-rvimrc"))
        );
        std::env::remove_var("RVIMRC");
    }
}
