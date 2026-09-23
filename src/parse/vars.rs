//! Variable expansion (spec §7).

use super::VariableHandler;

/// Expands `$NAME`, `${NAME}` and `$$` in string values.
pub(crate) struct Expander<'a> {
    /// Registered variables in lookup order (spec §7.1).
    variables: &'a [(String, String)],
    handler: Option<&'a mut VariableHandler>,
    enabled: bool,
}

impl<'a> Expander<'a> {
    pub(crate) fn new(
        variables: &'a [(String, String)],
        handler: Option<&'a mut VariableHandler>,
        enabled: bool,
    ) -> Self {
        Self {
            variables,
            handler,
            enabled,
        }
    }

    /// Expands the references in `text`.
    ///
    /// If no reference is replaced, the text is returned exactly as written, `$$` included
    /// (spec §7.5). Otherwise `$$` stands for `$`.
    pub(crate) fn expand(&mut self, text: Vec<u8>) -> Vec<u8> {
        if !self.enabled || !text.contains(&b'$') {
            return text;
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
        if replaced { out } else { text }
    }

    /// A registered variable whose name equals `name`, or the handler's answer (spec §7.3, §7.7).
    fn lookup_braced(&mut self, name: &[u8]) -> Option<String> {
        if let Some((_, value)) = self.variables.iter().find(|(n, _)| n.as_bytes() == name) {
            return Some(value.clone());
        }
        let handler = self.handler.as_mut()?;
        let name = std::str::from_utf8(name).ok()?;
        handler(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn expand(variables: &[(String, String)], text: &str) -> String {
        let mut e = Expander::new(variables, None, true);
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
        let mut e = Expander::new(&v, Some(&mut handler), true);
        let mut run = |t: &str| String::from_utf8(e.expand(t.as_bytes().to_vec())).unwrap();
        assert_eq!(run("${H_X}"), "[h]");
        assert_eq!(run("$H_X"), "$H_X");
        assert_eq!(run("${H_REG}"), "registered");
        assert_eq!(run("${OTHER}"), "${OTHER}");
    }

    #[test]
    fn disabled_expansion() {
        let v = vars(&[("ABI", "unknown")]);
        let mut off = Expander::new(&v, None, false);
        assert_eq!(off.expand(b"$ABI".to_vec()), b"$ABI");
    }
}
