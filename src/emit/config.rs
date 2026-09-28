//! The UCL config format (spec §10.5), with saved comments when they are asked for (§10.10).

use super::text;
use super::{Mode, Writer};
use crate::parse::CommentPlacement;
use crate::value::{Array, Object, Value};

impl Writer<'_> {
    /// The root: an object's entries without braces, or an array in brackets without a final line
    /// break (§10.5). Its comments go before and after the whole output (§10.10).
    pub(super) fn config_root(&mut self, value: &Value<'_>) {
        self.comments_before(0);
        match value {
            Value::Object(object) => self.config_entries(object, 0),
            Value::Array(items) if items.is_empty() => self.out.push_str("[]"),
            Value::Array(items) => {
                self.out.push_str("[\n");
                self.config_elements(items, 1);
                self.out.push(']');
            }
            scalar => self.config_scalar(scalar),
        }
        self.comments_after(0);
    }

    /// Every value of every entry of `object`, each with its own key (§10.1, §10.5).
    fn config_entries(&mut self, object: &Object<'_>, depth: usize) {
        for (position, (key, entry)) in object.iter().enumerate() {
            for (index, value) in entry.values().enumerate() {
                self.enter_entry(position, key, index);
                self.config_entry(key, value, depth);
                self.leave();
            }
        }
    }

    /// One entry line (and the lines of its container) at `depth`.
    fn config_entry(&mut self, key: &str, value: &Value<'_>, depth: usize) {
        self.indent(depth);
        self.comments_before(depth);
        self.write_key(key);
        match value {
            Value::Object(object) if object.is_empty() => self.out.push_str(" {}\n"),
            Value::Object(object) => {
                self.out.push_str(" {\n");
                self.config_entries(object, depth + 1);
                self.indent(depth);
                self.out.push_str("}\n");
            }
            Value::Array(items) if items.is_empty() => self.out.push_str(" []\n"),
            Value::Array(items) => {
                self.out.push_str(" [\n");
                self.config_elements(items, depth + 1);
                self.indent(depth);
                self.out.push_str("]\n");
            }
            scalar => {
                self.out.push_str(" = ");
                self.config_scalar(scalar);
                self.out.push_str(";\n");
            }
        }
        self.comments_after(depth);
    }

    /// The elements of an array, at `depth`: scalars followed by `,`, containers without (§10.5).
    fn config_elements(&mut self, items: &Array<'_>, depth: usize) {
        for (index, item) in items.iter().enumerate() {
            self.enter_index(index);
            self.indent(depth);
            self.comments_before(depth);
            match item {
                Value::Object(object) if object.is_empty() => self.out.push_str("{}\n"),
                Value::Object(object) => {
                    self.out.push_str("{\n");
                    self.config_entries(object, depth + 1);
                    self.indent(depth);
                    self.out.push_str("}\n");
                }
                Value::Array(inner) if inner.is_empty() => self.out.push_str("[]\n"),
                Value::Array(inner) => {
                    self.out.push_str("[\n");
                    self.config_elements(inner, depth + 1);
                    self.indent(depth);
                    self.out.push_str("]\n");
                }
                scalar => {
                    self.config_scalar(scalar);
                    self.out.push_str(",\n");
                }
            }
            self.comments_after(depth);
            self.leave();
        }
    }

    /// A scalar; strings in the heredoc, single-quoted or JSON form (§10.5).
    fn config_scalar(&mut self, value: &Value<'_>) {
        if self.mode == Mode::RoundTrip {
            self.exact_scalar(value, true);
            return;
        }
        match value {
            Value::String(s) => {
                let (single_quoted, multiline) = self
                    .facts()
                    .map_or((false, false), |f| (f.single_quoted, f.multiline));
                text::write_config_string(&mut self.out, s, single_quoted, multiline);
            }
            other => self.scalar(other),
        }
    }

    /// Comments attached before the value being written, at the start of its line after its
    /// indentation: each followed by a line break and the indentation again (§10.10).
    fn comments_before(&mut self, depth: usize) {
        for text in self.comments(CommentPlacement::Before) {
            self.out.push_str(text);
            self.out.push('\n');
            self.indent(depth);
        }
    }

    /// Comments attached after the value being written, after all of its text: each followed by
    /// a line break, the first at the start of the line and the others after the value's
    /// indentation (§10.10, *Quirk*).
    fn comments_after(&mut self, depth: usize) {
        for (i, text) in self
            .comments(CommentPlacement::After)
            .into_iter()
            .enumerate()
        {
            if i > 0 {
                self.indent(depth);
            }
            self.out.push_str(text);
            self.out.push('\n');
        }
    }
}
