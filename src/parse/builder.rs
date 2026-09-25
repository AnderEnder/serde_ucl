//! A by-value builder for [`Parser`].

use super::{Loader, MacroCall, MacroError, Parser};
use crate::value::{DuplicateStrategy, ParserFlags};
use std::path::PathBuf;

/// Builds a [`Parser`] from settings given by value, one method call each.
///
/// Every method sets what the [`Parser`] setter of the same name sets; [`ParserBuilder::build`]
/// returns the parser. A builder starts from [`Parser::new`]: no flags, priority 0, the `append`
/// strategy, no registered variables, no variable handler, no base directory, and a loader that
/// holds no files. [`ParserBuilder::with_loader`] opts into file access, for example with
/// [`FsLoader`](super::FsLoader) for the filesystem.
///
/// ```
/// use serde::Deserialize;
/// use ucl_lexer::parse::{MemoryLoader, ParserBuilder};
/// use ucl_lexer::{ParserFlags, from_value};
///
/// #[derive(Deserialize)]
/// struct Config {
///     url: String,
///     home: String,
///     port: u16,
/// }
///
/// let mut files = MemoryLoader::new();
/// files.add_file("/etc/app/port.conf", "port = 8443\n");
/// let mut parser = ParserBuilder::new()
///     .with_flags(ParserFlags::KEY_LOWERCASE)
///     .with_variable("HOST", "example.org")
///     .with_variable_handler(|name| (name == "HOME").then(|| "/home/app".to_string()))
///     .with_loader(files)
///     .with_base_dir("/etc/app")
///     .build();
/// let input = b"URL = \"https://$HOST/\"\nhome = \"${HOME}\"\n.include \"port.conf\"";
/// let value = parser.parse(input)?;
/// let config: Config = from_value(value)?;
/// assert_eq!(config.url, "https://example.org/");
/// assert_eq!(config.home, "/home/app");
/// assert_eq!(config.port, 8443);
/// # Ok::<(), ucl_lexer::UclError>(())
/// ```
#[derive(Debug, Default)]
pub struct ParserBuilder {
    parser: Parser,
}

impl ParserBuilder {
    /// A builder with the settings of [`Parser::new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the parser flags ([`Parser::set_flags`]).
    pub fn with_flags(mut self, flags: ParserFlags) -> Self {
        self.parser.set_flags(flags);
        self
    }

    /// Sets the priority of the document's values ([`Parser::set_priority`], spec §8.3).
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.parser.set_priority(priority);
        self
    }

    /// Sets how repeated keys are resolved ([`Parser::set_strategy`], spec §8.4).
    pub fn with_strategy(mut self, strategy: DuplicateStrategy) -> Self {
        self.parser.set_strategy(strategy);
        self
    }

    /// Registers a variable ([`Parser::register_variable`], spec §7.1).
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.parser.register_variable(name, value);
        self
    }

    /// Registers each variable of `variables`, in iteration order ([`Parser::register_variable`]).
    /// The order decides between names that are prefixes of one another in unbraced references
    /// (spec §7.4).
    pub fn with_variables<I, K, V>(mut self, variables: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (name, value) in variables {
            self.parser.register_variable(name, value);
        }
        self
    }

    /// Installs a handler for braced references `${NAME}` to names that are not registered
    /// ([`Parser::set_variable_handler`], spec §7.7). It is never asked for unbraced references.
    pub fn with_variable_handler(
        mut self,
        handler: impl FnMut(&str) -> Option<String> + 'static,
    ) -> Self {
        self.parser.set_variable_handler(handler);
        self
    }

    /// Registers a macro under `name` ([`Parser::register_macro`], spec §13.2).
    pub fn with_macro(
        mut self,
        name: impl Into<String>,
        handler: impl Fn(&mut MacroCall<'_>) -> Result<(), MacroError> + 'static,
    ) -> Self {
        self.parser.register_macro(name, handler);
        self
    }

    /// Registers a context macro under `name`, whose handler also gets the root as built so far
    /// ([`Parser::register_context_macro`], spec §13.2).
    pub fn with_context_macro(
        mut self,
        name: impl Into<String>,
        handler: impl Fn(&mut MacroCall<'_>) -> Result<(), MacroError> + 'static,
    ) -> Self {
        self.parser.register_context_macro(name, handler);
        self
    }

    /// Sets where [`Parser::parse_file`], the include macros and `.load` read files from
    /// ([`Parser::set_loader`]). The default loader holds no files.
    pub fn with_loader(mut self, loader: impl Loader + 'static) -> Self {
        self.parser.set_loader(loader);
        self
    }

    /// Sets the directory that relative paths resolve against ([`Parser::set_base_dir`]).
    pub fn with_base_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.parser.set_base_dir(dir);
        self
    }

    /// Sets the search directories of the include macros, in effect from the start of every
    /// parse as a `path` list would be ([`Parser::set_search_path`], spec §9.4).
    pub fn with_search_path<I, S>(mut self, dirs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.parser.set_search_path(dirs);
        self
    }

    /// Sets the most bytes one parse reads, the document and the files its macros read together
    /// ([`Parser::set_max_input_bytes`]). The default is no limit.
    pub fn with_max_input_bytes(mut self, limit: u64) -> Self {
        self.parser.set_max_input_bytes(Some(limit));
        self
    }

    /// The parser.
    pub fn build(self) -> Parser {
        self.parser
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::MemoryLoader;
    use crate::value::UclValue;

    #[test]
    fn builder_sets_what_the_setters_set() {
        let mut loader = MemoryLoader::new();
        loader.add_file("/cfg/part.conf", "b = 2\n");
        let mut parser = ParserBuilder::new()
            .with_flags(ParserFlags::KEY_LOWERCASE)
            .with_priority(3)
            .with_strategy(DuplicateStrategy::Rewrite)
            .with_variables([("A", "x"), ("AB", "y")])
            .with_variable("C", "z")
            .with_variable_handler(|name| (name == "H").then(|| "h".to_string()))
            .with_loader(loader)
            .with_base_dir("/cfg")
            .with_search_path(["/cfg"])
            .with_max_input_bytes(1000)
            .build();
        assert_eq!(parser.flags(), ParserFlags::KEY_LOWERCASE);
        assert_eq!(parser.base_dir(), Some(std::path::Path::new("/cfg")));
        assert_eq!(parser.search_path(), Some(&["/cfg".to_string()][..]));
        assert_eq!(parser.max_input_bytes(), Some(1000));
        let v = parser
            .parse(b"K = \"$AB ${C} ${H}\"\nk = 2\n.include \"part.conf\"")
            .unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o["k"], UclValue::Integer(2));
        assert_eq!(o.entry("k").unwrap().len(), 1);
        assert_eq!(o.entry("k").unwrap().slots()[0].priority(), 3);
        assert_eq!(o["b"], UclValue::Integer(2));
        let mut parser = ParserBuilder::new()
            .with_variables([("A", "x"), ("AB", "y")])
            .with_variable("C", "z")
            .with_variable_handler(|name| (name == "H").then(|| "h".to_string()))
            .build();
        // `A` is registered first, so `$AB` matches it (spec §7.4); the handler answers only
        // braced names that are not registered (§7.7).
        let v = parser.parse(b"s = \"$AB ${C} ${H} ${I}\"").unwrap();
        assert_eq!(v.as_object().unwrap()["s"].as_str(), Some("xB z h ${I}"));
    }
}
