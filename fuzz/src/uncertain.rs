//! The behaviour that the released spec leaves uncertain (spec README, *Uncertain behaviour*),
//! recognised in a difference between the oracle's dump and the crate's, so that the fuzzer skips
//! it and reports every other difference.
//!
//! Each recogniser matches one uncertain rule as narrowly as the dumps and the input allow:
//!
//! | Reason | Spec | What differs |
//! | --- | --- | --- |
//! | [`HANDLER`] | §7.7 | a string where a handler's result shares the string with other text |
//! | [`BINARY_MULTIPLIER`] | §5.4 | an int from an out-of-range float with `kb`, `mb` or `gb` |
//! | [`BLOCK_COMMENT_END`] | §12.5 | the byte saved after a block comment that ends its unit |
//! | [`REPLACED_COMMENTS`] | §12.5 | a replaced value's comments, on a value created later |
//! | [`NUL_IN_COPY`] | §9.7, §13.2 | the bytes after a NUL in a string that a macro copies |
//! | [`SEEN_COLLECTED_KEY`] | §13.2 | a collection's key in `.seen`'s copy of ARGUMENTS |
//! | [`ARRAY_TEXT`] | §9.4, §13.2 | an included file or text in place that starts with `[` |
//!
//! Where the dumps and the input cannot tell an uncertain difference from a nearby specified
//! one, a recogniser excuses more, or reports more, than its rule allows; `fuzz/README.md`,
//! *Known limits*, lists where.
//!
//! Four more cannot be seen in the dumps or the input, so the crate reports when a parse reaches
//! them ([`serde_ucl::parse::Parser::uncertain_reached`]), and any difference of such a parse is
//! skipped ([`reached`]): a macro after a name followed only by comments when the value created
//! most recently is not an object (§9.1), a container of an ended unit at the check at the end of
//! a later included file (§9.4), a `}` in an included file that closes an array element opened
//! by the including unit (§9.4), and a `/` at the end of a glob pattern that leaves out a
//! symbolic link to a regular file, or at the end of a plain include path after the name of a
//! file, which depends on the operating system (§9.4). The rest of the uncertain rules crash
//! libucl, which the fuzzer skips in any case: macro argument documents nested very deep (§9.2),
//! a failure in an included file after which libucl goes on with the same macro (§9.4), and a
//! `}` in a file included under a key where the object holds only its own bracket (§9.4).
//!
//! What `zerocopy` leaves undefined in libucl (§12.2) needs no recogniser: the expected result of
//! a document with `zerocopy` is the one without it, so the oracle runs without the flag
//! ([`crate::run::expectation`]) and the recognisers see the flags of that run. The same
//! holds under `variable-handler` (§7.7): the oracle runs with the names the handler would
//! resolve registered as variables with its value, and the handler recogniser below applies
//! only where that cannot be done.

use serde_json::Value as J;
use serde_ucl::UclValue;
use serde_ucl::parse::{PathSegment, Uncertain};
use std::collections::BTreeSet;

pub const HANDLER: &str = "uncertain: a handler result with other text (§7.7)";
pub const BINARY_MULTIPLIER: &str = "uncertain: an out-of-range float with kb, mb or gb (§5.4)";
pub const BLOCK_COMMENT_END: &str = "uncertain: the byte after a block comment at the end (§12.5)";
pub const REPLACED_COMMENTS: &str = "uncertain: comments of a replaced value (§12.5)";
pub const NUL_IN_COPY: &str = "uncertain: bytes after a NUL in a copied string (§9.7)";
pub const SEEN_COLLECTED_KEY: &str = "uncertain: the key of a collection in .seen's copy (§13.2)";
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
    /// The comments the crate saved, in the order it read them (spec §12.5).
    pub comments: &'a [SavedComment],
    /// The rules the crate's parse reached that its result does not show.
    pub uncertain: &'a [Uncertain],
    /// The bytes of the document's other units in the crate's parse: the files it read and the
    /// texts it parsed in place (`run::CrateNotes::units`).
    pub units: &'a [Vec<u8>],
}

/// A comment the crate saved, with the path of the value it is attached to (as in
/// `oracle::dump`), or `None` when §8 replaced or discarded that value (spec §12.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedComment {
    pub text: String,
    pub value: Option<Vec<PathSegment>>,
}

impl Context<'_> {
    fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    fn input_has(&self, needle: &[u8]) -> bool {
        holds(self.input, needle)
    }

    /// The input or another unit of the document holds `needle`.
    fn some_unit_has(&self, needle: &[u8]) -> bool {
        self.input_has(needle) || self.units.iter().any(|unit| holds(unit, needle))
    }

    /// A unit of the document, the input, an included file or text parsed in place, has a macro
    /// that copies values: `.inherit`, or the test macros `.seen` and `.ctx` (spec §9.7, §13.2),
    /// and `.priority` under `registered-priority-override`, which runs `.seen`'s handler.
    fn copies(&self) -> bool {
        self.some_unit_has(b".inherit")
            || (self.has_flag("registered-macros")
                && (self.some_unit_has(b".seen") || self.some_unit_has(b".ctx")))
            || (self.has_flag("registered-priority-override") && self.some_unit_has(b".priority"))
    }

    /// The document can replace a value in the two ways whose comments §12.5 leaves uncertain:
    /// under `rewrite`, or by a higher priority (§8.3, §8.4). True when the flags set the
    /// `rewrite` strategy or a priority, or some unit holds `rewrite` or `priority` in any letter
    /// case: an include's `duplicate` or `priority`, `.priority`, `.load`'s `priority`, and
    /// `PRIORITY` under `key-lowercase` (§9.2). Comments that the crate dropped in any other
    /// document, such as those pending at a silent stop (§9.4), are compared as they are.
    fn replaces_values(&self) -> bool {
        self.has_flag("strategy:rewrite")
            || self.flags.iter().any(|flag| flag.starts_with("priority:"))
            || [b"rewrite".as_slice(), b"priority"]
                .into_iter()
                .any(|word| {
                    std::iter::once(self.input)
                        .chain(self.units.iter().map(Vec::as_slice))
                        .any(|unit| {
                            unit.windows(word.len())
                                .any(|w| w.eq_ignore_ascii_case(word))
                        })
                })
    }
}

fn holds(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

/// Where the oracle's dump `golden` differs from the crate's `actual` only in behaviour the spec
/// leaves uncertain, rewrites those parts of `golden` to the crate's and adds the reasons.
/// Everything else is left for the comparison that follows. The dumps are walked in parallel,
/// entries and elements by position.
pub fn excuse(golden: &mut J, actual: &J, ctx: &Context<'_>, reasons: &mut BTreeSet<&'static str>) {
    let mut walk = Walk {
        path: Vec::new(),
        used: vec![false; ctx.comments.len()],
    };
    excuse_at(golden, actual, ctx, reasons, &mut walk);
}

/// Where the walk over the two dumps is.
struct Walk {
    /// The path of the value at hand, as `oracle::dump` builds it.
    path: Vec<PathSegment>,
    /// The saved comments ([`Context::comments`]) already taken for another value's list by
    /// [`replaced_comments_first`]: each can reappear only once.
    used: Vec<bool>,
}

fn excuse_at(
    golden: &mut J,
    actual: &J,
    ctx: &Context<'_>,
    reasons: &mut BTreeSet<&'static str>,
    walk: &mut Walk,
) {
    comments(golden, actual, walk, ctx, reasons);
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
            for (g_entry, a_entry) in g_entries.iter_mut().zip(a_entries) {
                if let (Some(g_key), Some(a_key)) =
                    (entry_key(g_entry), a_entry.get("k").and_then(J::as_str))
                    && g_key != a_key.as_bytes()
                    && let Some(entry) = g_entry.as_object_mut()
                    && seen_collected_key(&g_key, a_key, a_entry, ctx)
                {
                    entry.remove("khex");
                    entry.insert("k".to_owned(), J::from(a_key));
                    reasons.insert(SEEN_COLLECTED_KEY);
                }
                if let (Some(g_values), Some(a_values), Some(key)) = (
                    g_entry.get_mut("v").and_then(J::as_array_mut),
                    a_entry.get("v").and_then(J::as_array),
                    a_entry.get("k").and_then(J::as_str),
                ) {
                    for (index, (g, a)) in g_values.iter_mut().zip(a_values).enumerate() {
                        walk.path.push(PathSegment::Key {
                            key: key.to_owned(),
                            index,
                        });
                        excuse_at(g, a, ctx, reasons, walk);
                        walk.path.pop();
                    }
                }
            }
        }
        "array" => {
            if let (Some(g_items), Some(a_items)) = (
                golden.get_mut("v").and_then(J::as_array_mut),
                actual.get("v").and_then(J::as_array),
            ) {
                for (index, (g, a)) in g_items.iter_mut().zip(a_items).enumerate() {
                    walk.path.push(PathSegment::Index(index));
                    excuse_at(g, a, ctx, reasons, walk);
                    walk.path.pop();
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
/// lies outside the unit in libucl, and, in a document that can replace values
/// ([`Context::replaces_values`]), comments that libucl attached to this value from a value that
/// §8 replaced, which the crate drops ([`replaced_comments_first`]).
fn comments(
    golden: &mut J,
    actual: &J,
    walk: &mut Walk,
    ctx: &Context<'_>,
    reasons: &mut BTreeSet<&'static str>,
) {
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
    if g_texts != a_texts
        && ctx.replaces_values()
        && replaced_comments_first(&g_texts, &a_texts, a_key, actual, walk, ctx)
    {
        g_texts = a_texts;
        g_key = a_key;
        reasons.insert(REPLACED_COMMENTS);
        changed = true;
    }
    if changed && let Some(node) = golden.as_object_mut() {
        node.remove("c");
        node.remove("ca");
        if !g_texts.is_empty() {
            node.insert(g_key.to_owned(), J::from(g_texts));
        }
    }
}

/// §12.5: libucl can attach the comments of a value that was replaced, under `rewrite` or by a
/// higher priority, to a value created later, before that value's own. True when the oracle's
/// list `golden` is the crate's list `own` (with the dump key `own_key`) of the value at the
/// walk's path after one or more comments, each of which is a distinct comment the crate saved
/// but attached to no value, with that text, not taken for another value's list already, and
/// read before the first of the value's own comments: so the value was created after the
/// replaced one, even when the texts are the same. The caller asks only in a document that can
/// replace values ([`Context::replaces_values`]); the crate's notes do not say why a comment
/// was dropped, so there a comment of a value discarded at a lower priority, or one pending at a
/// silent stop, is taken for a replaced value's too.
///
/// With no own comments there is nothing to order by, and the comments only have to be ones the
/// crate dropped, so such a value is excused even when it was created before the replaced one;
/// only the crate's own record of where values were created could tell, which it does not give.
/// A container whose own comments come after it is left out: the outermost object of a section
/// path counts as the value created most recently when its bracket closes, though it was created
/// first.
fn replaced_comments_first(
    golden: &[String],
    own: &[String],
    own_key: &str,
    actual: &J,
    walk: &mut Walk,
    ctx: &Context<'_>,
) -> bool {
    let path = walk.path.as_slice();
    let Some(extra) = golden.len().checked_sub(own.len()).filter(|&n| n > 0) else {
        return false;
    };
    if golden[extra..] != *own {
        return false;
    }
    let own_places: Vec<usize> = (0..ctx.comments.len())
        .filter(|&i| ctx.comments[i].value.as_deref() == Some(path))
        .collect();
    if !own_places
        .iter()
        .map(|&i| &ctx.comments[i].text)
        .eq(own.iter())
    {
        return false;
    }
    let container = matches!(
        actual.get("t").and_then(J::as_str),
        Some("object" | "array")
    );
    if own_key == "ca" && container {
        return false;
    }
    let before = own_places.first().copied().unwrap_or(ctx.comments.len());
    let mut taken = walk.used.clone();
    let all_found = golden[..extra].iter().all(|text| {
        let found = (0..before).find(|&i| {
            !taken[i] && ctx.comments[i].value.is_none() && ctx.comments[i].text == *text
        });
        found.inspect(|&i| taken[i] = true).is_some()
    });
    if all_found {
        walk.used = taken;
    }
    all_found
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

    fn context<'a>(
        input: &'a str,
        flags: &'a [String],
        comments: &'a [SavedComment],
    ) -> Context<'a> {
        Context {
            input: input.as_bytes(),
            flags,
            comments,
            uncertain: &[],
            units: &[],
        }
    }

    /// A saved comment attached to the value at `path`, or to none.
    fn saved(text: &str, path: Option<&[PathSegment]>) -> SavedComment {
        SavedComment {
            text: text.to_owned(),
            value: path.map(<[PathSegment]>::to_vec),
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
        // A string without a NUL is compared as it is, copied or not.
        assert!(!excused(string("ab"), &string("ax"), &copy).0);
    }

    /// The copying macro may stand in another unit of the document: an included file, or text
    /// parsed in place (C14 finding `values-differ-244706530c4e7da6`).
    #[test]
    fn nul_bytes_in_copies_made_in_other_units() {
        fn with_units<'a>(ctx: Context<'a>, units: &'a [Vec<u8>]) -> Context<'a> {
            Context { units, ..ctx }
        }
        let flags = strings(&["registered-macros", "string-input"]);
        let input = "a \\u000x\"\no {.include \"files/macro_ctx.inc\"}";
        let ctx_file = [b"a = 1;\n.ctx \"x\"\n".to_vec()];
        let plain_file = [b"x = 1;\n".to_vec()];
        let ctx = |units| with_units(context(input, &flags, &[]), units);
        let (same, reasons) = excused(string("\0\0"), &string("\0\""), &ctx(&ctx_file));
        assert!(same);
        assert_eq!(reasons, BTreeSet::from([NUL_IN_COPY]));
        assert!(!excused(string("\0\0"), &string("\0\""), &ctx(&[])).0);
        assert!(!excused(string("\0\0"), &string("\0\""), &ctx(&plain_file)).0);
        assert!(!excused(string("x\0"), &string("\0\""), &ctx(&ctx_file)).0);
        assert!(!excused(string("\0\0\0"), &string("\0\""), &ctx(&ctx_file)).0);
        // `.seen` and `.ctx` copy only as test macros, which `registered-macros` registers.
        let unregistered = with_units(context(input, &[], &[]), &ctx_file);
        assert!(!excused(string("\0\0"), &string("\0\""), &unregistered).0);
        // Text parsed in place, here made by a variable, so that the input holds no macro.
        let input = "d { a = \"\\u0000x\" }\ne { .emit \"$I d\" }";
        let text = [b".inherit d".to_vec()];
        let in_place = with_units(context(input, &flags, &[]), &text);
        assert!(excused(string("\0\0"), &string("\0x"), &in_place).0);
        let no_text = with_units(context(input, &flags, &[]), &[]);
        assert!(!excused(string("\0\0"), &string("\0x"), &no_text).0);
        // `.priority` runs `.seen`'s handler under `registered-priority-override` (§13.2).
        let input = "a = \"\\u0000x\"\n.priority(a = \"\\u0000x\") 1";
        let overridden = strings(&["registered-priority-override", "string-input"]);
        let ctx = context(input, &overridden, &[]);
        assert!(excused(string("\0\0"), &string("\0x"), &ctx).0);
        let built_in = strings(&["registered-macros", "string-input"]);
        let ctx = context(input, &built_in, &[]);
        assert!(!excused(string("\0\0"), &string("\0x"), &ctx).0);
    }

    /// §12.5: comments of replaced values before a later value's own comments, the same text
    /// included (C14 finding `values-differ-6dc7de42c47fd96a`, QUESTIONS #81), identified by the
    /// crate's notes: which comments it dropped, and the order it read them in. Under `rewrite`,
    /// so that values can be replaced.
    #[test]
    fn comments_of_replaced_values_before_a_later_values_own() {
        let rewrite = strings(&["strategy:rewrite"]);
        let path = [PathSegment::Key {
            key: "a".to_owned(),
            index: 0,
        }];
        let value = |key: &str, texts: &[&str]| {
            json!({"t": "object", "entries": [
                {"k": "a", "v": [{"t": "string", "v": "v", key: texts}]}
            ]})
        };
        let own = value("ca", &["# c"]);
        // Dropped `# c` read first, then the value's own `# c`.
        let notes = vec![saved("# c", None), saved("# c", Some(&path))];
        let ctx = context("", &rewrite, &notes);
        let (same, reasons) = excused(value("c", &["# c", "# c"]), &own, &ctx);
        assert!(same);
        assert_eq!(reasons, BTreeSet::from([REPLACED_COMMENTS]));
        // A distinct text the same way.
        let notes = vec![saved("# x", None), saved("# c", Some(&path))];
        assert!(
            excused(
                value("c", &["# x", "# c"]),
                &own,
                &context("", &rewrite, &notes)
            )
            .0
        );
        // The value's own comment read first: an earlier value, not excused.
        let notes = vec![saved("# c", Some(&path)), saved("# c", None)];
        assert!(
            !excused(
                value("c", &["# c", "# c"]),
                &own,
                &context("", &rewrite, &notes)
            )
            .0
        );
        // Two extra comments with one dropped.
        let notes = vec![saved("# c", None), saved("# c", Some(&path))];
        let ctx = context("", &rewrite, &notes);
        assert!(!excused(value("c", &["# c", "# c", "# c"]), &own, &ctx).0);
        // Nothing dropped, the extra one after the own one, or the own list changed.
        let notes = vec![saved("# c", Some(&path))];
        assert!(
            !excused(
                value("c", &["# c", "# c"]),
                &own,
                &context("", &rewrite, &notes)
            )
            .0
        );
        let notes = vec![saved("# c", Some(&path)), saved("# x", None)];
        assert!(
            !excused(
                value("ca", &["# c", "# x"]),
                &own,
                &context("", &rewrite, &notes)
            )
            .0
        );
        let notes = vec![saved("# x", None), saved("# c", Some(&path))];
        let ctx = context("", &rewrite, &notes);
        assert!(!excused(value("c", &["# x", "# d"]), &own, &ctx).0);
        // A dropped comment with another text.
        assert!(!excused(value("c", &["# y", "# c"]), &own, &ctx).0);
        // The comment is that of another value.
        let other = [PathSegment::Key {
            key: "b".to_owned(),
            index: 0,
        }];
        let notes = vec![saved("# x", Some(&other)), saved("# c", Some(&path))];
        assert!(
            !excused(
                value("c", &["# x", "# c"]),
                &own,
                &context("", &rewrite, &notes)
            )
            .0
        );
        // One dropped comment given to two values: it can reappear once, on the first reached.
        let notes = vec![saved("# c", None), saved("# c", Some(&path))];
        let ctx = context("", &rewrite, &notes);
        let two = |a: J, b: J| {
            json!({"t": "object", "entries": [
                {"k": "a", "v": [a]},
                {"k": "b", "v": [b]}
            ]})
        };
        let a_own = json!({"t": "string", "v": "v", "ca": ["# c"]});
        let a_both = json!({"t": "string", "v": "v", "c": ["# c", "# c"]});
        let b = json!({"t": "int", "v": "1"});
        let b_dropped = json!({"t": "int", "v": "1", "c": ["# c"]});
        let actual = two(a_own, b.clone());
        assert!(excused(two(a_both.clone(), b), &actual, &ctx).0);
        assert!(!excused(two(a_both, b_dropped), &actual, &ctx).0);
        // A container whose own comments come after it: the outermost object of a section path
        // closed by its bracket counts as the most recent value, though created first.
        let object = |key: &str, texts: &[&str]| {
            json!({"t": "object", "entries": [
                {"k": "a", "v": [{"t": "object", "entries": [], key: texts}]}
            ]})
        };
        let notes = vec![saved("# c", None), saved("# z", Some(&path))];
        let ctx = context("", &rewrite, &notes);
        assert!(!excused(object("c", &["# c", "# z"]), &object("ca", &["# z"]), &ctx).0);
        assert!(excused(object("c", &["# c", "# z"]), &object("c", &["# z"]), &ctx).0);
    }

    /// §12.5 leaves uncertain the comments of a value replaced under `rewrite` or by a higher
    /// priority: a dropped comment is excused only where the flags or some unit can replace a
    /// value (review of the C14 classifier gaps, finding 2).
    #[test]
    fn replaced_comments_only_where_values_can_be_replaced() {
        let notes = [saved("# c", None)];
        let dump = |c: &[&str]| {
            let q = if c.is_empty() {
                json!({"t": "int", "v": "4"})
            } else {
                json!({"t": "int", "v": "4", "c": c})
            };
            json!({"t": "object", "entries": [{"k": "q", "v": [q]}]})
        };
        let excused_in = |input: &str, flags: &[&str], units: &[Vec<u8>]| {
            let flags = strings(flags);
            let ctx = Context {
                units,
                ..context(input, &flags, &notes)
            };
            excused(dump(&["# c"]), &dump(&[]), &ctx).0
        };
        let plain = "# c\nk = 2\nk = 3\nq = 4";
        // Nothing can replace a value: under `append` or `merge` with one priority, or at a
        // silent stop, a dropped comment is compared as it is.
        assert!(!excused_in(plain, &[], &[]));
        assert!(!excused_in(
            plain,
            &["strategy:merge", "no-implicit-arrays"],
            &[]
        ));
        assert!(!excused_in(
            &format!("{plain}\n.fail"),
            &["registered-macros"],
            &[]
        ));
        // The flags, the input, or another unit can.
        for flag in ["strategy:rewrite", "priority:2"] {
            assert!(excused_in(plain, &[flag], &[]), "{flag}");
        }
        for input in [
            ".priority 2\n# c\nk = 2",
            ".include(duplicate=\"rewrite\") \"f.inc\"\nq = 4",
            ".include(PRIORITY=2) \"f.inc\"\nq = 4",
            ".load(key=\"k\", priority=3) \"f.txt\"\nq = 4",
        ] {
            assert!(excused_in(input, &[], &[]), "{input}");
        }
        assert!(excused_in(plain, &[], &[b".priority 3\nk = 1".to_vec()]));
        assert!(!excused_in(plain, &[], &[b"k = 1".to_vec()]));
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
        let rewrite = strings(&["strategy:rewrite"]);
        let with = |key: &str, texts: &[&str]| json!({"t": "int", "v": "4", key: texts});
        let none = json!({"t": "int", "v": "4"});
        // The comment `# c` of a replaced value, then the value's own `# d`.
        let notes = [saved("# c", None), saved("# d", Some(&[]))];
        let ctx = context("# c\nk = 2\nk = 3\nq = 4", &rewrite, &notes[..1]);
        let (same, reasons) = excused(with("c", &["# c"]), &none, &ctx);
        assert!(same && reasons.contains(REPLACED_COMMENTS));
        let ctx = context("# c\nk = 2\nk = 3\nq = 4 # d", &rewrite, &notes);
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
