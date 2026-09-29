//! The behaviour that the released spec leaves uncertain (spec README, *Uncertain behaviour*),
//! recognised in a difference between the oracle's dump and the crate's, so that the fuzzer skips
//! it and reports every other difference.
//!
//! Each recogniser matches one uncertain rule as narrowly as the dumps and the input allow:
//!
//! | Reason | Spec | What differs |
//! | --- | --- | --- |
//! | [`HANDLER`] | §7.7 | a string where a handler's result shares the string with other text |
//! | [`BINARY_MULTIPLIER`] | §5.4 | an int from a float outside the 64-bit range with `kb`, `mb` or `gb` |
//! | [`BLOCK_COMMENT_END`] | §12.5 | the byte saved after a block comment that ends its unit |
//! | [`REPLACED_COMMENTS`] | §12.5 | comments of a value §8 replaced, which libucl can attach to a later value |
//! | [`NUL_IN_COPY`] | §9.7, §13.2 | the bytes after the first NUL of a string that `.inherit`, `.seen` or `.ctx` copies |
//! | [`SEEN_COLLECTED_KEY`] | §13.2 | the key of a `no-implicit-arrays` collection in `.seen`'s copy of ARGUMENTS |
//! | [`ZEROCOPY_EMIT`] | §12.2 | keys and string values parsed from an expanded `.emit` VALUE under `zerocopy` |
//! | [`ARRAY_TEXT`] | §9.4, §13.2 | an included file, or text parsed in place, that starts with `[` and holds more |
//!
//! Four more cannot be seen in the dumps or the input, so the crate reports when a parse reaches
//! them ([`serde_ucl::parse::Parser::uncertain_reached`]), and any difference of such a parse is
//! skipped ([`reached`]): a macro after a name followed only by comments when the value created
//! most recently is not an object (§9.1), a container of an ended unit at the check at the end of
//! a later included file (§9.4), a `}` in an included file that closes an array element
//! opened by the including unit (§9.4), and a `/` at the end of a glob pattern that leaves out a
//! symbolic link to a regular file, or at the end of a plain include path after the name of a
//! file, which depends on the operating system (§9.4). The rest of the uncertain rules crash libucl, which the
//! fuzzer skips in any case: macro argument documents nested very deep (§9.2), a failure in an
//! included file after which libucl goes on with the same macro (§9.4), and a `}` in a file
//! included under a key where the object holds only its own bracket (§9.4).

use serde_json::Value as J;
use serde_ucl::UclValue;
use serde_ucl::parse::Uncertain;
use std::collections::BTreeSet;

pub const HANDLER: &str = "uncertain: a handler result with other text (§7.7)";
pub const BINARY_MULTIPLIER: &str = "uncertain: an out-of-range float with kb, mb or gb (§5.4)";
pub const BLOCK_COMMENT_END: &str = "uncertain: the byte after a block comment at the end (§12.5)";
pub const REPLACED_COMMENTS: &str = "uncertain: comments of a replaced value (§12.5)";
pub const NUL_IN_COPY: &str = "uncertain: bytes after a NUL in a copied string (§9.7)";
pub const SEEN_COLLECTED_KEY: &str = "uncertain: the key of a collection in .seen's copy (§13.2)";
pub const ZEROCOPY_EMIT: &str = "uncertain: expanded .emit text under zerocopy (§12.2)";
pub const ARRAY_TEXT: &str = "uncertain: an included file or text in place that starts with '['";
pub const REOPENED_NOT_OBJECT: &str = "uncertain: a reopened value that is not an object (§9.1)";
pub const ENDED_UNIT: &str = "uncertain: a container of an ended unit at a file's end (§9.4)";
pub const CLOSED_ARRAY_ELEMENT: &str = "uncertain: a file's '}' closes an array element (§9.4)";
pub const TRAILING_SLASH: &str = "uncertain: a '/' after a file or a link to one (§9.4)";

/// §7.7 also affects the filename made from a macro value. The oracle can reject an include
/// whose quoted path contains a handler result and other text, while the crate's substitution
/// makes a glob with no matches. Limit this to a single include with an empty result: neither
/// another entry nor another macro can then be hidden by the skip.
pub fn handler_in_single_include_path(ctx: &Context<'_>, actual: &J) -> bool {
    if !ctx.has_flag("variable-handler") || ctx.has_flag("disable-macro") {
        return false;
    }
    let Some(map) = actual.as_object() else {
        return false;
    };
    if map.len() != 2
        || map.get("t").and_then(J::as_str) != Some("object")
        || !map
            .get("entries")
            .and_then(J::as_array)
            .is_some_and(Vec::is_empty)
    {
        return false;
    }
    let Some(mut rest) = ctx.input.trim_ascii().strip_prefix(b".include") else {
        return false;
    };
    if let Some(after_open) = rest.strip_prefix(b"(") {
        let Some(end) = after_open.iter().position(|&b| b == b')') else {
            return false;
        };
        // This recogniser deliberately leaves quoted or nested argument documents alone.
        if after_open[..end].iter().any(|&b| b == b'"' || b == b'(') {
            return false;
        }
        rest = &after_open[end + 1..];
    }
    rest = rest.trim_ascii_start();
    let Some(path) = rest.strip_prefix(b"\"").and_then(|s| s.strip_suffix(b"\"")) else {
        return false;
    };
    if path.contains(&b'"') || path.contains(&b'\\') {
        return false;
    }
    for (at, pair) in path.windows(2).enumerate() {
        if pair != b"${" {
            continue;
        }
        let Some(end) = path[at + 2..].iter().position(|&b| b == b'}') else {
            continue;
        };
        let name = &path[at + 2..at + 2 + end];
        if !name.starts_with(b"H_") || (at == 0 && at + 3 + end == path.len()) {
            continue;
        }
        let shadowed = ctx
            .flags
            .iter()
            .filter_map(|flag| flag.strip_prefix("var:"))
            .filter_map(|var| var.split_once('=').map(|(name, _)| name.as_bytes()))
            .any(|registered| registered == name);
        if !shadowed {
            return true;
        }
    }
    false
}

/// §7.7: a test handler result mixed with other text in one registered `.emit` VALUE can make
/// libucl accept text that the crate's chosen substitution rejects. Keep this to one simple
/// quoted VALUE and its one expected entry, so another parse failure stays visible.
pub fn handler_in_single_emit_value(ctx: &Context<'_>, golden: &J) -> bool {
    if !ctx.has_flag("variable-handler")
        || !ctx.has_flag("registered-macros")
        || ctx.has_flag("disable-macro")
    {
        return false;
    }
    let Some(rest) = ctx.input.trim_ascii().strip_prefix(b".emit") else {
        return false;
    };
    if !rest.first().is_some_and(u8::is_ascii_whitespace) {
        return false;
    }
    let value = rest.trim_ascii_start();
    let Some(text) = value
        .strip_prefix(b"\"")
        .and_then(|v| v.strip_suffix(b"\""))
    else {
        return false;
    };
    if text.contains(&b'"') || text.contains(&b'\\') || text.contains(&b'\n') {
        return false;
    }
    let Some(at) = text.windows(2).position(|pair| pair == b"${") else {
        return false;
    };
    let Some(end) = text[at + 2..].iter().position(|&b| b == b'}') else {
        return false;
    };
    let name = &text[at + 2..at + 2 + end];
    let key = text[..at].strip_suffix(b"=");
    if !name.starts_with(b"H_")
        || key.is_none_or(|k| k.is_empty() || !k.iter().all(u8::is_ascii_alphanumeric))
        || text[at + 3 + end..].is_empty()
        || ctx
            .flags
            .iter()
            .filter_map(|flag| flag.strip_prefix("var:"))
            .filter_map(|var| var.split_once('=').map(|(name, _)| name.as_bytes()))
            .any(|registered| registered == name)
    {
        return false;
    }
    let key = key.expect("checked above");
    let Some(entries) = golden.get("entries").and_then(J::as_array) else {
        return false;
    };
    if golden.get("t").and_then(J::as_str) != Some("object") || entries.len() != 1 {
        return false;
    }
    let entry = &entries[0];
    entry
        .get("k")
        .and_then(J::as_str)
        .is_some_and(|k| k.as_bytes() == key)
        && entry.get("v").and_then(J::as_array).is_some_and(|values| {
            values.len() == 1
                && values[0].get("t").and_then(J::as_str) == Some("string")
                && values[0]
                    .get("v")
                    .and_then(J::as_str)
                    .is_some_and(|v| v.contains("[handled]"))
        })
}

/// The reason for a difference in a parse that reached `rule`.
pub fn reached(rule: Uncertain) -> &'static str {
    match rule {
        Uncertain::ReopenedNotObject => REOPENED_NOT_OBJECT,
        Uncertain::EndedUnitContainer => ENDED_UNIT,
        Uncertain::ClosedArrayElement => CLOSED_ARRAY_ELEMENT,
        Uncertain::TrailingSlash => TRAILING_SLASH,
    }
}

/// What the recognisers need to know besides the two dumps.
pub struct Context<'a> {
    /// The input, as both parsed it.
    pub input: &'a [u8],
    pub flags: &'a [String],
    /// The texts of the comments the crate saved but attached to no value: those of values that
    /// §8 replaced or discarded (spec §12.5).
    pub dropped_comments: &'a [String],
    /// The rules the crate's parse reached that its result does not show.
    pub uncertain: &'a [Uncertain],
}

impl Context<'_> {
    fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    fn input_has(&self, needle: &[u8]) -> bool {
        self.input.windows(needle.len()).any(|w| w == needle)
    }

    /// The input has a macro that copies values: `.inherit`, or the test macros `.seen` and
    /// `.ctx` (spec §9.7, §13.2).
    fn copies(&self) -> bool {
        self.input_has(b".inherit")
            || (self.has_flag("registered-macros")
                && (self.input_has(b".seen") || self.input_has(b".ctx")))
    }

    /// The one unit is a registered `.emit` with an expanded built-in variable in its VALUE.
    /// Restricting this to one simple macro keeps entries outside its parsed text visible.
    fn single_expanded_emit(&self) -> bool {
        if !self.has_flag("registered-macros")
            || !self.has_flag("zerocopy")
            || self.has_flag("disable-macro")
        {
            return false;
        }
        let Some(rest) = self.input.trim_ascii().strip_prefix(b".emit") else {
            return false;
        };
        if !rest.first().is_some_and(u8::is_ascii_whitespace) {
            return false;
        }
        let value = rest.trim_ascii_start();
        // A lone `.` at the end is an ignored macro name (§9.2), after the
        // preceding line's VALUE has ended. It creates no other entry.
        let value = value.strip_suffix(b"\n.").unwrap_or(value);
        // ARGUMENTS may follow a block comment before VALUE (§9.2), and a
        // variable inside that comment or ARGUMENTS is not an expanded VALUE.
        // Decline all such shapes, including literal parentheses in VALUE.
        if value.windows(2).any(|pair| pair == b"/*") {
            return false;
        }
        if value.is_empty()
            || value
                .iter()
                .any(|b| matches!(b, b'\n' | b'\r' | b';' | b'\\' | b'#' | b'(' | b')'))
        {
            return false;
        }
        for (at, &byte) in value.iter().enumerate() {
            if byte != b'$' || (at > 0 && value[at - 1] == b'$') {
                continue;
            }
            let rest = &value[at + 1..];
            let name = if let Some(braced) = rest.strip_prefix(b"{") {
                let Some(end) = braced.iter().position(|&b| b == b'}') else {
                    continue;
                };
                &braced[..end]
            } else if rest.starts_with(b"ABI") {
                b"ABI".as_slice()
            } else if rest.starts_with(b"CURDIR") {
                b"CURDIR".as_slice()
            } else if rest.starts_with(b"FILENAME") {
                b"FILENAME".as_slice()
            } else {
                continue;
            };
            if name == b"ABI"
                || (!self.has_flag("no-filevars") && (name == b"CURDIR" || name == b"FILENAME"))
            {
                return true;
            }
        }
        false
    }

    /// The position and total count of entries when exactly one can depend on §12.2.
    /// A simple literal entry may precede or follow it; its dump must match exactly.
    /// Calls with ARGUMENTS remain outside this recognizer because VALUE starts later.
    fn expanded_emit_position(&self) -> Option<(usize, usize)> {
        if self.single_expanded_emit() {
            return Some((0, 1));
        }
        let input = std::str::from_utf8(self.input.trim_ascii()).ok()?;
        let (prefix, tail) = input.split_once('\n')?;
        let tail_ctx = Context {
            input: tail.as_bytes(),
            flags: self.flags,
            dropped_comments: self.dropped_comments,
            uncertain: self.uncertain,
        };
        if simple_literal_entry(prefix) && tail_ctx.single_expanded_emit() {
            return Some((1, 2));
        }
        let (head, suffix) = input.rsplit_once('\n')?;
        let head_ctx = Context {
            input: head.as_bytes(),
            flags: self.flags,
            dropped_comments: self.dropped_comments,
            uncertain: self.uncertain,
        };
        (head_ctx.single_expanded_emit() && simple_literal_entry(suffix)).then_some((0, 2))
    }
}

/// A one-line scalar entry whose boundary cannot consume text from a neighboring `.emit`.
fn simple_literal_entry(line: &str) -> bool {
    let (key, value) = {
        let mut words = line.split_ascii_whitespace();
        let Some(first) = words.next() else {
            return false;
        };
        match (words.next(), words.next()) {
            (Some(value), None) => (first, value),
            (None, None) => {
                let Some(parts) = first.split_once('=') else {
                    return false;
                };
                parts
            }
            _ => return false,
        }
    };
    !key.is_empty()
        && !value.is_empty()
        && key.bytes().all(|b| b.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'$')
}

/// Where the oracle's dump `golden` differs from the crate's `actual` only in behaviour the spec
/// leaves uncertain, rewrites those parts of `golden` to the crate's and adds the reasons.
/// Everything else is left for the comparison that follows. The dumps are walked in parallel,
/// entries and elements by position.
pub fn excuse(golden: &mut J, actual: &J, ctx: &Context<'_>, reasons: &mut BTreeSet<&'static str>) {
    if excuse_simple_later_rewrite_comment(golden, actual, ctx) {
        reasons.insert(REPLACED_COMMENTS);
        return;
    }
    if excuse_abi_emit_before_ignored_name(golden, actual, ctx) {
        reasons.insert(ZEROCOPY_EMIT);
        return;
    }
    excuse_at(golden, actual, ctx, reasons, false, true);
}

/// §9.2 ignores `.s` only when its NAME reaches the end of input. In this
/// restricted §12.2 form, it adds no entry, so only the preceding expanded
/// `.emit` key and string can have uncertain bytes. A single literal prefix
/// is allowed only when its whole dump agrees unchanged.
fn abi_emit_before_ignored_name_position(input: &[u8]) -> Option<usize> {
    const EMIT: &[u8] = b".emit r $ABI\n.s";
    if input == EMIT {
        return Some(0);
    }
    let prefix = input.strip_suffix(EMIT)?.strip_suffix(b"\n")?;
    let prefix = std::str::from_utf8(prefix).ok()?;
    simple_literal_entry(prefix).then_some(1)
}

fn excuse_abi_emit_before_ignored_name(golden: &mut J, actual: &J, ctx: &Context<'_>) -> bool {
    const FLAGS: [&str; 3] = ["zerocopy", "registered-macros", "string-input"];
    if ctx.flags.len() != FLAGS.len() || !FLAGS.iter().all(|flag| ctx.has_flag(flag)) {
        return false;
    }
    let Some(index) = abi_emit_before_ignored_name_position(ctx.input) else {
        return false;
    };
    let (Some(g_entries), Some(a_entries)) = (
        golden.get("entries").and_then(J::as_array),
        actual.get("entries").and_then(J::as_array),
    ) else {
        return false;
    };
    if golden.get("t").and_then(J::as_str) != Some("object")
        || actual.get("t").and_then(J::as_str) != Some("object")
        || g_entries.len() != index + 1
        || a_entries.len() != index + 1
        || a_entries[index].get("k").and_then(J::as_str) != Some("r")
        || entry_key(&g_entries[index]).is_none_or(|key| key.len() != 1)
    {
        return false;
    }
    let (Some(g_value), Some(a_value)) = (
        g_entries[index]
            .get("v")
            .and_then(J::as_array)
            .filter(|values| values.len() == 1)
            .and_then(|values| values[0].as_object()),
        a_entries[index]
            .get("v")
            .and_then(J::as_array)
            .filter(|values| values.len() == 1)
            .and_then(|values| values[0].as_object()),
    ) else {
        return false;
    };
    if g_value.get("t").and_then(J::as_str) != Some("string")
        || a_value.get("t").and_then(J::as_str) != Some("string")
        || g_value.get("v").and_then(J::as_str).map(str::len) != Some(7)
        || a_value.get("v").and_then(J::as_str) != Some("unknown")
    {
        return false;
    }
    let mut normalized = golden.clone();
    let Some(entry) = normalized["entries"][index].as_object_mut() else {
        return false;
    };
    entry.remove("khex");
    entry.insert("k".to_owned(), J::from("r"));
    normalized["entries"][index]["v"][0]["v"] = J::from("unknown");
    if &normalized != actual || &normalized == golden {
        return false;
    }
    *golden = normalized;
    true
}

/// §12.5 permits a replaced comment to reappear only on a value created later. This source
/// form proves the order without guessing from equal comment text: one initial comment, two
/// simple values of the same key under `rewrite`, then a distinct key and its own trailing
/// comment. Every other source form remains reportable.
fn simple_rewrite_pair(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(' ')?;
    (!key.is_empty()
        && key.bytes().all(|b| b.is_ascii_lowercase())
        && !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_alphanumeric()))
    .then_some((key, value))
}

fn simple_later_rewrite_source(input: &[u8]) -> Option<(&str, &str, &str)> {
    let input = std::str::from_utf8(input).ok()?;
    let input = input.strip_suffix('\n').unwrap_or(input);
    let lines: Vec<_> = input.split('\n').collect();
    if lines.len() != 4 {
        return None;
    }
    let comment = lines[0];
    let comment_body = comment.strip_prefix("# ")?;
    if comment_body.is_empty() || !comment_body.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let (replaced_key, _) = simple_rewrite_pair(lines[1])?;
    let (repeat_key, _) = simple_rewrite_pair(lines[2])?;
    let (last_value, last_comment) = lines[3].split_once('#')?;
    let (later_key, _) = simple_rewrite_pair(last_value)?;
    if replaced_key != repeat_key
        || replaced_key == later_key
        || last_comment != comment.strip_prefix('#')?
    {
        return None;
    }
    Some((replaced_key, later_key, comment))
}

/// For that proven source order, normalize the one comment difference only when the whole
/// value tree otherwise agrees. This keeps changed values, other comments and entries visible.
fn excuse_simple_later_rewrite_comment(golden: &mut J, actual: &J, ctx: &Context<'_>) -> bool {
    const FLAGS: [&str; 4] = [
        "dump-comments",
        "strategy:rewrite",
        "string-input",
        "no-filevars",
    ];
    if ctx.flags.len() != FLAGS.len() || !FLAGS.iter().all(|flag| ctx.has_flag(flag)) {
        return false;
    }
    let Some((replaced_key, later_key, comment)) = simple_later_rewrite_source(ctx.input) else {
        return false;
    };
    if ctx.dropped_comments.len() != 1 || ctx.dropped_comments[0] != comment {
        return false;
    }
    let (Some(g_entries), Some(a_entries)) = (
        golden.get("entries").and_then(J::as_array),
        actual.get("entries").and_then(J::as_array),
    ) else {
        return false;
    };
    if golden.get("t").and_then(J::as_str) != Some("object")
        || actual.get("t").and_then(J::as_str) != Some("object")
        || g_entries.len() != 2
        || a_entries.len() != 2
        || g_entries[0].get("k").and_then(J::as_str) != Some(replaced_key)
        || a_entries[0].get("k").and_then(J::as_str) != Some(replaced_key)
        || g_entries[1].get("k").and_then(J::as_str) != Some(later_key)
        || a_entries[1].get("k").and_then(J::as_str) != Some(later_key)
    {
        return false;
    }
    let Some(g_target) = g_entries[1]
        .get("v")
        .and_then(J::as_array)
        .filter(|values| values.len() == 1)
        .and_then(|values| values[0].as_object())
    else {
        return false;
    };
    let Some(g_comments) = g_target.get("c").and_then(J::as_array) else {
        return false;
    };
    if g_target.contains_key("ca")
        || g_comments.len() != 2
        || !g_comments.iter().all(|text| text.as_str() == Some(comment))
    {
        return false;
    }
    let mut normalized = golden.clone();
    let Some(target) = normalized["entries"][1]["v"][0].as_object_mut() else {
        return false;
    };
    target.remove("c");
    target.insert("ca".to_owned(), J::from(vec![comment]));
    if &normalized != actual {
        return false;
    }
    *golden = normalized;
    true
}

/// `emitted` is inherited only by descendants of an isolated expanded `.emit` entry.
fn excuse_at(
    golden: &mut J,
    actual: &J,
    ctx: &Context<'_>,
    reasons: &mut BTreeSet<&'static str>,
    emitted: bool,
    root: bool,
) {
    comments(golden, actual, ctx, reasons);
    let Some(kind) = golden.get("t").and_then(J::as_str).map(str::to_owned) else {
        return;
    };
    if actual.get("t").and_then(J::as_str) != Some(kind.as_str()) {
        return;
    }
    match kind.as_str() {
        "object" => {
            let (Some(g_entries), Some(a_entries)) = (
                golden.get_mut("entries").and_then(J::as_array_mut),
                actual.get("entries").and_then(J::as_array),
            ) else {
                return;
            };
            let emit_pos = if root {
                match ctx.expanded_emit_position() {
                    Some((0, 1)) if g_entries.len() == 1 && a_entries.len() == 1 => Some(0),
                    Some((1, 2))
                        if g_entries.len() == 2
                            && a_entries.len() == 2
                            && g_entries[0] == a_entries[0] =>
                    {
                        Some(1)
                    }
                    Some((0, 2))
                        if g_entries.len() == 2
                            && a_entries.len() == 2
                            && g_entries[1] == a_entries[1] =>
                    {
                        Some(0)
                    }
                    _ => None,
                }
            } else {
                None
            };
            for (index, (g_entry, a_entry)) in g_entries.iter_mut().zip(a_entries).enumerate() {
                let emitted_here = emitted || emit_pos == Some(index);
                if let (Some(g_key), Some(a_key)) =
                    (entry_key(g_entry), a_entry.get("k").and_then(J::as_str))
                    && g_key != a_key.as_bytes()
                    && let Some(entry) = g_entry.as_object_mut()
                {
                    let reason = if seen_collected_key(&g_key, a_key, a_entry, ctx) {
                        Some(SEEN_COLLECTED_KEY)
                    } else if emitted_here && g_key.len() == a_key.len() {
                        Some(ZEROCOPY_EMIT)
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        entry.remove("khex");
                        entry.insert("k".to_owned(), J::from(a_key));
                        reasons.insert(reason);
                    }
                }
                if let (Some(g_values), Some(a_values)) = (
                    g_entry.get_mut("v").and_then(J::as_array_mut),
                    a_entry.get("v").and_then(J::as_array),
                ) {
                    for (g, a) in g_values.iter_mut().zip(a_values) {
                        excuse_at(g, a, ctx, reasons, emitted_here, false);
                    }
                }
            }
        }
        "array" => {
            if let (Some(g_items), Some(a_items)) = (
                golden.get_mut("v").and_then(J::as_array_mut),
                actual.get("v").and_then(J::as_array),
            ) {
                for (g, a) in g_items.iter_mut().zip(a_items) {
                    excuse_at(g, a, ctx, reasons, emitted, false);
                }
            }
        }
        "string" => {
            let (Some(g), Some(a)) = (
                golden.get("v").and_then(J::as_str),
                actual.get("v").and_then(J::as_str),
            ) else {
                return;
            };
            if g == a {
                return;
            }
            let reason = if nul_in_copy(g, a, ctx) {
                Some(NUL_IN_COPY)
            } else if handler_with_other_text(g, a, ctx) {
                Some(HANDLER)
            } else if emitted && g.len() == a.len() {
                Some(ZEROCOPY_EMIT)
            } else {
                None
            };
            if let Some(reason) = reason {
                golden["v"] = J::from(a);
                reasons.insert(reason);
            }
        }
        "int" => {
            if let Some(a) = actual.get("v").and_then(J::as_str)
                && golden.get("v").and_then(J::as_str) != Some(a)
                && out_of_range_binary_multiplier(a, ctx)
            {
                golden["v"] = J::from(a);
                reasons.insert(BINARY_MULTIPLIER);
            }
        }
        _ => {}
    }
}

/// §9.7, §13.2: a copied string keeps its length, and libucl's bytes after its first NUL depend
/// on memory contents (mostly NUL on the oracle's machine); the crate copies the bytes. Bytes
/// there that are not UTF-8 make the dump a `hex` one, which the comparison skips anyway.
fn nul_in_copy(golden: &str, actual: &str, ctx: &Context<'_>) -> bool {
    let (g, a) = (golden.as_bytes(), actual.as_bytes());
    let Some(nul) = a.iter().position(|&b| b == 0) else {
        return false;
    };
    ctx.copies() && g.len() == a.len() && g[..=nul] == a[..=nul]
}

/// §7.7: a handler's result that shares its string with other text. The crate substitutes it in
/// place; libucl's result depends on memory contents, and can hold bytes never written, as for
/// `${H_$ABI`, where libucl asks the handler and the crate sees no reference.
fn handler_with_other_text(golden: &str, actual: &str, ctx: &Context<'_>) -> bool {
    let combined = |s: &str| s.contains("[handled]") && s != "[handled]";
    ctx.has_flag("variable-handler") && (combined(golden) || combined(actual))
}

/// The key of a dump entry as bytes: `k`, or `khex` for a key that is not UTF-8.
fn entry_key(entry: &J) -> Option<Vec<u8>> {
    if let Some(key) = entry.get("k").and_then(J::as_str) {
        return Some(key.as_bytes().to_vec());
    }
    let hex = entry.get("khex")?.as_str()?;
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

/// §13.2, *The test macros*: under `no-implicit-arrays`, the collection of a repeated name in
/// ARGUMENTS keeps the length of its key in `.seen`'s copy, not its bytes, which depend on memory
/// contents (NUL, a space, `0xc0` on the oracle's machine). `actual_entry` is the crate's entry,
/// whose value is that collection.
fn seen_collected_key(golden: &[u8], actual: &str, actual_entry: &J, ctx: &Context<'_>) -> bool {
    let collection = actual_entry
        .get("v")
        .and_then(J::as_array)
        .and_then(|values| values.first())
        .and_then(|value| value.get("t"))
        .and_then(J::as_str)
        == Some("array");
    ctx.has_flag("registered-macros")
        && ctx.has_flag("no-implicit-arrays")
        && ctx.input_has(b".seen")
        && collection
        && golden.len() == actual.len()
}

/// §5.4: with `kb`, `mb` or `gb`, a float outside the 64-bit signed range is truncated in a way
/// that depends on the platform. True when the input holds such a number whose value in the crate
/// is `actual`.
fn out_of_range_binary_multiplier(actual: &str, ctx: &Context<'_>) -> bool {
    let is_token_byte = |b: &u8| b.is_ascii_alphanumeric() || matches!(*b, b'.' | b'+' | b'-');
    ctx.input
        .split(|b| !is_token_byte(b))
        .filter_map(|token| std::str::from_utf8(token).ok())
        .any(|token| {
            let lower = token.to_ascii_lowercase();
            let Some(number) = ["kb", "mb", "gb"]
                .iter()
                .find_map(|suffix| lower.strip_suffix(suffix))
            else {
                return false;
            };
            // The float, or for the §5.2 quirk the decimal number after the `x`.
            let float = number
                .parse::<f64>()
                .ok()
                .or_else(|| number.split_once('x')?.1.parse::<f64>().ok());
            let out_of_range = float.is_some_and(|f| f.abs() >= 9.223_372_036_854_776e18);
            out_of_range
                && serde_ucl::parse::parse(format!("k = {token}").as_bytes())
                    .ok()
                    .and_then(|v| v.as_object()?.get("k").cloned())
                    == Some(UclValue::Integer(actual.parse().unwrap_or(i64::MIN)))
        })
}

/// The saved comments of a dump node: its key (`"c"` or `"ca"`) and texts.
fn comment_list(node: &J) -> Option<(&'static str, Vec<String>)> {
    ["c", "ca"].into_iter().find_map(|key| {
        let texts = node.get(key)?.as_array()?;
        Some((
            key,
            texts
                .iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect(),
        ))
    })
}

/// The comments of one value (§12.5): the byte after a block comment that ends its unit, which
/// lies outside the unit in libucl, and comments that libucl attached to this value from a value
/// that §8 replaced, which the crate drops.
fn comments(golden: &mut J, actual: &J, ctx: &Context<'_>, reasons: &mut BTreeSet<&'static str>) {
    let (g, a) = (comment_list(golden), comment_list(actual));
    if g == a {
        return;
    }
    let Some((mut g_key, mut g_texts)) = g else {
        return;
    };
    let (a_key, a_texts) = a.unwrap_or(("", Vec::new()));
    let mut changed = false;
    for (g_text, a_text) in g_texts.iter_mut().zip(&a_texts) {
        if g_text != a_text
            && a_text.ends_with("*/")
            && g_text
                .strip_prefix(a_text.as_str())
                .is_some_and(|rest| rest.chars().count() == 1)
            && ends_its_unit(a_text, ctx)
        {
            *g_text = a_text.clone();
            reasons.insert(BLOCK_COMMENT_END);
            changed = true;
        }
    }
    if g_texts != a_texts {
        // A matching text on the actual value cannot be identified as a dropped
        // comment: these notes have no source positions or value creation order.
        let dropped =
            |text: &String| ctx.dropped_comments.contains(text) && !a_texts.contains(text);
        let kept: Vec<String> = g_texts.iter().filter(|t| !dropped(t)).cloned().collect();
        let first_dropped = g_texts.first().is_some_and(dropped);
        if kept.len() < g_texts.len()
            && kept == a_texts
            && (g_key == a_key || first_dropped || a_texts.is_empty())
        {
            g_texts = kept;
            g_key = a_key;
            reasons.insert(REPLACED_COMMENTS);
            changed = true;
        }
    }
    if changed && let Some(node) = golden.as_object_mut() {
        node.remove("c");
        node.remove("ca");
        if !g_texts.is_empty() {
            node.insert(g_key.to_owned(), J::from(g_texts));
        }
    }
}

/// Whether a block comment saved as `text` can end its unit: the input ends with it, or it is
/// not in the input at all, so that it comes from an included file.
fn ends_its_unit(text: &str, ctx: &Context<'_>) -> bool {
    ctx.input.ends_with(text.as_bytes()) || !ctx.input_has(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context<'a>(input: &'a str, flags: &'a [String], dropped: &'a [String]) -> Context<'a> {
        Context {
            input: input.as_bytes(),
            flags,
            dropped_comments: dropped,
            uncertain: &[],
        }
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn string(v: &str) -> J {
        json!({"t": "string", "v": v})
    }

    fn excused(golden: J, actual: &J, ctx: &Context<'_>) -> (bool, BTreeSet<&'static str>) {
        let mut golden = golden;
        let mut reasons = BTreeSet::new();
        excuse(&mut golden, actual, ctx, &mut reasons);
        (&golden == actual, reasons)
    }

    #[test]
    fn nul_bytes_in_copies() {
        let copy = context("d { a = \"\\u0000x\" }\ne { .inherit \"d\" }", &[], &[]);
        let (same, reasons) = excused(string("\0\0"), &string("\0x"), &copy);
        assert!(same);
        assert!(reasons.contains(NUL_IN_COPY));
        // Not without a copy, and not when the bytes before the NUL differ.
        let plain = context("a = \"\\u0000x\"", &[], &[]);
        assert!(!excused(string("\0\0"), &string("\0x"), &plain).0);
        assert!(excused(string("\0 "), &string("\0x"), &copy).0);
        assert!(!excused(string("y\0\0"), &string("x\0x"), &copy).0);
        assert!(!excused(string("\0\0\0"), &string("\0x"), &copy).0);
    }

    #[test]
    fn handler_results_with_other_text() {
        let flags = strings(&["variable-handler"]);
        let ctx = context("a = \"x${H_X}\"", &flags, &[]);
        assert!(excused(string("x"), &string("x[handled]"), &ctx).0);
        let ctx = context("a=${H_$ABI", &flags, &[]);
        assert!(excused(string("[handled]\0\0"), &string("${H_unknown"), &ctx).0);
        assert!(!excused(string("y"), &string("[handled]"), &ctx).0);
        assert!(!excused(string("x"), &string("x[handled]"), &context("", &[], &[])).0);
    }

    #[test]
    fn zerocopy_expanded_emit_excuses_bytes_but_not_nearby_differences() {
        let flags = strings(&["registered-macros", "zerocopy", "string-input"]);
        let emitted = context(".emit $CURDIR 2", &flags, &[]);
        let actual = json!({"t":"object","entries":[{"k":"/tmp/x","v":[{"t":"int","v":"2"}]}]});
        let corrupted =
            json!({"t":"object","entries":[{"khex":"000000000000","v":[{"t":"int","v":"2"}]}]});
        let (same, reasons) = excused(corrupted.clone(), &actual, &emitted);
        assert!(same && reasons.iter().any(|reason| reason.contains("zerocopy")));

        let wrong_number =
            json!({"t":"object","entries":[{"khex":"000000000000","v":[{"t":"int","v":"3"}]}]});
        assert!(!excused(wrong_number, &actual, &emitted).0);
        let wrong_length =
            json!({"t":"object","entries":[{"khex":"0000000000","v":[{"t":"int","v":"2"}]}]});
        assert!(!excused(wrong_length, &actual, &emitted).0);
        for (input, flags) in [
            (
                ".emit $CURDIR 2",
                strings(&["registered-macros", "string-input"]),
            ),
            (".emit $CURDIR 2", strings(&["zerocopy", "string-input"])),
            (
                ".emit $CURDIR 2",
                strings(&["registered-macros", "zerocopy", "no-filevars"]),
            ),
            (".seen $CURDIR", flags.clone()),
            (".emit literal = stable", flags.clone()),
            (".emit $$CURDIR 2", flags.clone()),
            ("direct = $ABI", flags.clone()),
            (".emit $CURDIR 2\nother = 1", flags.clone()),
        ] {
            assert!(
                !excused(corrupted.clone(), &actual, &context(input, &flags, &[])).0,
                "{input}"
            );
        }

        let actual = json!({"t":"object","entries":[{"k":"k","v":[{"t":"string","v":"unknown"}]}]});
        let corrupted =
            json!({"t":"object","entries":[{"k":"x","v":[{"t":"string","v":"xxxxxxx"}]}]});
        let (same, reasons) = excused(corrupted, &actual, &context(".emit k = $ABI", &flags, &[]));
        assert!(same && reasons.iter().any(|reason| reason.contains("zerocopy")));
    }

    #[test]
    fn zerocopy_emit_arguments_do_not_make_literal_value_uncertain() {
        let flags = strings(&["registered-macros", "zerocopy", "string-input"]);
        let actual = json!({"t":"object","entries":[{"k":"k","v":[{"t":"string","v":"stable"}]}]});
        let wrong_key =
            json!({"t":"object","entries":[{"k":"x","v":[{"t":"string","v":"stable"}]}]});
        let wrong_value =
            json!({"t":"object","entries":[{"k":"k","v":[{"t":"string","v":"xxxxxx"}]}]});
        for name in ["ABI", "CURDIR", "FILENAME"] {
            let input = format!(".emit (a=${name}) k=stable");
            let ctx = context(&input, &flags, &[]);
            assert!(!excused(wrong_key.clone(), &actual, &ctx).0, "key: {input}");
            assert!(
                !excused(wrong_value.clone(), &actual, &ctx).0,
                "value: {input}"
            );
        }
    }

    #[test]
    fn zerocopy_emit_comment_before_arguments_or_value_does_not_expand_comment_text() {
        let flags = strings(&["registered-macros", "zerocopy", "string-input"]);
        let actual = json!({"t":"object","entries":[{"k":"k","v":[{"t":"string","v":"stable"}]}]});
        let wrong_key =
            json!({"t":"object","entries":[{"k":"x","v":[{"t":"string","v":"stable"}]}]});
        let wrong_value =
            json!({"t":"object","entries":[{"k":"k","v":[{"t":"string","v":"xxxxxx"}]}]});
        for input in [
            ".emit /* c */(a=$ABI) k=stable",
            ".emit /* c */ (a=$CURDIR) k=stable",
            ".emit /* $ABI */k=stable",
            ".emit /* $FILENAME */k=stable",
            ".emit # $ABI\nk=stable",
        ] {
            let ctx = context(input, &flags, &[]);
            assert!(!excused(wrong_key.clone(), &actual, &ctx).0, "key: {input}");
            assert!(
                !excused(wrong_value.clone(), &actual, &ctx).0,
                "value: {input}"
            );
        }
    }

    #[test]
    fn zerocopy_expanded_emit_before_ignored_dot_is_still_uncertain() {
        let flags = strings(&["zerocopy", "registered-macros", "string-input"]);
        let actual = json!({"t":"object","entries":[{"k":"n","v":[{"t":"string","v":"unknown"}]}]});
        let corrupted = json!({"t":"object","entries":[{"k":"\u{0}","v":[{"t":"string","v":"\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}"}]}]});
        let ctx = context(".emit n $ABI\n.", &flags, &[]);
        let (same, reasons) = excused(corrupted.clone(), &actual, &ctx);
        assert!(same && reasons.contains(ZEROCOPY_EMIT));

        let ctx = context(".emit n $ABI\n.other = 1", &flags, &[]);
        assert!(!excused(corrupted, &actual, &ctx).0);
    }

    #[test]
    fn zerocopy_expanded_emit_before_ignored_name_only_excuses_its_bytes() {
        let flags = strings(&["zerocopy", "registered-macros", "string-input"]);
        let actual = json!({"t":"object","entries":[
            {"k":"r","v":[{"t":"string","v":"unknown"}]}
        ]});
        let oracle = json!({"t":"object","entries":[
            {"k":"\0","v":[{"t":"string","v":"\0\0\0\0\0\0\0"}]}
        ]});
        let input = ".emit r $ABI\n.s";
        let ctx = context(input, &flags, &[]);
        let (same, reasons) = excused(oracle.clone(), &actual, &ctx);
        assert!(same);
        assert_eq!(reasons, BTreeSet::from([ZEROCOPY_EMIT]));

        let without_zerocopy = strings(&["registered-macros", "string-input"]);
        assert!(
            !excused(
                oracle.clone(),
                &actual,
                &context(input, &without_zerocopy, &[])
            )
            .0
        );
        let wrong_key_length = json!({"t":"object","entries":[
            {"k":"\0\0","v":[{"t":"string","v":"\0\0\0\0\0\0\0"}]}
        ]});
        assert!(!excused(wrong_key_length, &actual, &ctx).0);
        let wrong_value_length = json!({"t":"object","entries":[
            {"k":"\0","v":[{"t":"string","v":"\0\0\0\0\0\0"}]}
        ]});
        assert!(!excused(wrong_value_length, &actual, &ctx).0);
        let wrong_value_type = json!({"t":"object","entries":[
            {"k":"\0","v":[{"t":"int","v":"1"}]}
        ]});
        assert!(!excused(wrong_value_type, &actual, &ctx).0);
        let changed_actual_key = json!({"t":"object","entries":[
            {"k":"q","v":[{"t":"string","v":"unknown"}]}
        ]});
        assert!(!excused(oracle.clone(), &changed_actual_key, &ctx).0);
        let changed_actual_value = json!({"t":"object","entries":[
            {"k":"r","v":[{"t":"string","v":"unknowx"}]}
        ]});
        assert!(!excused(oracle.clone(), &changed_actual_value, &ctx).0);

        let stable_actual = json!({"t":"object","entries":[
            {"k":"r","v":[{"t":"string","v":"stable"}]}
        ]});
        let stable_wrong = json!({"t":"object","entries":[
            {"k":"\0","v":[{"t":"string","v":"xxxxxx"}]}
        ]});
        for input in [
            ".emit (a=$ABI) r stable\n.s",
            ".emit /* $ABI */ r stable\n.s",
        ] {
            assert!(
                !excused(
                    stable_wrong.clone(),
                    &stable_actual,
                    &context(input, &flags, &[])
                )
                .0,
                "{input}"
            );
        }
        for input in [
            ".emit r $ABI\n.s ",
            ".emit r $ABI\n.s\nx 1",
            ".emit r $ABI\n.s\n",
        ] {
            assert!(
                !excused(oracle.clone(), &actual, &context(input, &flags, &[])).0,
                "{input:?}"
            );
        }

        let prefixed_actual = json!({"t":"object","entries":[
            {"k":"p","v":[{"t":"int","v":"1"}]},
            {"k":"r","v":[{"t":"string","v":"unknown"}]}
        ]});
        let prefixed_oracle = json!({"t":"object","entries":[
            {"k":"p","v":[{"t":"int","v":"1"}]},
            {"k":"\0","v":[{"t":"string","v":"\0\0\0\0\0\0\0"}]}
        ]});
        let prefix = context("p 1\n.emit r $ABI\n.s", &flags, &[]);
        assert!(excused(prefixed_oracle.clone(), &prefixed_actual, &prefix).0);
        let mut changed_prefix_value = prefixed_oracle.clone();
        changed_prefix_value["entries"][0]["v"][0]["v"] = json!("2");
        assert!(!excused(changed_prefix_value, &prefixed_actual, &prefix).0);
        let mut changed_prefix_key = prefixed_oracle.clone();
        changed_prefix_key["entries"][0]["k"] = json!("q");
        assert!(!excused(changed_prefix_key, &prefixed_actual, &prefix).0);
        let mut extra_entry = prefixed_oracle;
        extra_entry["entries"]
            .as_array_mut()
            .unwrap()
            .push(json!({"k":"x","v":[{"t":"int","v":"3"}]}));
        assert!(!excused(extra_entry, &prefixed_actual, &prefix).0);
    }

    #[test]
    fn zerocopy_emit_after_literal_entry_only_excuses_emitted_entry() {
        let flags = strings(&["zerocopy", "registered-macros", "string-input"]);
        let actual = json!({"t":"object","entries":[
            {"k":"n","v":[{"t":"string","v":"I"}]},
            {"k":"t","v":[{"t":"string","v":"unknown"}]}
        ]});
        let mut corrupted = actual.clone();
        corrupted["entries"][1] =
            json!({"k":"\u{0}","v":[{"t":"string","v":"\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}"}]});
        let ctx = context("n I\n.emit t $ABI", &flags, &[]);
        let (same, reasons) = excused(corrupted.clone(), &actual, &ctx);
        assert!(same && reasons.contains(ZEROCOPY_EMIT));

        // A disagreement in the literal prefix remains a finding.
        let mut wrong_prefix = corrupted.clone();
        wrong_prefix["entries"][0]["v"][0]["v"] = json!("J");
        assert!(!excused(wrong_prefix, &actual, &ctx).0);

        // The recognizer does not cover further parsed text after .emit.
        let mut with_suffix = actual.clone();
        with_suffix["entries"]
            .as_array_mut()
            .unwrap()
            .push(json!({"k":"z","v":[{"t":"int","v":"1"}]}));
        let mut wrong_suffix = with_suffix.clone();
        wrong_suffix["entries"][1] = corrupted["entries"][1].clone();
        wrong_suffix["entries"][2]["v"][0]["v"] = json!("2");
        let ctx = context("n I\n.emit t $ABI\nz 1", &flags, &[]);
        assert!(!excused(wrong_suffix, &with_suffix, &ctx).0);
    }

    #[test]
    fn zerocopy_emit_after_literal_dollar_keeps_prefix_visible() {
        let flags = strings(&["zerocopy", "registered-macros", "string-input"]);
        let actual = json!({"t":"object","entries":[
            {"k":"n","v":[{"t":"string","v":"$"}]},
            {"k":"i","v":[{"t":"string","v":"unknown"}]}
        ]});
        let mut corrupted = actual.clone();
        corrupted["entries"][1] =
            json!({"k":"\u{0}","v":[{"t":"string","v":"\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}"}]});
        let ctx = context("n $\n.emit i $ABI", &flags, &[]);
        assert!(excused(corrupted.clone(), &actual, &ctx).0);
        corrupted["entries"][0]["v"][0]["v"] = json!("x");
        assert!(!excused(corrupted, &actual, &ctx).0);
    }

    #[test]
    fn zerocopy_emit_after_compact_literal_keeps_prefix_visible() {
        let flags = strings(&["zerocopy", "registered-macros", "string-input"]);
        let actual = json!({"t":"object","entries":[
            {"k":"t","v":[{"t":"string","v":"I"}]},
            {"k":"l","v":[{"t":"string","v":"undef"}]}
        ]});
        let mut corrupted = actual.clone();
        corrupted["entries"][1] =
            json!({"k":"\u{0}","v":[{"t":"string","v":"\u{0}\u{0}\u{0}\u{0}\u{0}"}]});
        let ctx = context("t=I\n.emit l $FILENAME", &flags, &[]);
        assert!(excused(corrupted.clone(), &actual, &ctx).0);
        corrupted["entries"][0]["v"][0]["v"] = json!("J");
        assert!(!excused(corrupted, &actual, &ctx).0);
    }

    #[test]
    fn zerocopy_emit_before_literal_entry_keeps_suffix_visible() {
        let flags = strings(&["zerocopy", "registered-macros", "string-input"]);
        let actual = json!({"t":"object","entries":[
            {"k":"i","v":[{"t":"string","v":"unknown"}]},
            {"k":"e","v":[{"t":"string","v":"s"}]}
        ]});
        let mut corrupted = actual.clone();
        corrupted["entries"][0] =
            json!({"k":"\u{0}","v":[{"t":"string","v":"\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}\u{0}"}]});
        let ctx = context(".emit i=$ABI\ne s", &flags, &[]);
        assert!(excused(corrupted.clone(), &actual, &ctx).0);
        corrupted["entries"][1]["v"][0]["v"] = json!("t");
        assert!(!excused(corrupted, &actual, &ctx).0);
    }

    #[test]
    fn keys_of_collections_in_seen_copies() {
        let flags = strings(&["registered-macros", "no-implicit-arrays"]);
        let ctx = context(".seen(n = 1; n = 2) \"v\"", &flags, &[]);
        let array = json!([{"t": "array", "v": []}]);
        let entry = |k: &str| json!({"t": "object", "entries": [{"k": k, "v": array}]});
        assert!(excused(entry("\0"), &entry("n"), &ctx).0);
        assert!(excused(entry(" "), &entry("n"), &ctx).0);
        let khex = json!({"t": "object", "entries": [{"khex": "c0", "v": array}]});
        assert!(excused(khex, &entry("n"), &ctx).0);
        assert!(!excused(entry("\0\0"), &entry("n"), &ctx).0);
        // Only the key of a collection.
        let scalar =
            |k: &str| json!({"t": "object", "entries": [{"k": k, "v": [{"t": "int", "v": "1"}]}]});
        assert!(!excused(scalar("m"), &scalar("n"), &ctx).0);
        let flags = strings(&["registered-macros"]);
        assert!(
            !excused(
                entry("\0"),
                &entry("n"),
                &context(".seen(n = 1) v", &flags, &[])
            )
            .0
        );
    }

    #[test]
    fn out_of_range_floats_with_binary_multipliers() {
        let int = |v: &str| json!({"t": "int", "v": v});
        let crate_value = |text: &str| {
            let v = serde_ucl::parse::parse(format!("k = {text}").as_bytes()).unwrap();
            v.as_object().unwrap()["k"]
                .as_integer()
                .unwrap()
                .to_string()
        };
        for text in ["1e20kb", "-1e20kb", "1.5x99999999999999999999kb", "1e30GB"] {
            let ctx = context(text, &[], &[]);
            let actual = int(&crate_value(text));
            assert!(excused(int("12345"), &actual, &ctx).0, "{text}");
        }
        // An in-range float is specified (§5.4).
        let ctx = context("1.5kb", &[], &[]);
        assert!(!excused(int("1"), &int("1024"), &ctx).0);
        // The crate's value must be that of the number.
        assert!(!excused(int("1"), &int("5"), &context("1e20kb", &[], &[])).0);
    }

    #[test]
    fn comments_of_replaced_values_and_block_comment_ends() {
        let with = |key: &str, texts: &[&str]| json!({"t": "int", "v": "4", key: texts});
        let none = json!({"t": "int", "v": "4"});
        let dropped = strings(&["# c"]);
        let ctx = context("# c\nk = 2\nk = 3\nq = 4", &[], &dropped);
        let (same, reasons) = excused(with("c", &["# c"]), &none, &ctx);
        assert!(same && reasons.contains(REPLACED_COMMENTS));
        assert!(excused(with("c", &["# c", "# d"]), &with("ca", &["# d"]), &ctx).0);
        // Only dropped comments are excused.
        assert!(!excused(with("c", &["# e"]), &none, &ctx).0);
        assert!(!excused(none.clone(), &with("c", &["# c"]), &ctx).0);
        // The byte after a block comment that ends the input.
        let ctx = context("a = 1 /* c */", &[], &[]);
        assert!(excused(with("ca", &["/* c */\0"]), &with("ca", &["/* c */"]), &ctx).0);
        let ctx = context("a = 1 /* c */\n", &[], &[]);
        assert!(!excused(with("ca", &["/* c */\0"]), &with("ca", &["/* c */"]), &ctx).0);
        // Only the last of two equal comments ends the input (fuzz finding, C9).
        let ctx = context("/* c *//* c */", &[], &[]);
        let two = |last: &str| json!({"t": "object", "entries": [], "ca": ["/* c *//", last]});
        assert!(excused(two("/* c */\0"), &two("/* c */"), &ctx).0);
    }
}
