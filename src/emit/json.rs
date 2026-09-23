//! JSON, compact JSON (spec §10.4) and YAML (spec §10.6), with multi-value entries as spec §10.7
//! describes. The three share one layout; YAML differs in its keys and at the root.

use super::Writer;
use super::text;
use crate::value::{Entry, UclArray, UclObject, UclValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Style {
    Json,
    Compact,
    Yaml,
}

impl Writer<'_> {
    pub(super) fn json_root(&mut self, value: &UclValue, style: Style) {
        match value {
            UclValue::Object(object) if style == Style::Yaml => self.yaml_root(object),
            _ => self.json_value(value, 0, style),
        }
    }

    /// The root object of YAML: its entries without braces, one per line (§10.6). A multi-value
    /// entry that is not the first is preceded by `,⏎` instead of `⏎` (§10.7, *Quirk*).
    fn yaml_root(&mut self, object: &UclObject) {
        for (i, (key, entry)) in object.iter().enumerate() {
            if i > 0 {
                self.out
                    .push_str(if entry.is_multi() { ",\n" } else { "\n" });
            }
            self.member(key, entry, 0, Style::Yaml);
        }
    }

    fn newline(&mut self, style: Style) {
        if style != Style::Compact {
            self.out.push('\n');
        }
    }

    fn line_indent(&mut self, depth: usize, style: Style) {
        if style != Style::Compact {
            self.indent(depth);
        }
    }

    /// A value whose line is indented `depth` levels.
    fn json_value(&mut self, value: &UclValue, depth: usize, style: Style) {
        match value {
            UclValue::Object(object) => self.json_object(object, depth, style),
            UclValue::Array(items) => self.json_array(items, depth, style),
            scalar => self.scalar(scalar),
        }
    }

    fn json_object(&mut self, object: &UclObject, depth: usize, style: Style) {
        if object.is_empty() {
            self.out.push_str("{}");
            return;
        }
        self.out.push('{');
        self.newline(style);
        for (i, (key, entry)) in object.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
                self.newline(style);
            }
            self.line_indent(depth + 1, style);
            self.member(key, entry, depth + 1, style);
        }
        self.newline(style);
        self.line_indent(depth, style);
        self.out.push('}');
    }

    fn json_array(&mut self, items: &UclArray, depth: usize, style: Style) {
        if items.is_empty() {
            self.out.push_str("[]");
            return;
        }
        self.out.push('[');
        self.newline(style);
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
                self.newline(style);
            }
            self.line_indent(depth + 1, style);
            self.enter_index(i);
            self.json_value(item, depth + 1, style);
            self.leave();
        }
        self.newline(style);
        self.line_indent(depth, style);
        self.out.push(']');
    }

    /// `key: value` of an object member whose line is indented `depth` levels. The key is that
    /// of the entry's first value (§10.1).
    fn member(&mut self, key: &str, entry: &Entry, depth: usize, style: Style) {
        self.enter_key(key, 0);
        self.member_key(key, style);
        self.leave();
        self.out.push(':');
        if style != Style::Compact {
            self.out.push(' ');
        }
        if !entry.is_multi() {
            self.enter_key(key, 0);
            self.json_value(entry.first(), depth, style);
            self.leave();
            return;
        }
        let first = entry.first();
        if first.is_array() {
            // Only the first value is written when it is an explicit array (§10.7, *Quirk*).
            self.enter_key(key, 0);
            self.json_value(first, depth, style);
            self.leave();
            return;
        }
        self.enter_key(key, 0);
        let kept_layout = self.facts().is_some_and(|f| f.normal_layout);
        self.leave();
        let normal = kept_layout
            || match first {
                UclValue::String(s) => !s.is_empty(),
                UclValue::Object(o) => !o.is_empty(),
                _ => false,
            };
        self.out.push('[');
        if normal {
            self.newline(style);
        }
        for (i, value) in entry.values().enumerate() {
            if i > 0 {
                self.out.push(',');
                self.newline(style);
            }
            self.line_indent(depth + 1, style);
            self.enter_key(key, i);
            self.json_value(value, depth + 1, style);
            self.leave();
        }
        if normal {
            self.newline(style);
            self.line_indent(depth, style);
        }
        self.out.push(']');
    }

    /// A member key: always in the JSON form in JSON, bare unless it needs quoting in YAML; the
    /// empty key is `null` in both (§10.1).
    fn member_key(&mut self, key: &str, style: Style) {
        let spelling = self
            .facts()
            .and_then(|f| f.key_spelling.clone())
            .unwrap_or_else(|| key.to_owned());
        if spelling.is_empty() {
            self.out.push_str("null");
        } else if style == Style::Yaml {
            self.write_key(key);
        } else {
            text::write_json_string(&mut self.out, &spelling);
        }
    }
}
