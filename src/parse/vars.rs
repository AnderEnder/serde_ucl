//! Variable expansion (spec §7).

use smallvec::SmallVec;
use std::borrow::Cow;

/// A variable: its name and value, borrowed where they can be, from the parser's registered
/// variables or from constants (clean-room work item C13).
pub(crate) type Variable<'a> = (Cow<'a, str>, Cow<'a, str>);

/// The variables in lookup order (spec §7.1). The two file variables fit inline, so that a parse
/// that registers no variables allocates nothing for them (clean-room work item C13).
pub(crate) type Variables<'a> = SmallVec<[Variable<'a>; 2]>;

/// A variable handler as the expander calls it: [`super::VariableHandler`], or a closure of the
/// parser around it.
pub(crate) type Handler<'a> = dyn FnMut(&str) -> Option<String> + 'a;

/// Expands `$NAME`, `${NAME}` and `$$` in string values.
pub(crate) struct Expander<'a> {
    /// Registered variables in lookup order (spec §7.1).
    variables: Variables<'a>,
    handler: Option<Box<Handler<'a>>>,
    enabled: bool,
}

/// The file variables' values from before an included file set them (see
/// [`Expander::enter_file`]); `None` for one that was not defined.
#[derive(Debug)]
pub(crate) struct SavedFileVars<'a>([Option<Cow<'a, str>>; 2]);

const FILE_VARS: [&str; 2] = ["FILENAME", "CURDIR"];

impl<'a> Expander<'a> {
    pub(crate) fn new(
        variables: Variables<'a>,
        handler: Option<Box<Handler<'a>>>,
        enabled: bool,
    ) -> Self {
        Self {
            variables,
            handler,
            enabled,
        }
    }

    /// Replaces the variables, in lookup order.
    pub(crate) fn set_variables(&mut self, variables: Variables<'a>) {
        self.variables = variables;
    }

    /// Sets `FILENAME` and `CURDIR` for an input given as a file (spec §13.1, *File variables
    /// and paths*): each keeps its place in the lookup order if it is defined, and is added at
    /// the end otherwise (oracle runs). They keep these values for later inputs.
    pub(crate) fn set_file_vars(&mut self, filename: String, curdir: String) {
        for (name, value) in FILE_VARS.into_iter().zip([filename, curdir]) {
            match self.variables.iter_mut().find(|(n, _)| n == name) {
                Some(slot) => slot.1 = Cow::Owned(value),
                None => self
                    .variables
                    .push((Cow::Borrowed(name), Cow::Owned(value))),
            }
        }
    }

    /// Sets `FILENAME` and `CURDIR` for an included file (spec §9.4), whatever the registered
    /// variables and `NO_FILEVARS` say. As the oracle does, they move to the end of the lookup
    /// order, after every registered variable, and stay there (QUESTIONS.md #28).
    pub(crate) fn enter_file(&mut self, filename: String, curdir: String) -> SavedFileVars<'a> {
        let mut saved = [None, None];
        for (slot, (name, value)) in saved
            .iter_mut()
            .zip(FILE_VARS.into_iter().zip([filename, curdir]))
        {
            if let Some(i) = self.variables.iter().position(|(n, _)| n == name) {
                *slot = Some(self.variables.remove(i).1);
            }
            self.variables
                .push((Cow::Borrowed(name), Cow::Owned(value)));
        }
        SavedFileVars(saved)
    }

    /// Gives `FILENAME` and `CURDIR` back the values they had before [`Expander::enter_file`].
    /// One that was not defined then keeps the included file's value (spec §9.4, §12.7,
    /// *Quirk*).
    pub(crate) fn leave_file(&mut self, saved: SavedFileVars<'a>) {
        for (name, value) in FILE_VARS.into_iter().zip(saved.0) {
            if let (Some(value), Some(slot)) =
                (value, self.variables.iter_mut().find(|(n, _)| n == name))
            {
                slot.1 = value;
            }
        }
    }

    /// Expands the references in `text`.
    ///
    /// If no reference is replaced, the text is returned exactly as written, `$$` included
    /// (spec §7.5). Otherwise `$$` stands for `$`.
    pub(crate) fn expand(&mut self, text: Vec<u8>) -> Vec<u8> {
        match self.expand_bytes(&text) {
            Some(expanded) => expanded,
            None => text,
        }
    }

    /// [`Expander::expand`] of `text`, or `None` when no reference was replaced, so that the
    /// text stays exactly as written (clean-room work item C13: it can then be borrowed).
    pub(crate) fn expand_bytes(&mut self, text: &[u8]) -> Option<Vec<u8>> {
        if !self.enabled || !text.contains(&b'$') {
            return None;
        }
        let mut out = Vec::with_capacity(text.len());
        let mut replaced = false;
        let mut i = 0;
        while i < text.len() {
            let byte = text[i];
            if byte != b'$' {
                out.push(byte);
                i += 1;
                continue;
            }
            match text.get(i + 1) {
                None => {
                    out.push(b'$');
                    i += 1;
                }
                Some(b'{') => {
                    // §7.3: NAME runs to the first `}`. An unresolved or unclosed `${` is kept,
                    // and scanning resumes right after it, so references inside are expanded.
                    let start = i + 2;
                    let resolved = text[start..]
                        .iter()
                        .position(|&b| b == b'}')
                        .and_then(|len| {
                            let name = &text[start..start + len];
                            self.lookup_braced(name).map(|v| (v, start + len + 1))
                        });
                    match resolved {
                        Some((value, next)) => {
                            out.extend_from_slice(value.as_bytes());
                            replaced = true;
                            i = next;
                        }
                        None => {
                            out.extend_from_slice(b"${");
                            i += 2;
                        }
                    }
                }
                Some(b'$') => {
                    out.push(b'$');
                    i += 2;
                }
                Some(_) => {
                    // §7.4: the first registered name, in lookup order, that is a prefix of the
                    // following text.
                    let rest = &text[i + 1..];
                    let found = self
                        .variables
                        .iter()
                        .find(|(name, _)| !name.is_empty() && rest.starts_with(name.as_bytes()));
                    match found {
                        Some((name, value)) => {
                            out.extend_from_slice(value.as_bytes());
                            replaced = true;
                            i += 1 + name.len();
                        }
                        None => {
                            out.push(b'$');
                            i += 1;
                        }
                    }
                }
            }
        }
        replaced.then_some(out)
    }

    /// A registered variable whose name equals `name`, or the handler's answer (spec §7.3, §7.7).
    /// A registered variable's value is lent, not copied (clean-room work item C13).
    fn lookup_braced(&mut self, name: &[u8]) -> Option<Cow<'_, str>> {
        if let Some((_, value)) = self.variables.iter().find(|(n, _)| n.as_bytes() == name) {
            return Some(Cow::Borrowed(value));
        }
        let handler = self.handler.as_mut()?;
        let name = std::str::from_utf8(name).ok()?;
        handler(name).map(Cow::Owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars<'a>(pairs: &[(&str, &str)]) -> Variables<'a> {
        pairs
            .iter()
            .map(|(n, v)| (Cow::Owned(n.to_string()), Cow::Owned(v.to_string())))
            .collect()
    }

    fn expand(variables: &Variables<'_>, text: &str) -> String {
        let mut e = Expander::new(variables.clone(), None, true);
        String::from_utf8(e.expand(text.as_bytes().to_vec())).unwrap()
    }

    #[test]
    fn braced_unbraced_and_unknown() {
        let v = vars(&[("ABI", "unknown")]);
        assert_eq!(expand(&v, "x${ABI}y"), "xunknowny");
        assert_eq!(expand(&v, "$ABI/x"), "unknown/x");
        assert_eq!(expand(&v, "$ABItest"), "unknowntest");
        assert_eq!(expand(&v, "${unknown}"), "${unknown}");
        assert_eq!(expand(&v, "${}"), "${}");
        assert_eq!(expand(&v, "${$ABI}"), "${unknown}");
        assert_eq!(expand(&v, "${ABI"), "${ABI");
        assert_eq!(expand(&v, "${$ABI"), "${unknown");
        assert_eq!(expand(&v, "$unknown"), "$unknown");
        assert_eq!(expand(&v, "a$"), "a$");
    }

    #[test]
    fn dollar_dollar_only_after_a_replacement() {
        // spec §7.5 table
        let v = vars(&[("ABI", "unknown")]);
        assert_eq!(expand(&v, "$$test"), "$$test");
        assert_eq!(expand(&v, "$$ABI"), "$$ABI");
        assert_eq!(expand(&v, "$ABI$$ABI"), "unknown$ABI");
        assert_eq!(expand(&v, "$ABI$$"), "unknown$");
        assert_eq!(expand(&v, "$ABI$$$"), "unknown$$");
        assert_eq!(expand(&v, "$ABI$$ABI$$$ABI$$$$"), "unknown$ABI$unknown$$");
    }

    #[test]
    fn registration_order_decides_prefix_matches() {
        let v = vars(&[("ABI", "unknown"), ("AB", "short")]);
        assert_eq!(expand(&v, "$AB $ABI $ABIX"), "short unknown unknownX");
        let v = vars(&[("ABI", "unknown"), ("ABIX", "long")]);
        assert_eq!(expand(&v, "$AB $ABI $ABIX"), "$AB unknown unknownX");
    }

    #[test]
    fn handler_only_for_braced_unregistered_names() {
        let v = vars(&[("H_REG", "registered")]);
        let mut handler =
            |name: &str| -> Option<String> { name.starts_with("H_").then(|| "[h]".to_string()) };
        let mut e = Expander::new(v, Some(Box::new(&mut handler)), true);
        let mut run = |t: &str| String::from_utf8(e.expand(t.as_bytes().to_vec())).unwrap();
        assert_eq!(run("${H_X}"), "[h]");
        assert_eq!(run("$H_X"), "$H_X");
        assert_eq!(run("${H_REG}"), "registered");
        assert_eq!(run("${OTHER}"), "${OTHER}");
    }

    #[test]
    fn disabled_expansion() {
        let v = vars(&[("ABI", "unknown")]);
        let mut off = Expander::new(v, None, false);
        assert_eq!(off.expand(b"$ABI".to_vec()), b"$ABI");
    }

    #[test]
    fn file_variables_of_an_included_file() {
        // Oracle runs (QUESTIONS.md #28): the included file's FILENAME and CURDIR come after
        // the registered variables, so `FILE` wins an unbraced `$FILENAME`.
        let v = vars(&[("FILENAME", "main"), ("CURDIR", "/m"), ("FILE", "f")]);
        let mut e = Expander::new(v, None, true);
        let run = |e: &mut Expander<'_>, t: &str| {
            String::from_utf8(e.expand(t.as_bytes().to_vec())).unwrap()
        };
        assert_eq!(run(&mut e, "$FILENAME ${FILENAME}"), "main main");
        let saved = e.enter_file("/i/x.inc".into(), "/i".into());
        assert_eq!(
            run(&mut e, "$FILENAME ${FILENAME} $CURDIR"),
            "fNAME /i/x.inc /i"
        );
        e.leave_file(saved);
        assert_eq!(
            run(&mut e, "$FILENAME ${FILENAME} $CURDIR"),
            "fNAME main /m"
        );
        // Not defined before (NO_FILEVARS): the included file's values stay.
        let mut e = Expander::new(vars(&[("ABI", "unknown")]), None, true);
        let saved = e.enter_file("/i/x.inc".into(), "/i".into());
        e.leave_file(saved);
        assert_eq!(run(&mut e, "${FILENAME} ${CURDIR}"), "/i/x.inc /i");
    }
}
