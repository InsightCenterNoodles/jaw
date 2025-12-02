mod cpp;

use std::{fmt::Display, io::Write};

struct OutfileInner {
    out: std::io::BufWriter<std::fs::File>,
    indent: usize,
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

struct Outfile(OutfileInner);

impl<T: Display> std::ops::AddAssign<T> for Outfile {
    fn add_assign(&mut self, rhs: T) {
        self.0.wdisp(&rhs);
    }
}

impl Outfile {
    fn indent<'a>(&'a mut self) -> Indenter<'a> {
        self.0.indent += 1;
        Indenter(&mut self.0)
    }
}

struct Indenter<'a>(&'a mut OutfileInner);

impl<'a, T: Display> std::ops::AddAssign<T> for Indenter<'a> {
    fn add_assign(&mut self, rhs: T) {
        self.0.wdisp(&rhs);
    }
}

impl<'a> Drop for Indenter<'a> {
    fn drop(&mut self) {
        self.0.indent -= 1;
    }
}

impl<'a> Indenter<'a> {
    fn indent<'b>(&'b mut self) -> Indenter<'b> {
        self.0.indent += 1;
        Indenter(&mut self.0)
    }
}

fn thing(x: &mut Outfile) {
    {
        let mut indent = x.indent();

        {
            let mut indent2 = indent.indent();

            indent2 += "this is a test";
        }

        indent += "another test";
    }
}
