use crate::module::{Module, TypeID};

/// Minimal text emitter with indentation and name lookup helpers
pub struct TextEmitter<'a> {
    pub module: &'a Module,
    pub out: String,
    pub indent: usize,
}

impl<'a> TextEmitter<'a> {
    pub fn new(module: &'a Module) -> Self {
        Self {
            module,
            out: String::new(),
            indent: 0,
        }
    }

    pub fn finish(self) -> String {
        self.out
    }

    pub fn wln(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        assert!(self.indent > 0);
        self.indent -= 1;
    }

    pub fn name_of(&self, tid: TypeID) -> Option<&str> {
        self.module.inv_type_map.get(&tid).map(|s| s.as_str())
    }
}

/// Sanitize an identifier: allow [A-Za-z0-9_], replace others with '_',
/// and prefix '_' if starting with a digit. Fallback to `default` if empty.
pub fn sanitize_ident(raw: &str, default: &str) -> String {
    let mut s = String::new();
    for (i, ch) in raw.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if i == 0 && ch.is_ascii_digit() {
                s.push('_');
            }
            s.push(ch);
        } else {
            if i == 0 {
                s.push('_');
            }
            s.push('_');
        }
    }
    if s.is_empty() {
        s.push_str(default);
    }
    s
}

