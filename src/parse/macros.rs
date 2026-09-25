//! Macros (spec §9): the syntax every macro shares (§9.2), parameter matching, and the macros
//! `.priority` (§9.5) and `.inherit` (§9.7). The include macros and `.load` are in
//! `super::include`.
//!
//! A macro is read in full before it runs: `.NAME`, optional `(ARGUMENTS)` and VALUE. The
//! arguments are a UCL document of their own, parsed with the same flags, priority 0 and the
//! `append` strategy, and the file variables of a document given as bytes (§7.8). Its root
//! object's entries are the parameters; each macro matches them against its table
//! ([`MacroKind::parameters`]). After the macro has run, whitespace, line breaks and `;` are
//! skipped (§9.2).
//!
//! `.includes`, and `sign=true` on the include macros, are rejected with
//! [`ErrorKind::Unsupported`]: the crate never verifies signatures (project decision). So is
//! `.load` when the crate is built without its `load` feature.

use super::core::{Core, Settings, is_space, parse_nested};
use super::error::position_at;
use super::string;
use super::vars::Expander;
use super::{Error, ErrorKind, MAX_ARGUMENT_DEPTH, PathSegment};
use crate::value::{DuplicateStrategy, Entry, ParserFlags, Slot, UclObject, UclValue};

/// The macros of §9.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacroKind {
    Include,
    TryInclude,
    Includes,
    Priority,
    Load,
    Inherit,
}

/// The type a parameter's value must have (§9.2). A value of another type is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParamType {
    /// One of the keywords `true`, `false`, `yes`, `no`, `on`, `off`.
    Bool,
    /// A string, quoted or unquoted.
    String,
    /// An integer; multipliers are allowed.
    Int,
    /// An explicit array.
    Array,
}

/// One entry of a macro's parameter table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Param {
    pub(crate) name: &'static str,
    pub(crate) ty: ParamType,
}

const fn param(name: &'static str, ty: ParamType) -> Param {
    Param { name, ty }
}

/// `.include`, `.try_include` and `.includes` (§9.4), in matching order within each type.
const INCLUDE_PARAMS: &[Param] = &[
    param("try", ParamType::Bool),
    param("sign", ParamType::Bool),
    param("glob", ParamType::Bool),
    param("url", ParamType::Bool),
    param("prefix", ParamType::Bool),
    param("key", ParamType::String),
    param("target", ParamType::String),
    param("duplicate", ParamType::String),
    param("path", ParamType::Array),
    param("priority", ParamType::Int),
];

/// `.load` (§9.6), in matching order within each type.
const LOAD_PARAMS: &[Param] = &[
    param("try", ParamType::Bool),
    param("multiline", ParamType::Bool),
    param("escape", ParamType::Bool),
    param("trim", ParamType::Bool),
    param("key", ParamType::String),
    param("target", ParamType::String),
    param("priority", ParamType::Int),
];

/// `.priority` (§9.5).
const PRIORITY_PARAMS: &[Param] = &[param("priority", ParamType::Int)];

impl MacroKind {
    /// The macro called `name`, matched case-sensitively.
    pub(crate) fn from_name(name: &[u8]) -> Option<Self> {
        Some(match name {
            b"include" => Self::Include,
            b"try_include" => Self::TryInclude,
            b"includes" => Self::Includes,
            b"priority" => Self::Priority,
            b"load" => Self::Load,
            b"inherit" => Self::Inherit,
            _ => return None,
        })
    }

    /// The parameters matched by name prefix (§9.2). `.inherit` has none: its only parameter,
    /// `replace`, is matched by exact name ([`Arguments::exact_bool`]).
    pub(crate) fn parameters(self) -> &'static [Param] {
        match self {
            Self::Include | Self::TryInclude | Self::Includes => INCLUDE_PARAMS,
            Self::Load => LOAD_PARAMS,
            Self::Priority => PRIORITY_PARAMS,
            Self::Inherit => &[],
        }
    }
}

/// The parameters written in a macro's ARGUMENTS: the entries of the argument document's root
/// object. A root array, or no ARGUMENTS, gives none (§9.2).
#[derive(Debug, Default)]
pub(crate) struct Arguments {
    root: UclObject,
}

impl Arguments {
    pub(crate) fn from_root(root: UclValue) -> Self {
        match root {
            UclValue::Object(root) => Self { root },
            _ => Self::default(),
        }
    }

    /// Matches the written names against `table` (§9.2, *Quirk*). Only the first value of each
    /// entry counts. A written name matches the first parameter of its value's type, in table
    /// order, whose name it is a prefix of; a value of no parameter's type, or a name that is a
    /// prefix of none, is ignored. When several written names match one parameter, the last
    /// one wins.
    pub(crate) fn resolve(&self, table: &'static [Param]) -> Resolved<'_> {
        let mut values = vec![None; table.len()];
        for (name, entry) in self.root.iter() {
            let value = entry.first();
            let Some(ty) = param_type(value) else {
                continue;
            };
            if let Some(i) = table
                .iter()
                .position(|p| p.ty == ty && p.name.starts_with(name.as_str()))
            {
                values[i] = Some(value);
            }
        }
        Resolved { table, values }
    }

    /// The first value of the parameter written exactly `name`, if it is a bool (the `replace`
    /// parameter of `.inherit`, §9.7).
    pub(crate) fn exact_bool(&self, name: &str) -> Option<bool> {
        self.root.get(name).and_then(UclValue::as_bool)
    }
}

fn param_type(value: &UclValue) -> Option<ParamType> {
    match value {
        UclValue::Boolean(_) => Some(ParamType::Bool),
        UclValue::String(_) => Some(ParamType::String),
        UclValue::Integer(_) => Some(ParamType::Int),
        UclValue::Array(_) => Some(ParamType::Array),
        _ => None,
    }
}

/// The parameter values that apply to one macro call, by parameter name.
#[derive(Debug)]
pub(crate) struct Resolved<'a> {
    table: &'static [Param],
    values: Vec<Option<&'a UclValue>>,
}

impl<'a> Resolved<'a> {
    fn get(&self, name: &str) -> Option<&'a UclValue> {
        let i = self.table.iter().position(|p| p.name == name);
        debug_assert!(i.is_some(), "no parameter '{name}' in this table");
        i.and_then(|i| self.values[i])
    }

    pub(crate) fn bool(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(UclValue::as_bool)
    }

    pub(crate) fn string(&self, name: &str) -> Option<&'a str> {
        self.get(name).and_then(UclValue::as_str)
    }

    pub(crate) fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(UclValue::as_integer)
    }

    pub(crate) fn array(&self, name: &str) -> Option<&'a [UclValue]> {
        self.get(name)
            .and_then(UclValue::as_array)
            .map(Vec::as_slice)
    }
}

/// A macro as read from the input, before it runs.
#[derive(Debug)]
pub(crate) struct MacroCall {
    pub(crate) kind: MacroKind,
    /// The offset of the `.`.
    pub(crate) at: usize,
    pub(crate) args: Arguments,
    /// VALUE as written, with variables expanded (§9.2): the bytes between the quotes, between
    /// the braces after leading whitespace, or of the bare form. Escapes are not decoded.
    pub(crate) value: Vec<u8>,
    /// The offset where VALUE starts.
    pub(crate) value_at: usize,
}

/// The integer a `.priority` value holds (§9.5): optional leading whitespace, an optional `+`
/// or `-`, then one or more decimal digits and nothing after them. A number outside the 64-bit
/// signed range is limited to the nearest end.
pub(crate) fn decimal_integer(value: &[u8]) -> Option<i64> {
    let start = value.iter().take_while(|&&b| is_space(b)).count();
    let (negative, digits) = match &value[start..] {
        [b'-', rest @ ..] => (true, rest),
        [b'+', rest @ ..] => (false, rest),
        rest => (false, rest),
    };
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    // Magnitudes past 2^63 are all limited the same way, so stop growing there.
    let limit = i128::from(i64::MAX) + 1;
    let magnitude = digits
        .iter()
        .fold(0i128, |n, &d| (n * 10 + i128::from(d - b'0')).min(limit));
    let n = if negative { -magnitude } else { magnitude };
    Some(n.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
}

/// A priority kept modulo 16 (§8.3): `17` → 1, `-1` → 15.
pub(crate) fn priority_bits(n: i64) -> u8 {
    (n & i64::from(crate::value::MAX_PRIORITY)) as u8
}

/// Bytes that end a bare macro VALUE (§9.2).
fn ends_bare_value(b: u8) -> bool {
    matches!(b, b'\n' | b'\r' | 0 | b',' | b';' | b'#' | b']' | b'}')
}

/// The position of `name` in `object`: the key itself or, with `ignore_case`, a key that
/// differs from it only in ASCII case (§12.1).
pub(super) fn find_key(object: &UclObject, name: &str, ignore_case: bool) -> Option<usize> {
    object.index_of(name).or_else(|| {
        ignore_case
            .then(|| object.keys().position(|k| k.eq_ignore_ascii_case(name)))
            .flatten()
    })
}

impl Core<'_, '_, '_, '_> {
    /// A `.` where a key could start is a macro (§9.1): `.NAME (ARGUMENTS)? VALUE` (§9.2).
    ///
    /// - A NAME that runs to the end of input is ignored, whatever it is.
    /// - An unknown NAME is an error.
    /// - A known NAME followed by whitespace, then at most one group of comments that follow
    ///   each other directly, then the end of input, is ignored.
    /// - After ARGUMENTS, the end of input (after the same skipping) is an error.
    pub(super) fn macro_entry(&mut self) -> Result<(), Error> {
        let at = self.pos;
        if self.settings.flags.contains(ParserFlags::DISABLE_MACRO) {
            return Err(self.error(ErrorKind::MacrosDisabled, at));
        }
        let name_start = at + 1;
        let name_end = name_start
            + self.src[name_start..]
                .iter()
                .take_while(|&&b| !is_space(b) && b != b'(')
                .count();
        if name_end == self.src.len() {
            self.pos = name_end;
            return Ok(());
        }
        let name = &self.src[name_start..name_end];
        let Some(kind) = MacroKind::from_name(name) else {
            let name = String::from_utf8_lossy(name).into_owned();
            return Err(self.error(ErrorKind::UnknownMacro { name }, at));
        };
        self.pos = name_end;
        self.macro_gap()?;
        let args = if self.peek() == Some(b'(') {
            let open = self.pos;
            let end = self.macro_arguments_end(open)?;
            let args = self.macro_arguments(open, end)?;
            self.pos = end;
            self.macro_gap()?;
            if self.peek().is_none() {
                return Err(self.error(ErrorKind::MissingValue, self.pos));
            }
            args
        } else if self.peek().is_none() {
            return Ok(());
        } else {
            Arguments::default()
        };
        let value_at = self.pos;
        let value = self.macro_value()?;
        let call = MacroCall {
            kind,
            at,
            args,
            value,
            value_at,
        };
        // The macro runs before what follows it is checked: a silent stop (§9.4) ends the
        // parse at the macro, whatever comes after it (QUESTIONS.md #23).
        match call.kind {
            MacroKind::Priority => self.priority_macro(&call)?,
            MacroKind::Inherit => self.inherit_macro(&call)?,
            MacroKind::Include | MacroKind::TryInclude | MacroKind::Includes => {
                self.include_macro(&call)?
            }
            #[cfg(feature = "load")]
            MacroKind::Load => self.load_macro(&call)?,
            #[cfg(not(feature = "load"))]
            MacroKind::Load => {
                return Err(self.error(
                    ErrorKind::Unsupported {
                        feature: "the macro .load (the crate's `load` feature is off)".into(),
                    },
                    call.at,
                ));
            }
        }
        self.after_macro()?;
        self.macro_ran()
    }

    /// Skips what may stand between the parts of a macro (§9.2): whitespace, line breaks
    /// included, then at most one group of comments that follow each other directly. A `#`
    /// that is the last byte of the input where the group would start is not a comment: it
    /// ends an empty VALUE (QUESTIONS.md #23).
    fn macro_gap(&mut self) -> Result<(), Error> {
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
        if self.peek() == Some(b'#') && self.pos + 1 == self.src.len() {
            return Ok(());
        }
        self.comment_group().map(|_| ())
    }

    /// The offset after the `)` that matches the `(` at `open`. Parentheses between double
    /// quotes do not count (§9.2); as in block comments (§2.3), a `"` directly after a `\` does
    /// not begin or end a quoted part.
    fn macro_arguments_end(&self, open: usize) -> Result<usize, Error> {
        let mut depth = 0usize;
        let mut in_quotes = false;
        let mut i = open;
        while let Some(&b) = self.src.get(i) {
            match b {
                b'"' if self.src[i - 1] != b'\\' => in_quotes = !in_quotes,
                b'(' if !in_quotes => depth += 1,
                b')' if !in_quotes => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(i + 1);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        Err(self.error(ErrorKind::UnterminatedArguments, open))
    }

    /// Parses the text between the parentheses at `open` and `end - 1` as a document of its own
    /// (§9.2): the same flags, priority 0, the `append` strategy, and only the file variables of
    /// a document given as bytes (`FILENAME` is `undef`), none under `NO_FILEVARS`. It may
    /// include files, as a new parser would. Errors report positions in the enclosing input,
    /// unless they are in a file it includes; a silent stop there makes the macro fail.
    fn macro_arguments(&mut self, open: usize, end: usize) -> Result<Arguments, Error> {
        if self.depth + 1 >= MAX_ARGUMENT_DEPTH {
            return Err(self.error(
                ErrorKind::ArgumentsTooDeep {
                    limit: MAX_ARGUMENT_DEPTH,
                },
                open,
            ));
        }
        let text = &self.src[open + 1..end - 1];
        let flags = self.settings.flags;
        let variables = if flags.contains(ParserFlags::NO_FILEVARS) {
            Vec::new()
        } else {
            vec![
                ("FILENAME".to_string(), "undef".to_string()),
                (
                    "CURDIR".to_string(),
                    self.includes.base.to_string_lossy().into_owned(),
                ),
            ]
        };
        let mut expander =
            Expander::new(variables, None, !flags.contains(ParserFlags::DISABLE_MACRO));
        let mut includes = self.includes.for_arguments();
        let settings = Settings {
            flags,
            priority: 0,
            strategy: DuplicateStrategy::Append,
        };
        let root = parse_nested(text, settings, &mut expander, &mut includes, self.depth + 1)
            .map_err(|e| {
                let kind = match e.kind() {
                    ErrorKind::Stopped { path } => {
                        ErrorKind::StoppedInArguments { path: path.clone() }
                    }
                    kind => kind.clone(),
                };
                match e.file() {
                    Some(file) => Error::new(kind, e.position()).in_file(file),
                    None => Error::new(kind, position_at(self.src, open + 1 + e.position().offset)),
                }
            })?;
        Ok(Arguments::from_root(root))
    }

    /// Reads VALUE at the current position (§9.2) and expands variables in it.
    ///
    /// - `"…"`: checked like a double-quoted string (§6.1), but the value is the bytes between
    ///   the quotes as written.
    /// - `{…}`: leading whitespace is skipped; the value runs to the first `}`, trailing
    ///   whitespace included.
    /// - anything else: the bytes up to the next LF, CR, NUL, `,`, `;`, `#`, `]` or `}`,
    ///   trailing spaces and tabs included.
    fn macro_value(&mut self) -> Result<Vec<u8>, Error> {
        let at = self.pos;
        let src = self.src;
        let raw = match self.peek() {
            Some(b'"') => {
                let (_, end) = string::double_quoted(src, at)?;
                self.pos = end;
                &src[at + 1..end - 1]
            }
            Some(b'{') => {
                let start = at + 1 + src[at + 1..].iter().take_while(|&&b| is_space(b)).count();
                let Some(len) = src[start..].iter().position(|&b| b == b'}') else {
                    return Err(self.error(ErrorKind::UnterminatedMacroValue, at));
                };
                self.pos = start + len + 1;
                &src[start..start + len]
            }
            _ => {
                let len = src[at..]
                    .iter()
                    .position(|&b| ends_bare_value(b))
                    .unwrap_or(src.len() - at);
                self.pos = at + len;
                &src[at..at + len]
            }
        };
        Ok(self.expander.expand(raw.to_vec()))
    }

    /// After a macro: whitespace, line breaks and `;` are skipped, and the next entry may start
    /// right there. A `,` there, or a `#` that is the last byte of the input, is an error
    /// (§9.2).
    fn after_macro(&mut self) -> Result<(), Error> {
        while self.peek().is_some_and(|b| is_space(b) || b == b';') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b',') => Err(self.error(ErrorKind::UnexpectedTerminator, self.pos)),
            Some(b'#') if self.pos + 1 == self.src.len() => {
                Err(self.error(ErrorKind::HashAtEnd, self.pos))
            }
            _ => Ok(()),
        }
    }

    // ----- .priority (§9.5) ----------------------------------------------------------------

    /// Sets the priority of the values that follow in the current input unit. An empty VALUE
    /// takes the int parameter `priority` instead.
    fn priority_macro(&mut self, call: &MacroCall) -> Result<(), Error> {
        let n = if call.value.is_empty() {
            call.args
                .resolve(MacroKind::Priority.parameters())
                .int("priority")
                .ok_or_else(|| self.error(ErrorKind::InvalidPriority, call.at))?
        } else {
            decimal_integer(&call.value)
                .ok_or_else(|| self.error(ErrorKind::InvalidPriority, call.value_at))?
        };
        self.settings.priority = priority_bits(n);
        Ok(())
    }

    // ----- .inherit (§9.7) -----------------------------------------------------------------

    /// Copies the entries of the root object's entry NAME into the current object: each entry
    /// whose key the current object does not have yet, with its values, marked inherited. With
    /// `replace=true`, every entry, its values appended to an existing entry as they are,
    /// whatever the priorities and the strategy. An entry whose first value is an object or an
    /// array gives that value only (QUESTIONS.md #24).
    ///
    /// Entries are copied one at a time, each as it is at that moment, so when the current
    /// object is inside the source, the copy of the entry that holds it includes what it has
    /// received so far (§9.7, *Quirk*).
    fn inherit_macro(&mut self, call: &MacroCall) -> Result<(), Error> {
        let replace = call.args.exact_bool("replace").unwrap_or(false);
        let key_lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        let name = String::from_utf8_lossy(&call.value).into_owned();
        let missing = || ErrorKind::InheritSourceMissing { name: name.clone() };
        let root = self
            .root
            .as_object()
            .ok_or_else(|| self.error(missing(), call.value_at))?;
        let source_index = std::str::from_utf8(&call.value)
            .ok()
            .filter(|n| !n.is_empty())
            .and_then(|n| find_key(root, n, key_lowercase))
            .ok_or_else(|| self.error(missing(), call.value_at))?;
        let source_key = root
            .get_index(source_index)
            .map(|(key, _)| key.clone())
            .expect("the source was just found");
        let source = [PathSegment::Key {
            key: source_key,
            index: 0,
        }];
        let count = match self.value_at(&source) {
            Some(UclValue::Object(object)) => object.len(),
            _ => {
                return Err(self.error(
                    ErrorKind::InheritSourceNotObject { name: name.clone() },
                    call.value_at,
                ));
            }
        };
        for i in 0..count {
            let Some(key) = self
                .value_at(&source)
                .and_then(UclValue::as_object)
                .and_then(|object| object.get_index(i))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.copy_entry(key, i, replace, &source, call.at)?;
        }
        Ok(())
    }

    /// The values `.inherit` copies from entry `index` of the object at `source`, as they are
    /// now, with any containers open inside them: all of them, or the first only when it is an
    /// object or an array (QUESTIONS.md #24).
    fn inherited_slots(&self, source: &[PathSegment], index: usize) -> Option<Vec<Slot>> {
        let (key, entry) = self.value_at(source)?.as_object()?.get_index(index)?;
        let count = match entry.first() {
            UclValue::Object(_) | UclValue::Array(_) => 1,
            _ => entry.len(),
        };
        let mut slots = Vec::with_capacity(count);
        for (slot_index, slot) in entry.slots()[..count].iter().enumerate() {
            let value = match slot.value() {
                UclValue::Object(_) | UclValue::Array(_) => {
                    let mut path = source.to_vec();
                    path.push(PathSegment::Key {
                        key: key.clone(),
                        index: slot_index,
                    });
                    self.filled_copy(&path)?
                }
                scalar => scalar.clone(),
            };
            slots.push(slot.with_value(value));
        }
        Some(slots)
    }

    /// Adds entry `index`, whose key is `key`, of the object at `source`, copied by `.inherit`,
    /// to the current object. The copies keep the output facts of the values they copy (spec
    /// §10.1).
    ///
    /// A copy that would be nested more than [`super::MAX_NESTING`] containers deep, the root
    /// included, is an error at `at`, the macro. libucl accepts it (a project divergence, see
    /// [`Core::check_nesting`]).
    fn copy_entry(
        &mut self,
        key: String,
        index: usize,
        replace: bool,
        source: &[PathSegment],
        at: usize,
    ) -> Result<(), Error> {
        let key_lowercase = self.settings.flags.contains(ParserFlags::KEY_LOWERCASE);
        if key_lowercase && key.bytes().any(|b| b.is_ascii_uppercase()) {
            self.uppercase_keys = true;
        }
        let ignore_case = key_lowercase && self.uppercase_keys;
        let existing = self.find_current_key(&key, ignore_case);
        if !replace && existing.is_some() {
            return Ok(());
        }
        // Taken before anything is copied: the source may contain the current object (§9.7).
        let Some(slots) = self.inherited_slots(source, index) else {
            return Ok(());
        };
        for slot in &slots {
            self.check_nesting(slot.value(), at)?;
        }
        let copied_facts: Option<Vec<_>> = self.facts.as_ref().map(|facts| {
            (0..slots.len())
                .map(|index| {
                    let mut path = source.to_vec();
                    path.push(PathSegment::Key {
                        key: key.clone(),
                        index,
                    });
                    facts.subtree(&path)
                })
                .collect()
        });
        let count = slots.len();
        let object = self
            .current()
            .as_object_mut()
            .expect("macros are read inside objects");
        let (target, first_index) = match existing {
            None => {
                let mut slots = slots
                    .into_iter()
                    .map(|slot| if replace { slot } else { slot.into_inherited() });
                let first = slots.next().expect("an entry holds at least one value");
                let mut entry = Entry::from_slot(first);
                slots.for_each(|slot| entry.push_slot(slot));
                object.insert_entry(key.clone(), entry);
                (key.clone(), 0)
            }
            Some(index) if replace => {
                let (name, entry) = object
                    .get_index_mut(index)
                    .expect("the index was just found");
                let first_index = entry.len();
                slots.into_iter().for_each(|slot| entry.push_slot(slot));
                (name.clone(), first_index)
            }
            Some(_) => return Ok(()),
        };
        let Some(copied_facts) = copied_facts else {
            return Ok(());
        };
        if copied_facts.iter().all(Vec::is_empty) && target == key {
            return Ok(());
        }
        for (offset, subtree) in copied_facts.into_iter().enumerate().take(count) {
            let Some(node) = self.facts_node_below(&[PathSegment::Key {
                key: target.clone(),
                index: first_index + offset,
            }]) else {
                return Ok(());
            };
            let facts = self.facts.as_mut().expect("facts are recorded");
            // The copy's key is spelled as the copied value's key was.
            let spelling = subtree
                .iter()
                .find(|(rest, _, _)| rest.is_empty())
                .and_then(|(_, f, _)| f.key_spelling.clone())
                .unwrap_or_else(|| key.clone());
            facts.graft(node, subtree);
            facts.update(node, |f| {
                f.key_spelling = (spelling != target).then_some(spelling)
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{CommentPlacement, MAX_NESTING, Parser, parse};

    fn arguments(text: &str) -> Arguments {
        Arguments::from_root(parse(text.as_bytes()).unwrap())
    }

    fn obj(v: &UclValue) -> &UclObject {
        v.as_object().expect("object")
    }

    fn kind(input: &str) -> ErrorKind {
        parse(input.as_bytes()).expect_err(input).kind().clone()
    }

    /// The priorities of the values of `key`.
    fn priorities(object: &UclObject, key: &str) -> Vec<u8> {
        object
            .entry(key)
            .unwrap()
            .slots()
            .iter()
            .map(Slot::priority)
            .collect()
    }

    /// The values of `key`, as integers.
    fn ints(object: &UclObject, key: &str) -> Vec<i64> {
        object
            .get_all(key)
            .map(|v| v.as_integer().unwrap())
            .collect()
    }

    #[test]
    fn inherit_copies_open_containers_as_they_are() {
        // §9.7, *Quirk*: a source that holds the current object is copied with what the open
        // containers have received so far, at any depth (oracle runs).
        for (input, expected) in [
            (
                r#"o { x = 1; p { y = 2; e { z = 3; .inherit "o"; w = 4 } } }"#,
                r#"{"o":{"x":1,"p":{"y":2,"e":{"z":3,"x":1,"p":{"y":2,"e":{"z":3,"x":1}},"w":4}}}}"#,
            ),
            (
                r#"o { x = 1; p { y = 2; e { z = 3; .inherit(replace=true) "o" } } }"#,
                r#"{"o":{"x":1,"p":{"y":2,"e":{"z":3,"x":1,"p":{"y":2,"e":{"z":3,"x":1}}}}}}"#,
            ),
            (
                r#"o { x = 1; p [ { e { z = 3; .inherit "o" } } ] }"#,
                r#"{"o":{"x":1,"p":[{"e":{"z":3,"x":1,"p":[{"e":{"z":3,"x":1}}]}}]}}"#,
            ),
            (
                r#"o { a = 1; a { b = 2; .inherit "o" } }"#,
                r#"{"o":{"a":[1,{"b":2,"a":[1,{"b":2}]}]}}"#,
            ),
            (
                r#"o { a { b { c { .inherit "o" } } } }"#,
                r#"{"o":{"a":{"b":{"c":{"a":{"b":{"c":{}}}}}}}}"#,
            ),
        ] {
            let value = parse(input.as_bytes()).unwrap();
            assert_eq!(crate::emit::to_json_compact(&value), expected, "{input}");
        }
    }

    #[test]
    fn priority_applies_to_later_values_of_the_unit() {
        // spec §9.5, §8.3
        let v = parse(b"a = 1\n.priority 3\nb = 1\nc { d = 1 }\ne { .priority 5\nf = 1 }\ng = 1")
            .unwrap();
        let o = obj(&v);
        assert_eq!(priorities(o, "a"), [0]);
        assert_eq!(priorities(o, "b"), [3]);
        assert_eq!(priorities(o, "c"), [3]);
        assert_eq!(priorities(obj(&o["c"]), "d"), [3]);
        assert_eq!(priorities(o, "e"), [3]);
        assert_eq!(priorities(obj(&o["e"]), "f"), [5]);
        assert_eq!(priorities(o, "g"), [5]);
        // Priorities decide repeated keys (§8.3).
        let v = parse(b"a = 1\n.priority 3\na = 2\n.priority 1\na = 3").unwrap();
        assert_eq!(ints(obj(&v), "a"), [2]);
    }

    #[test]
    fn priority_value_forms() {
        // spec §9.2, §9.5
        let mut p = Parser::new();
        p.register_variable("P", "7");
        for (input, expected) in [
            (".priority 3\na = 1", 3),
            (".priority \"4\"\na = 1", 4),
            (".priority \" 3\"\na = 1", 3),
            (".priority \"+4\"\na = 1", 4),
            (".priority {3}\na = 1", 3),
            (".priority {\n3}\na = 1", 3),
            (".priority 010\na = 1", 10),
            (".priority 20;a = 1", 4),
            (".priority /* c */ 3\na = 1", 3),
            (".priority # c\n3\na = 1", 3),
            (".priority\n\n3\na = 1", 3),
            (".priority(priority=6) 5\na = 1", 5),
            (".priority(priority=4);\na = 1", 4),
            (".priority(priority=4) \"\"\na = 1", 4),
            (".priority(p=4);a = 1", 4),
            (".priority(priority=1k);a = 1", 8),
            (".priority(priority=-1);a = 1", 15),
            (".priority({priority=2});a = 1", 2),
            (".priority(p=\"(\") 1\na = 1", 1),
            (".priority $P\na = 1", 7),
            (".priority \"1$P\"\na = 1", 1),
            (".priority 1;;\n# c\na = 1", 1),
            (".priority 3}", 3),
        ] {
            let result = if input.ends_with('}') {
                p.parse(format!("x {{ {input}").as_bytes()).map(|_| ())
            } else {
                p.parse(input.as_bytes()).map(|v| {
                    assert_eq!(priorities(obj(&v), "a"), [expected], "{input:?}");
                })
            };
            assert!(result.is_ok(), "{input:?}: {result:?}");
        }
        // A `#` after a comment is a comment again.
        assert!(parse(b".priority 1\n/* c */#").is_ok());
    }

    #[test]
    fn priority_errors() {
        // spec §9.2, §9.5
        for input in [
            ".priority 3 # c\na = 1",
            ".priority 3 ;a = 1",
            "a { .priority 3 }",
            ".priority { 3 }\na = 1",
            ".priority x",
            ".priority \"0x10\"",
            ".priority \" \"",
            ".priority(priority=4) \" \"",
            ".priority +",
            ".priority \"\"\na = 1",
            ".priority();a = 1",
            ".priority(priority=2.5);a = 1",
            ".priority(priority=\"2\");a = 1",
            ".priority\na = 1",
            ".priority(priority=4)\na = 1",
            ".priority\n#",
        ] {
            assert_eq!(kind(input), ErrorKind::InvalidPriority, "{input:?}");
        }
        assert_eq!(kind(".priority 3,a = 1"), ErrorKind::UnexpectedTerminator);
        for input in [
            ".priority 1#",
            ".priority 1;#",
            "a = 1\n.priority 1\n #",
            ".priority(priority=4)#",
            ".priority(priority=4) #",
            ".priority(priority=4)\n#",
        ] {
            assert_eq!(kind(input), ErrorKind::HashAtEnd, "{input:?}");
        }
        assert_eq!(kind(".priority {3"), ErrorKind::UnterminatedMacroValue);
        assert_eq!(kind(".priority \"3"), ErrorKind::UnterminatedString);
        assert_eq!(kind(".priority \"\\u12\""), ErrorKind::InvalidUnicodeEscape);
        assert_eq!(kind(".priority(priority=2)"), ErrorKind::MissingValue);
        assert_eq!(kind(".priority(priority=2) # c\n"), ErrorKind::MissingValue);
        // With repeated names collected into an array, `priority` is no longer an int.
        let e = Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS)
            .parse(b".priority(priority=1, priority=2);\na = 1")
            .unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::InvalidPriority);
    }

    #[test]
    fn argument_documents() {
        // spec §9.2: a separate document with the same flags; errors point into the input.
        let e = parse(b"a = 1\n.priority(x) 1").unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::MissingValue);
        assert_eq!((e.position().line, e.position().column), (2, 12));
        assert_eq!(
            kind(".priority(priority=2, .foo 1);"),
            ErrorKind::UnknownMacro { name: "foo".into() }
        );
        // Macros inside arguments run in the argument document.
        let v = parse(b".priority(d { priority = 3 }; .inherit \"d\");\na = 1").unwrap();
        assert_eq!(priorities(obj(&v), "a"), [3]);
        // Only the file variables of a document given as bytes: FILENAME is `undef`.
        let input = b".priority(undef { priority = 3 }; .inherit \"$FILENAME\");\na = 1";
        let mut p = Parser::new();
        p.register_variable("FILENAME", "registered");
        assert_eq!(priorities(obj(&p.parse(input).unwrap()), "a"), [3]);
        let input = b".priority(\"$FILENAME\" { priority = 3 }; .inherit \"$FILENAME\");a = 1";
        let v = Parser::with_flags(ParserFlags::NO_FILEVARS)
            .parse(input)
            .unwrap();
        assert_eq!(priorities(obj(&v), "a"), [3]);
        let mut p = Parser::new();
        p.register_variable("V", "v");
        let e = p
            .parse(b".priority(v { priority = 3 }; .inherit \"$V\");a = 1")
            .unwrap_err();
        assert!(matches!(e.kind(), ErrorKind::InheritSourceMissing { name } if name == "$V"));
        // Under KEY_LOWERCASE parameter names are lowercased.
        let v = Parser::with_flags(ParserFlags::KEY_LOWERCASE)
            .parse(b".priority(PRIORITY=2);a = 1")
            .unwrap();
        assert_eq!(priorities(obj(&v), "a"), [2]);
        assert_eq!(
            kind(".priority(PRIORITY=2);a = 1"),
            ErrorKind::InvalidPriority
        );
    }

    #[test]
    fn argument_documents_nest_up_to_the_limit() {
        let nested = |levels: usize| {
            let mut inner = String::from("priority=2");
            for _ in 1..levels {
                inner = format!(".priority({inner}) 1");
            }
            format!(".priority({inner}) 1\na = 1\n")
        };
        let v = parse(nested(MAX_ARGUMENT_DEPTH - 1).as_bytes()).unwrap();
        assert_eq!(priorities(obj(&v), "a"), [1]);
        let e = parse(nested(MAX_ARGUMENT_DEPTH).as_bytes()).unwrap_err();
        assert_eq!(
            e.kind(),
            &ErrorKind::ArgumentsTooDeep {
                limit: MAX_ARGUMENT_DEPTH
            }
        );
        assert!(parse(nested(2_000).as_bytes()).is_err());
    }

    #[test]
    fn signatures_are_unsupported() {
        // Project decision: the crate never verifies signatures.
        for input in [
            ".includes \"x.conf\"",
            ".includes(sign=false) {x}",
            ".include(sign=true) \"x.conf\"",
            ".try_include(s=yes) x\nk = 1",
        ] {
            assert!(
                parse(input.as_bytes()).unwrap_err().is_unsupported(),
                "{input:?}"
            );
        }
        assert!(matches!(
            kind(".include(sign=false) \"no such file\""),
            ErrorKind::FileNotFound { .. }
        ));
        // Syntax errors in the arguments, the value and after it come first.
        assert_eq!(kind(".include(x) \"a\""), ErrorKind::MissingValue);
        assert_eq!(kind(".include \"a"), ErrorKind::UnterminatedString);
        assert_eq!(kind(".include {a"), ErrorKind::UnterminatedMacroValue);
        // The include runs before what follows it is checked (QUESTIONS.md #23).
        assert!(matches!(
            kind(".include \"a\", k = 1"),
            ErrorKind::FileNotFound { .. }
        ));
        assert_eq!(
            kind(".include(try=true) \"a\", k = 1"),
            ErrorKind::UnexpectedTerminator
        );
        // A last-byte `#` ends an empty VALUE: the empty path, which names no file.
        assert!(matches!(kind(".include\n#"), ErrorKind::FileNotFound { path } if path.is_empty()));
        assert!(matches!(
            kind("a = 1\n.include #"),
            ErrorKind::FileNotFound { .. }
        ));
        assert_eq!(kind(".include(try=true)#"), ErrorKind::HashAtEnd);
        let e = parse(b"a = 1\n.try_include()#").unwrap_err();
        assert!(e.is_stopped(), "{e}");
        assert_eq!(obj(e.partial().unwrap()).len(), 1);
    }

    #[test]
    fn inherit_copies_missing_entries_marked_inherited() {
        // spec §9.7
        let v = parse(b"d { a = 1; b = 2; b = 3 }\ne { b = 9; .inherit \"d\"; c = 4 }").unwrap();
        let e = obj(&obj(&v)["e"]);
        assert_eq!(e.keys().collect::<Vec<_>>(), ["b", "a", "c"]);
        assert_eq!(ints(e, "b"), [9]);
        assert!(e.entry("a").unwrap().slots()[0].is_inherited());
        assert!(!e.entry("b").unwrap().slots()[0].is_inherited());
        // All values are copied; a later explicit value replaces them all (§8.3).
        let v = parse(b"d { a = 1; a = 2 }\ne { .inherit \"d\" }\nf { .inherit \"d\"; a = 3 }")
            .unwrap();
        assert_eq!(ints(obj(&obj(&v)["e"]), "a"), [1, 2]);
        assert_eq!(ints(obj(&obj(&v)["f"]), "a"), [3]);
        // Copies keep their priority.
        let v = parse(b".priority 2\nd { a = 1 }\n.priority 1\ne { .inherit \"d\" }").unwrap();
        assert_eq!(priorities(obj(&obj(&v)["e"]), "a"), [2]);
        // At the root, into the root; the source is the first value of NAME.
        let v = parse(b"d { a = 1 }\nd { b = 1 }\n.inherit \"d\"").unwrap();
        assert_eq!(obj(&v).keys().collect::<Vec<_>>(), ["d", "a"]);
        // The copy is deep: merging into it leaves the source alone.
        let mut p = Parser::new();
        p.set_strategy(DuplicateStrategy::Merge);
        let v = p
            .parse(b"d { a { x = 1 } }\ne { .inherit \"d\"; a { y = 2 } }")
            .unwrap();
        assert_eq!(obj(&obj(&obj(&v)["d"])["a"]).len(), 1);
        assert_eq!(obj(&obj(&obj(&v)["e"])["a"]).len(), 2);
    }

    #[test]
    fn inherit_copies_only_a_container_first_value() {
        // Oracle runs (QUESTIONS.md #24): an entry whose first value is an object or an array
        // gives that value only; otherwise every value is copied.
        let v = parse(
            b"d { a = [1]; a = 5; b { x = 1 }; b = 5; c = 5; c = [1]; c = 6 }\n\
              e { .inherit \"d\" }\nf { a = 0; .inherit(replace=true) \"d\" }",
        )
        .unwrap();
        let e = obj(&obj(&v)["e"]);
        assert_eq!(e.entry("a").unwrap().len(), 1);
        assert_eq!(e.entry("b").unwrap().len(), 1);
        assert_eq!(e.entry("c").unwrap().len(), 3);
        let f = obj(&obj(&v)["f"]);
        assert_eq!(f.entry("a").unwrap().len(), 2);
    }

    #[test]
    fn inherit_from_enclosing_and_own_objects() {
        // spec §9.7, *Quirk*: entries are copied one at a time.
        let v = parse(b"o { x = 1; e { .inherit \"o\" } }").unwrap();
        let e = obj(&obj(&obj(&v)["o"])["e"]);
        assert_eq!(e.keys().collect::<Vec<_>>(), ["x", "e"]);
        assert_eq!(obj(&e["e"]).keys().collect::<Vec<_>>(), ["x"]);
        let v = parse(b"e { a = 1; .inherit \"e\" }").unwrap();
        assert_eq!(ints(obj(&obj(&v)["e"]), "a"), [1]);
        let v = parse(b"e { a = 1 }\ne { .inherit \"e\"; b = 2 }").unwrap();
        let second = obj(&v).get_all("e").nth(1).unwrap();
        assert_eq!(obj(second).keys().collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn inherit_with_replace_appends_copies_as_they_are() {
        // spec §9.7, *Quirk*; details from oracle runs (QUESTIONS.md #24).
        let v = parse(b"d { a = 1; b = 2 }\ne { b = 3; .inherit(replace=true) \"d\" }").unwrap();
        let e = obj(&obj(&v)["e"]);
        assert_eq!(ints(e, "b"), [3, 2]);
        assert!(!e.entry("a").unwrap().slots()[0].is_inherited());
        let v = parse(b"e { a = 1; .inherit(replace=true) \"e\"; .inherit(replace=true) \"e\" }")
            .unwrap();
        assert_eq!(ints(obj(&obj(&v)["e"]), "a"), [1, 1, 1, 1]);
        // Whatever the priorities and the strategy.
        let v =
            parse(b"d { a = 1 }\n.priority 2\ne { a = 0; .inherit(replace=true) \"d\" }").unwrap();
        assert_eq!(priorities(obj(&obj(&v)["e"]), "a"), [2, 0]);
        let mut p = Parser::new();
        p.set_strategy(DuplicateStrategy::Error);
        let v = p
            .parse(b"d { a = 1 }\ne { a = 0; .inherit(replace=true) \"d\" }")
            .unwrap();
        assert_eq!(ints(obj(&obj(&v)["e"]), "a"), [0, 1]);
        // The source's marks are kept: a copy of an inherited value is inherited.
        let v =
            parse(b"d { a = 1 }\ne { .inherit \"d\" }\nf { .inherit(replace=true) \"e\"; a = 5 }")
                .unwrap();
        assert_eq!(ints(obj(&obj(&v)["f"]), "a"), [5]);
        let v = Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS)
            .parse(b"d { a = 1; a = 2 }\ne { .inherit(replace=true) \"d\"; a = 3 }")
            .unwrap();
        let a = &obj(&obj(&v)["e"])["a"];
        assert_eq!(a.as_array().map(Vec::len), Some(3));
        // Under NO_IMPLICIT_ARRAYS, a repeat collects only the entry's first value
        // (QUESTIONS.md #25).
        let v = Parser::with_flags(ParserFlags::NO_IMPLICIT_ARRAYS)
            .parse(b"d { a = 1 }\ne { a = 6; .inherit(replace=true) \"d\"; a = 7; a = 8 }")
            .unwrap();
        let a = obj(&obj(&v)["e"]).entry("a").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(
            a.first(),
            &UclValue::Array(vec![6, 7, 8].into_iter().map(UclValue::Integer).collect())
        );
        // `replace` is matched by exact name and must be a bool.
        for args in ["r=true", "replace=1", "replace=\"true\""] {
            let input = format!("d {{ a = 1 }}\ne {{ a = 0; .inherit({args}) \"d\" }}");
            let v = parse(input.as_bytes()).unwrap();
            assert_eq!(ints(obj(&obj(&v)["e"]), "a"), [0], "{args}");
        }
    }

    /// `name { a { a { … } } }`: an object `depth` containers deep, with `inner` in the last.
    fn chain(name: &str, depth: usize, inner: &str) -> String {
        format!(
            "{name} {{ {}{inner}{}\n",
            "a { ".repeat(depth - 1),
            " }".repeat(depth)
        )
    }

    #[test]
    fn inherit_copies_nest_no_deeper_than_the_limit() {
        // A copy placed `m` containers deep, the root included, of an object `n` deep holds
        // containers `m + n - 1` deep. The oracle accepts any depth here; the core allows what
        // spec §11.2 allows to be open at once, 1024 containers with the root.
        let source = chain("d", 600, "leaf = 1;");
        let at_limit = format!("{source}{}", chain("e", 424, ".inherit \"d\";"));
        let v = parse(at_limit.as_bytes()).unwrap();
        assert_eq!(crate::value::nesting(&v), MAX_NESTING);
        let beyond = format!("{source}{}", chain("e", 425, ".inherit \"d\";"));
        let e = parse(beyond.as_bytes()).unwrap_err();
        assert_eq!(e.kind(), &ErrorKind::NestingTooDeep { limit: MAX_NESTING });
        // At the macro, after `e { ` and 424 times `a { `.
        assert_eq!((e.position().line, e.position().column), (2, 1 + 4 * 425));

        // The copy of an enclosing object holds the objects open inside it (§9.7, *Quirk*).
        let enclosing = |depth| chain("o", depth, ".inherit \"o\";");
        let v = parse(enclosing(512).as_bytes()).unwrap();
        assert_eq!(crate::value::nesting(&v), MAX_NESTING);
        assert_eq!(
            kind(&enclosing(513)),
            ErrorKind::NestingTooDeep { limit: MAX_NESTING }
        );

        // Chains of copies, each of which libucl nests 1000 levels deeper, stop at the limit.
        let mut chained = chain("r0", 1000, "leaf = 1;");
        for i in 1..20 {
            chained.push_str(&chain(
                &format!("r{i}"),
                1000,
                &format!(".inherit \"r{}\";", i - 1),
            ));
        }
        assert_eq!(
            kind(&chained),
            ErrorKind::NestingTooDeep { limit: MAX_NESTING }
        );
    }

    #[test]
    fn inherit_names_and_errors() {
        // spec §9.7
        let v = parse(b"d.x { a = 1 }\ne { .inherit \"d.x\" }\nf { .inherit {d.x} }").unwrap();
        assert_eq!(ints(obj(&obj(&v)["f"]), "a"), [1]);
        let mut p = Parser::new();
        p.register_variable("V", "d");
        let v = p.parse(b"d { a = 1 }\ne { .inherit \"$V\" }").unwrap();
        assert_eq!(ints(obj(&obj(&v)["e"]), "a"), [1]);
        let v = Parser::with_flags(ParserFlags::KEY_LOWERCASE)
            .parse(
                b"D { A = 1 }\ne { .inherit \"D\"; \"\\u0042\" = 2; .inherit(replace=true) \"e\" }",
            )
            .unwrap();
        let e = obj(&obj(&v)["e"]);
        assert_eq!(e.keys().collect::<Vec<_>>(), ["a", "B"]);
        assert_eq!(ints(e, "B"), [2, 2]);
        assert_eq!(ints(e, "a"), [1, 1]);
        let missing = |input: &str| matches!(kind(input), ErrorKind::InheritSourceMissing { .. });
        for input in [
            "e { .inherit \"nope\" }",
            "d { a = 1 }\ne { .inherit \"\" }",
            "d { a = 1 }\ne { .inherit d }",
            "D { a = 1 }\ne { .inherit \"d\" }",
            "e { .inherit \"d\" }\nd { a = 1 }",
            "o { d { a = 1 } e { .inherit \"d\" } }",
            "d { x { a = 1 } }\ne { .inherit \"d.x\" }",
            "[ { .inherit \"x\" } ]",
            ".inherit\n#",
        ] {
            assert!(missing(input), "{input:?}: {:?}", kind(input));
        }
        assert_eq!(
            kind("d = 1\ne { .inherit \"d\" }"),
            ErrorKind::InheritSourceNotObject { name: "d".into() }
        );
        assert!(matches!(
            kind("d = [1]\ne { .inherit \"d\" }"),
            ErrorKind::InheritSourceNotObject { .. }
        ));
    }

    #[test]
    fn macros_create_no_values_for_comments() {
        // spec §12.5; oracle runs for `.inherit`.
        let run = |input: &[u8]| {
            let mut p = Parser::with_flags(ParserFlags::SAVE_COMMENTS);
            p.parse(input).unwrap();
            p.attached_comments()
                .iter()
                .map(|g| {
                    let texts: Vec<_> = g
                        .comments
                        .iter()
                        .map(|&i| p.comments()[i].text.clone())
                        .collect();
                    (g.path.clone(), g.placement, texts)
                })
                .collect::<Vec<_>>()
        };
        let key = |k: &str| PathSegment::Key {
            key: k.into(),
            index: 0,
        };
        assert_eq!(
            run(b"a = 1\n# c\n.priority 2\nb = 2"),
            [(
                vec![key("b")],
                CommentPlacement::Before,
                vec!["# c".to_string()]
            )]
        );
        assert_eq!(
            run(b".priority /* c */3\na = 1"),
            [(
                vec![key("a")],
                CommentPlacement::Before,
                vec!["/* c */3".to_string()]
            )]
        );
        assert_eq!(
            run(b"d { a = 1 }\ne {\n# c\n.inherit \"d\"\n}"),
            [(
                vec![key("e")],
                CommentPlacement::After,
                vec!["# c".to_string()]
            )]
        );
        assert!(run(b"d { a = 1 }\n.priority(# c\npriority=2);\na = 1").is_empty());
    }

    #[test]
    fn parameters_match_by_prefix_and_type() {
        // spec §9.2, *Quirk*; §9.4, §9.6 tables
        let include = MacroKind::Include.parameters();
        let r = arguments("t = true; p = true");
        let r = r.resolve(include);
        assert_eq!((r.bool("try"), r.bool("prefix")), (Some(true), Some(true)));
        assert_eq!(r.bool("sign"), None);
        let args = arguments("t = true; tr = true; tri = true");
        let r = args.resolve(MacroKind::Load.parameters());
        assert_eq!((r.bool("try"), r.bool("trim")), (Some(true), Some(true)));
        let args = arguments("k = \"x\"; ta = array; d = merge; pa = [\"d\"]; pr = 1k");
        let r = args.resolve(include);
        assert_eq!(r.string("key"), Some("x"));
        assert_eq!(r.string("target"), Some("array"));
        assert_eq!(r.string("duplicate"), Some("merge"));
        assert_eq!(r.array("path").map(<[_]>::len), Some(1));
        assert_eq!(r.int("priority"), Some(1000));
        // Two names for one parameter: the last wins. A repeated name: its first value counts.
        let args = arguments("prio = 2, pr = 3");
        assert_eq!(args.resolve(include).int("priority"), Some(3));
        let args = arguments("priority = 1, priority = 2");
        assert_eq!(args.resolve(include).int("priority"), Some(1));
        // Values of another type are ignored, as are unknown and case-different names.
        for text in [
            "priority = 2.0",
            "priority = \"2\"",
            "priority = 1s",
            "PRIORITY = 2",
            "x = 2",
        ] {
            let args = arguments(text);
            assert_eq!(args.resolve(include).int("priority"), None, "{text}");
        }
        let args = arguments("try = 1; t = \"true\"");
        assert_eq!(args.resolve(include).bool("try"), None);
    }

    #[test]
    fn replace_is_matched_exactly() {
        // spec §9.7
        assert_eq!(arguments("replace = yes").exact_bool("replace"), Some(true));
        assert_eq!(arguments("r = true").exact_bool("replace"), None);
        assert_eq!(arguments("replace = 1").exact_bool("replace"), None);
        let args = arguments("replace = true, replace = false");
        assert_eq!(args.exact_bool("replace"), Some(true));
        assert_eq!(
            Arguments::from_root(UclValue::Array(vec![])).exact_bool("replace"),
            None
        );
    }

    #[test]
    fn priority_values() {
        // spec §9.5
        let read = |v: &str| decimal_integer(v.as_bytes()).map(priority_bits);
        for (text, expected) in [
            ("3", 3),
            ("010", 10),
            (" 3", 3),
            ("\t\x0b\x0c3", 3),
            ("+4", 4),
            ("-0", 0),
            ("17", 1),
            ("-1", 15),
            ("99999999999999999999", 15),
            ("-9223372036854775809", 0),
        ] {
            assert_eq!(read(text), Some(expected), "{text:?}");
        }
        for bad in ["", " ", "x", "0x10", "+", "- 3", "+-3", "3 ", "3\x0b", "3a"] {
            assert_eq!(read(bad), None, "{bad:?}");
        }
    }
}
