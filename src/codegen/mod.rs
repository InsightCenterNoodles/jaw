mod cpp;
mod python;
mod rust;

use std::{fmt::Display, io::Write, path::Path};

use crate::{GlobalOptions, compile::World};
use anyhow::Context;

// MARK: Inner

struct OutfileInner {
    out: std::io::BufWriter<std::fs::File>,
    indent: usize,
    indent_char: &'static str,
    dedent_char: &'static str,
}

impl Write for OutfileInner {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.out.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

impl OutfileInner {
    fn wln(&mut self, s: &str) {
        for _ in 0..self.indent {
            write!(self, "    ").unwrap();
        }
        writeln!(self, "{s}").unwrap();
    }

    fn wdisp<T: Display>(&mut self, s: &T) {
        for _ in 0..self.indent {
            write!(self, "    ").unwrap();
        }
        writeln!(self, "{s}").unwrap();
    }
}

impl Drop for OutfileInner {
    fn drop(&mut self) {
        self.out.flush().unwrap();
    }
}

// MARK: Outfile

struct Outfile(OutfileInner);

impl<T: Display> std::ops::AddAssign<T> for Outfile {
    fn add_assign(&mut self, rhs: T) {
        self.0.wdisp(&rhs);
    }
}

impl Outfile {
    fn wln(&mut self, s: &str) {
        self.0.wln(s);
    }

    #[allow(dead_code)]
    fn wdisp<T: Display>(&mut self, s: &T) {
        self.0.wdisp(s);
    }

    fn indent<'a>(&'a mut self) -> Indenter<'a> {
        if !self.0.indent_char.is_empty() {
            self.wln(self.0.indent_char);
        }
        self.0.indent += 1;
        Indenter(&mut self.0)
    }
}

impl Write for Outfile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

// MARK: Indenter
struct Indenter<'a>(&'a mut OutfileInner);

impl<'a, T: Display> std::ops::AddAssign<T> for Indenter<'a> {
    fn add_assign(&mut self, rhs: T) {
        self.0.wdisp(&rhs);
    }
}

impl<'a> Drop for Indenter<'a> {
    fn drop(&mut self) {
        self.0.indent -= 1;
        if !self.0.dedent_char.is_empty() {
            self.wln(self.0.dedent_char);
        }
    }
}

impl<'a> Indenter<'a> {
    fn wln(&mut self, s: &str) {
        self.0.wln(s);
    }

    #[allow(dead_code)]
    fn wdisp<T: Display>(&mut self, s: &T) {
        self.0.wdisp(s);
    }

    fn indent<'b>(&'b mut self) -> Indenter<'b> {
        if !self.0.indent_char.is_empty() {
            self.wln(self.0.indent_char);
        }
        self.0.indent += 1;
        Indenter(self.0)
    }
}

impl<'a> Write for Indenter<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

// MARK: Utils

fn open_outfile(
    path: &Path,
    indent_char: &'static str,
    dedent_char: &'static str,
) -> anyhow::Result<Outfile> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("opening output file {}", path.display()))?;
    Ok(Outfile(OutfileInner {
        out: std::io::BufWriter::new(file),
        indent: 0,
        indent_char,
        dedent_char,
    }))
}

pub fn emit_cpp(
    world: &World,
    global: &GlobalOptions,
    path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut out = open_outfile(path, "{", "}")?;
    cpp::emit(world, global, &mut out)
        .with_context(|| format!("while generating C++ into {}", path.display()))
}

pub fn emit_python(
    world: &World,
    global: &GlobalOptions,
    path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut out = open_outfile(path, "", "")?;
    python::emit(world, global, &mut out)
        .with_context(|| format!("while generating Python into {}", path.display()))
}

pub fn emit_rust(
    world: &World,
    global: &GlobalOptions,
    path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut out = open_outfile(path, "{", "}")?;
    rust::emit(world, global, &mut out)
        .with_context(|| format!("while generating Rust into {}", path.display()))
}

trait Sink {
    fn wln(&mut self, s: &str);
    fn indent(&mut self) -> Indenter<'_>;

    fn newline(&mut self) {
        self.wln("");
    }
}

impl Sink for Outfile {
    fn wln(&mut self, s: &str) {
        Outfile::wln(self, s);
    }

    fn indent(&mut self) -> Indenter<'_> {
        Outfile::indent(self)
    }
}

impl<'a> Sink for Indenter<'a> {
    fn wln(&mut self, s: &str) {
        Indenter::wln(self, s);
    }

    fn indent(&mut self) -> Indenter<'_> {
        Indenter::indent(self)
    }
}

impl<T: Sink + ?Sized> Sink for &mut T {
    fn wln(&mut self, s: &str) {
        (**self).wln(s);
    }

    fn indent(&mut self) -> Indenter<'_> {
        (**self).indent()
    }
}
