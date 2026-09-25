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
//! | [`ARRAY_TEXT`] | §9.4, §13.2 | an included file, or text parsed in place, that starts with `[` and holds more |
//!
//! Three more cannot be seen in the dumps or the input, so the crate reports when a parse reaches
//! them ([`ucl_lexer::parse::Parser::uncertain_reached`]), and any difference of such a parse is
//! skipped ([`reached`]): a macro after a name followed only by comments when the value created
//! most recently is not an object (§9.1), a container of an ended unit at the check at the end of
//! a later included file (§9.4), and a `}` in an included file that closes an array element
//! opened by the including unit (§9.4). The rest of the uncertain rules crash libucl, which the
//! fuzzer skips in any case: macro argument documents nested very deep (§9.2), a failure in an
//! included file after which libucl goes on with the same macro (§9.4), and a `}` in a file
//! included under a key where the object holds only its own bracket (§9.4).

use serde_json::Value as J;
use std::collections::BTreeSet;
use ucl_lexer::UclValue;
use ucl_lexer::parse::Uncertain;

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

/// The reason for a difference in a parse that reached `rule`.
pub fn reached(rule: Uncertain) -> &'static str {
    match rule {
        Uncertain::ReopenedNotObject => REOPENED_NOT_OBJECT,
        Uncertain::EndedUnitContainer => ENDED_UNIT,
        Uncertain::ClosedArrayElement => CLOSED_ARRAY_ELEMENT,
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
}

/// Where the oracle's dump `golden` differs from the crate's `actual` only in behaviour the spec
/// leaves uncertain, rewrites those parts of `golden` to the crate's and adds the reasons.
/// Everything else is left for the comparison that follows. The dumps are walked in parallel,
/// entries and elements by position.
pub fn excuse(golden: &mut J, actual: &J, ctx: &Context<'_>, reasons: &mut BTreeSet<&'static str>) {
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
            for (g_entry, a_entry) in g_entries.iter_mut().zip(a_entries) {
                if let (Some(g_key), Some(a_key)) =
                    (entry_key(g_entry), a_entry.get("k").and_then(J::as_str))
                    && g_key != a_key.as_bytes()
                    && seen_collected_key(&g_key, a_key, a_entry, ctx)
                    && let Some(entry) = g_entry.as_object_mut()
                {
                    entry.remove("khex");
                    entry.insert("k".to_owned(), J::from(a_key));
                    reasons.insert(SEEN_COLLECTED_KEY);
                }
                if let (Some(g_values), Some(a_values)) = (
                    g_entry.get_mut("v").and_then(J::as_array_mut),
                    a_entry.get("v").and_then(J::as_array),
                ) {
                    for (g, a) in g_values.iter_mut().zip(a_values) {
                        excuse(g, a, ctx, reasons);
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
                    excuse(g, a, ctx, reasons);
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
                && ucl_lexer::parse::parse(format!("k = {token}").as_bytes())
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
            let v = ucl_lexer::parse::parse(format!("k = {text}").as_bytes()).unwrap();
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
