//! Macros registered by the application (spec §13.2).
//!
//! A registered macro is recognised where a built-in macro is, by the same rules (§9.1, §9.2), in
//! every input of the parser and in the files they include, but not in macro argument documents,
//! which know only the built-in macros. Its name matches exactly and case-sensitively, and
//! replaces a built-in macro of the same name. `disable-macro` (§12.6) makes it an error like
//! every macro.
//!
//! Its handler gets a [`MacroCall`] for each use: the macro's name, its VALUE as text with
//! variables expanded, its ARGUMENTS parsed as a document, and for a context macro the root as
//! built so far. It can add entries to the innermost open object ([`MacroCall::add`]) and have
//! text parsed in place of the macro ([`MacroCall::parse`]). It returns `Ok(())`, a silent stop
//! ([`MacroError::stop`]) as libucl's failing handlers give, or an error with a message
//! ([`MacroError::new`]), which libucl has no way to report (WORKLIST C8b decision 3).

use super::Error;
use crate::value::UclValue;
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// The signature of a registered macro's handler: see [`MacroCall`] for what it receives and
/// can do, and [`MacroError`] for how it fails.
///
/// Handlers are `Fn`, so that a macro can run again inside text its own handler has parsed in
/// place; a handler that keeps state uses a [`Cell`] or a [`std::cell::RefCell`].
pub type MacroHandler = dyn Fn(&mut MacroCall<'_>) -> Result<(), MacroError>;

/// A registered macro: its handler, and whether it receives the root (a context macro).
#[derive(Clone)]
pub(crate) struct Registered {
    pub(crate) handler: Rc<MacroHandler>,
    pub(crate) context: bool,
}

/// The macros registered with a parser, by name.
#[derive(Clone, Default)]
pub(crate) struct MacroTable {
    macros: HashMap<String, Registered>,
    /// Some handler ran in the last parse ([`super::Parser::parse_located`] does not repeat
    /// such a parse).
    pub(crate) ran: Cell<bool>,
}

impl MacroTable {
    pub(crate) fn insert(&mut self, name: String, macro_: Registered) {
        self.macros.insert(name, macro_);
    }

    /// The macro registered under `name`, compared byte for byte (spec §13.2). The empty name,
    /// which no macro in a document has (§9.2), matches nothing.
    pub(crate) fn get(&self, name: &[u8]) -> Option<&Registered> {
        if name.is_empty() {
            return None;
        }
        std::str::from_utf8(name)
            .ok()
            .and_then(|name| self.macros.get(name))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.macros.is_empty()
    }

    /// The registered names, sorted.
    pub(crate) fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.macros.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }
}

/// What a handler's [`MacroCall`] does to the parse, through the parser core.
pub(crate) trait Host {
    /// Adds `value` under `key` to the innermost open object, with `priority`, as a further
    /// value of an existing key (spec §13.2, *Add entries*). `at` is the macro.
    fn add_entry(
        &mut self,
        key: String,
        value: UclValue,
        priority: u8,
        at: usize,
    ) -> Result<(), MacroError>;

    /// Parses `text` in place of the macro at `at` (spec §13.2, *Have text parsed in place*).
    fn parse_text(&mut self, text: &[u8], at: usize) -> Result<(), Error>;
}

/// One use of a registered macro, as its handler sees it (spec §13.2).
///
/// ```
/// use serde_ucl::parse::{MacroError, ParserBuilder};
/// use serde_ucl::UclValue;
///
/// let mut parser = ParserBuilder::new()
///     // `.env NAME`: the entry NAME from a fixed table.
///     .with_macro("env", |call| {
///         let name = call.value_str().ok_or_else(|| MacroError::new("not UTF-8"))?;
///         match name {
///             "HOME" => call.add("HOME", UclValue::String("/home/app".into())),
///             _ => Err(MacroError::new(format!("unknown variable {name}"))),
///         }
///     })
///     // `.defaults {}`: text parsed in place of the macro.
///     .with_macro("defaults", |call| call.parse("workers = 4; log { level = info }"))
///     .build();
/// let value = parser.parse(b"server {\n  .env HOME\n  .defaults {}\n  workers = 8\n}")?;
/// let server = value.as_object().unwrap()["server"].as_object().unwrap();
/// assert_eq!(server["HOME"].as_str(), Some("/home/app"));
/// let workers: Vec<i64> = server.get_all("workers").filter_map(UclValue::as_integer).collect();
/// assert_eq!(workers, [4, 8]);
/// # Ok::<(), serde_ucl::parse::Error>(())
/// ```
pub struct MacroCall<'a> {
    host: &'a mut dyn Host,
    name: &'a str,
    value: &'a [u8],
    arguments: Option<&'a UclValue>,
    root: Option<(&'a UclValue, u8)>,
    at: usize,
    /// The error of text parsed in place that was rejected or stopped: it decides the outcome
    /// of the macro, whatever the handler returns.
    text_error: Option<Error>,
}

impl<'a> MacroCall<'a> {
    pub(crate) fn new(
        host: &'a mut dyn Host,
        name: &'a str,
        value: &'a [u8],
        arguments: Option<&'a UclValue>,
        root: Option<(&'a UclValue, u8)>,
        at: usize,
    ) -> Self {
        Self {
            host,
            name,
            value,
            arguments,
            root,
            at,
            text_error: None,
        }
    }

    /// The error of text parsed in place that failed or stopped, if any.
    pub(crate) fn take_text_error(&mut self) -> Option<Error> {
        self.text_error.take()
    }

    /// The macro's name, as registered.
    pub fn name(&self) -> &str {
        self.name
    }

    /// The macro's VALUE as text (spec §9.2, §13.2): for `"…"` the bytes between the quotes,
    /// escapes not decoded; for `{…}` the bytes from the first that is not whitespace up to the
    /// `}`, trailing whitespace kept; otherwise the bytes up to the end of the value, trailing
    /// spaces kept. Variables are expanded (§7). An empty VALUE is empty.
    pub fn value(&self) -> &[u8] {
        self.value
    }

    /// [`MacroCall::value`] as a string, if it is UTF-8.
    pub fn value_str(&self) -> Option<&str> {
        std::str::from_utf8(self.value).ok()
    }

    /// The macro's ARGUMENTS, the text between its parentheses parsed as a document of its own
    /// (spec §9.2): an object, or an array when the arguments are written as one; `()` gives an
    /// empty object. `None` when the macro has no parentheses. The names are as written (no
    /// parameter table applies), lowercased under `key-lowercase`; repeated names give
    /// multi-value entries; built-in macros work in it, registered ones do not.
    pub fn arguments(&self) -> Option<&UclValue> {
        self.arguments
    }

    /// For a macro registered with [`super::Parser::register_context_macro`], a copy of the root
    /// as built so far when the handler was called, with the containers still open and the
    /// entries they had then, also when the macro stands in an included file (spec §13.2).
    /// `None` for a macro registered with [`super::Parser::register_macro`].
    pub fn root(&self) -> Option<&UclValue> {
        self.root.map(|(root, _)| root)
    }

    /// For a macro registered with [`super::Parser::register_context_macro`], the priority of
    /// the root: that of the first input, which creates the root, whatever `.priority` says
    /// later (spec §13.2, *The root's priority*). It has no effect on the result (§8.7), but a
    /// copy of the root keeps it, as libucl's does: give it to
    /// [`MacroCall::add_with_priority`] to add such a copy. `None` for a macro registered with
    /// [`super::Parser::register_macro`].
    ///
    /// ```
    /// use serde_ucl::parse::Parser;
    ///
    /// let mut parser = Parser::new();
    /// parser.set_priority(3);
    /// parser.register_context_macro("snapshot", |call| {
    ///     let root = call.root().cloned().expect("a context macro gets the root");
    ///     let priority = call.root_priority().expect("and its priority");
    ///     call.add_with_priority("snapshot", root, priority)
    /// });
    /// let value = parser.parse(b".priority 7\na = 1\n.snapshot {}")?;
    /// let entry = value.as_object().unwrap().entry("snapshot").unwrap();
    /// assert_eq!(entry.slots()[0].priority(), 3);
    /// # Ok::<(), serde_ucl::parse::Error>(())
    /// ```
    pub fn root_priority(&self) -> Option<u8> {
        self.root.map(|(_, priority)| priority)
    }

    /// Adds `value` under `key` to the innermost open object, where the macro stands, in order
    /// with the entries around it (spec §13.2, *Add entries*). The duplicate rules of §8 do not
    /// apply: the value has priority 0 and joins an existing entry `key` as a further value,
    /// whatever the input's priority and strategy and `no-implicit-arrays` say. The key is used
    /// as given, also under `key-lowercase`. Values added so are not values created for saved
    /// comments (§12.5).
    ///
    /// Fails when no object is open, after text parsed in place closed the braced root, and when
    /// `value` would nest containers deeper than the limit of spec §11.2.
    pub fn add(&mut self, key: impl Into<String>, value: UclValue) -> Result<(), MacroError> {
        self.host.add_entry(key.into(), value, 0, self.at)
    }

    /// [`MacroCall::add`] with the value at `priority` (0 to 15; higher bits are ignored, as in
    /// spec §8.3) instead of 0. The duplicate rules still do not apply when it is added; the
    /// priority takes part when a later value of `key` is inserted (§8.3). A copy of the root
    /// that keeps the root's priority ([`MacroCall::root_priority`]) is added this way.
    pub fn add_with_priority(
        &mut self,
        key: impl Into<String>,
        value: UclValue,
        priority: u8,
    ) -> Result<(), MacroError> {
        let priority = priority & crate::value::MAX_PRIORITY;
        self.host.add_entry(key.into(), value, priority, self.at)
    }

    /// Parses `text` in place of the macro (spec §13.2, *Have text parsed in place*), as the
    /// content of an included file is read (§9.4): its entries go into the innermost open
    /// object, a leading `{` takes over the brace of that object, the text must close the
    /// brackets it opens and hold complete entries, and it counts as one more open input unit
    /// for the limit of [`super::MAX_INCLUDE_DEPTH`]. Unlike an included file, it takes the
    /// priority and the duplicate strategy in effect where the macro stands, `.priority`
    /// included, and sets no file variables; `.priority` in the text applies to the rest of the
    /// text only. Its comments are saved as the input's are. Registered macros work in it.
    ///
    /// If the text is rejected, the error is returned, with its position in `text`, and the parse
    /// fails with it, reported at the macro, whatever the handler then returns. If the text stops
    /// silently (§9.4), the parse stops at the macro in the same way. A later call returns the
    /// same error without parsing.
    pub fn parse(&mut self, text: impl AsRef<[u8]>) -> Result<(), MacroError> {
        if let Some(error) = &self.text_error {
            return Err(MacroError(Repr::Text(Box::new(error.clone()))));
        }
        match self.host.parse_text(text.as_ref(), self.at) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.text_error = Some(error.clone());
                Err(MacroError(Repr::Text(Box::new(error))))
            }
        }
    }
}

impl fmt::Debug for MacroCall<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroCall")
            .field("name", &self.name)
            .field("value", &String::from_utf8_lossy(self.value))
            .field("arguments", &self.arguments)
            .field("root", &self.root.is_some())
            .field("root_priority", &self.root_priority())
            .finish()
    }
}

/// How a registered macro's handler fails (spec §13.2, *Fail*).
///
/// - [`MacroError::stop`] ends the parse silently at the macro, as libucl does for any handler
///   that fails: the entries parsed so far are the result, in every open unit of the input, and
///   later inputs are still read (§13.1). The parse reports [`super::ErrorKind::MacroStopped`],
///   for which [`super::Error::is_stopped`] is true and whose [`super::Error::partial`] holds
///   that result.
/// - [`MacroError::new`] fails the parse with [`super::ErrorKind::MacroFailed`], the macro's
///   name and the message, at the macro. libucl has no way to report an error; this is an
///   addition (WORKLIST C8b decision 3).
/// - The error [`MacroCall::parse`] returns stands for the text's own error: returned by the
///   handler, the parse fails or stops with that. The text's outcome wins over whatever the
///   handler returns.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroError(pub(crate) Repr);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Repr {
    Stop,
    Failed(String),
    Text(Box<Error>),
}

impl MacroError {
    /// A silent stop at the macro (spec §13.2).
    pub fn stop() -> Self {
        Self(Repr::Stop)
    }

    /// An error with `message`, reported as [`super::ErrorKind::MacroFailed`].
    pub fn new(message: impl Into<String>) -> Self {
        Self(Repr::Failed(message.into()))
    }

    /// True for [`MacroError::stop`], and for the error of text parsed in place that stopped
    /// silently.
    pub fn is_stop(&self) -> bool {
        match &self.0 {
            Repr::Stop => true,
            Repr::Failed(_) => false,
            Repr::Text(error) => error.is_stopped(),
        }
    }

    /// For the error [`MacroCall::parse`] returns, the text's error, with its position in the
    /// text.
    pub fn parse_error(&self) -> Option<&Error> {
        match &self.0 {
            Repr::Text(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for MacroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Repr::Stop => f.write_str("the macro stops the parse"),
            Repr::Failed(message) => f.write_str(message),
            Repr::Text(error) => write!(f, "text parsed in place of the macro: {error}"),
        }
    }
}

impl std::error::Error for MacroError {}
