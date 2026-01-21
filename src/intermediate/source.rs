use std::fmt::Display;

#[derive(Debug, Clone)]
pub struct SourceCode(pub std::sync::Arc<String>);

impl SourceCode {
    /// Converts a 0-based line/column pair into a `SourceLocation` bound to this source.
    pub fn location(&self, line: usize, column: usize) -> SourceLocation {
        SourceLocation {
            source: self.clone(),
            position: Position { line, column },
        }
    }

    /// Converts a `Position` into a `SourceLocation` bound to this source.
    pub fn position(&self, position: Position) -> SourceLocation {
        SourceLocation {
            source: self.clone(),
            position,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct SourceLocation {
    source: SourceCode,
    position: Position,
}

impl SourceLocation {
    /// Constructs a source location from a source buffer and a position.
    pub fn new(source: SourceCode, position: Position) -> Self {
        Self { source, position }
    }
}

impl Display for SourceLocation {
    /// Formats a human-readable snippet (best-effort) for diagnostics.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.source.0.lines().nth(self.position.line) {
            if let Some((a, b)) = line.split_at_checked(self.position.column) {
                return write!(f, "line {}: {}↪{}", self.position.line, a, b);
            }
        }

        write!(f, "unknown location")
    }
}
