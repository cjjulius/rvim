//! A minimal but real plugin system.
//!
//! Plugins register ex-commands (e.g. `:wordcount`). They receive a read-only
//! [`PluginDoc`] snapshot of the buffer and return a [`PluginResponse`]. This
//! keeps plugins decoupled from the editor's internals while still being useful;
//! the API can grow (mutation hooks, event subscriptions) without breaking the
//! trait's existing methods.

use crate::buffer::Position;

/// A read-only view of the document handed to plugins.
pub struct PluginDoc<'a> {
    pub lines: &'a [String],
    pub cursor: Position,
    pub language: &'a str,
    pub path: Option<&'a str>,
}

/// What a plugin returns after handling (or declining) a command.
#[derive(Debug, Default)]
pub struct PluginResponse {
    /// Whether the plugin consumed the command.
    pub handled: bool,
    /// An optional message to show on the command line.
    pub message: Option<String>,
}

impl PluginResponse {
    pub fn handled(message: impl Into<String>) -> Self {
        Self {
            handled: true,
            message: Some(message.into()),
        }
    }
    pub fn ignored() -> Self {
        Self {
            handled: false,
            message: None,
        }
    }
}

/// The trait every plugin implements.
pub trait Plugin {
    /// A unique plugin name.
    fn name(&self) -> &str;
    /// The ex-commands this plugin provides (without the leading `:`), used for
    /// `:help` and discovery.
    fn commands(&self) -> Vec<&str> {
        Vec::new()
    }
    /// Handle an ex-command. Return `handled(..)` if consumed.
    fn on_command(&mut self, _name: &str, _args: &str, _doc: &PluginDoc) -> PluginResponse {
        PluginResponse::ignored()
    }
}

/// Owns and dispatches to the registered plugins.
#[derive(Default)]
pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// A manager preloaded with the built-in example plugins.
    pub fn with_builtins() -> Self {
        let mut m = Self::new();
        m.register(Box::new(WordCountPlugin));
        m
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(plugin);
    }

    /// Names of all registered plugins.
    pub fn plugin_names(&self) -> Vec<&str> {
        self.plugins.iter().map(|p| p.name()).collect()
    }

    /// All commands provided by all plugins.
    pub fn all_commands(&self) -> Vec<&str> {
        self.plugins.iter().flat_map(|p| p.commands()).collect()
    }

    /// Offer a command to each plugin until one handles it.
    pub fn dispatch(&mut self, name: &str, args: &str, doc: &PluginDoc) -> Option<PluginResponse> {
        for plugin in self.plugins.iter_mut() {
            let resp = plugin.on_command(name, args, doc);
            if resp.handled {
                return Some(resp);
            }
        }
        None
    }
}

/// Example plugin: `:wordcount` reports lines / words / characters.
pub struct WordCountPlugin;

impl Plugin for WordCountPlugin {
    fn name(&self) -> &str {
        "wordcount"
    }
    fn commands(&self) -> Vec<&str> {
        vec!["wordcount", "wc"]
    }
    fn on_command(&mut self, name: &str, _args: &str, doc: &PluginDoc) -> PluginResponse {
        if name != "wordcount" && name != "wc" {
            return PluginResponse::ignored();
        }
        let lines = doc.lines.len();
        let words: usize = doc.lines.iter().map(|l| l.split_whitespace().count()).sum();
        let chars: usize = doc.lines.iter().map(|l| l.chars().count()).sum();
        PluginResponse::handled(format!("{lines} lines, {words} words, {chars} chars"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(lines: &'a [String]) -> PluginDoc<'a> {
        PluginDoc {
            lines,
            cursor: Position::default(),
            language: "text",
            path: None,
        }
    }

    #[test]
    fn wordcount_reports_counts() {
        let lines = vec!["hello world".to_string(), "foo".to_string()];
        let mut mgr = PluginManager::with_builtins();
        let resp = mgr.dispatch("wordcount", "", &doc(&lines)).unwrap();
        assert!(resp.handled);
        assert_eq!(resp.message.unwrap(), "2 lines, 3 words, 14 chars");
    }

    #[test]
    fn unknown_command_not_handled() {
        let lines = vec!["x".to_string()];
        let mut mgr = PluginManager::with_builtins();
        assert!(mgr.dispatch("nope", "", &doc(&lines)).is_none());
    }

    #[test]
    fn builtin_commands_listed() {
        let mgr = PluginManager::with_builtins();
        assert!(mgr.all_commands().contains(&"wordcount"));
    }
}
