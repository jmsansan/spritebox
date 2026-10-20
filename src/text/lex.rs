//! Splits text into lines of words with their positions, nested by indentation.

use crate::{Error, ErrorCode, Position};

/// A word of a line: text without spaces, except inside parentheses.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Token<'a> {
    pub text: &'a str,
    pub pos: Position,
}

impl<'a> Token<'a> {
    /// The position of the character `chars` characters into the token.
    pub fn at(&self, chars: usize) -> Position {
        Position::new(self.pos.line, self.pos.column + chars)
    }
    /// The position just after the token's last character.
    pub fn end(&self) -> Position {
        self.at(self.text.chars().count())
    }
    pub fn error(&self, code: ErrorCode, message: impl Into<String>) -> Error {
        Error::at(self.pos, code, message)
    }
}

/// A line with at least one word; blank lines and comments are dropped.
#[derive(Debug)]
pub(crate) struct Line<'a> {
    pub indent: usize,
    pub tokens: Vec<Token<'a>>,
}

impl<'a> Line<'a> {
    pub fn first(&self) -> Token<'a> {
        self.tokens[0]
    }
}

/// A line and the more indented lines under it.
#[derive(Debug)]
pub(crate) struct Node<'a> {
    pub line: Line<'a>,
    pub children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    pub fn keyword(&self) -> &'a str {
        self.line.tokens[0].text
    }
}

/// Splits `text` into a tree of lines. Errors are appended to `errors`; the lines
/// they concern are left out.
pub(crate) fn lex<'a>(text: &'a str, errors: &mut Vec<Error>) -> Vec<Node<'a>> {
    let mut roots: Vec<Node<'a>> = Vec::new();
    // The open nodes, outermost first.
    let mut stack: Vec<Node<'a>> = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let Some(line) = lex_line(raw, number, errors) else {
            continue;
        };
        while stack
            .last()
            .is_some_and(|open| open.line.indent >= line.indent)
        {
            close(&mut stack, &mut roots);
        }
        let siblings = match stack.last() {
            Some(parent) => &parent.children,
            None => &roots,
        };
        let expected = siblings.first().map(|s| s.line.indent);
        let misplaced = match (stack.is_empty(), expected) {
            (true, _) => line.indent != 0,
            (false, Some(indent)) => indent != line.indent,
            (false, None) => false,
        };
        if misplaced {
            let message = if stack.is_empty() {
                "Unexpected indentation: definitions start at the first column".to_owned()
            } else {
                format!(
                    "Indentation of {} spaces does not match the lines above it, indented by {}",
                    line.indent,
                    expected.unwrap_or(0)
                )
            };
            errors.push(Error::at(
                Position::new(number, line.indent + 1),
                ErrorCode::Syntax,
                message,
            ));
            continue;
        }
        stack.push(Node {
            line,
            children: Vec::new(),
        });
    }
    while !stack.is_empty() {
        close(&mut stack, &mut roots);
    }
    roots
}

fn close<'a>(stack: &mut Vec<Node<'a>>, roots: &mut Vec<Node<'a>>) {
    if let Some(node) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => roots.push(node),
        }
    }
}

fn lex_line<'a>(raw: &'a str, number: usize, errors: &mut Vec<Error>) -> Option<Line<'a>> {
    let content = match raw.find(';') {
        Some(i) => &raw[..i],
        None => raw,
    };
    let mut chars = content.char_indices().peekable();
    let mut column = 1;
    let mut indent = 0;
    while let Some(&(_, c)) = chars.peek() {
        if c == ' ' {
            indent += 1;
        } else if c == '\t' {
            errors.push(Error::at(
                Position::new(number, column),
                ErrorCode::Syntax,
                "Indent with spaces, not tabs",
            ));
            return None;
        } else {
            break;
        }
        chars.next();
        column += 1;
    }
    let mut tokens = Vec::new();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            column += 1;
            continue;
        }
        let pos = Position::new(number, column);
        let mut depth = 0usize;
        let mut end = content.len();
        while let Some(&(i, c)) = chars.peek() {
            if c.is_whitespace() && depth == 0 {
                end = i;
                break;
            }
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
            chars.next();
            column += 1;
        }
        tokens.push(Token {
            text: &content[start..end],
            pos,
        });
    }
    if tokens.is_empty() {
        return None;
    }
    Some(Line { indent, tokens })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(nodes: &[Node]) -> String {
        nodes
            .iter()
            .map(|n| {
                let words: Vec<&str> = n.line.tokens.iter().map(|t| t.text).collect();
                if n.children.is_empty() {
                    words.join(" ")
                } else {
                    format!("{}[{}]", words.join(" "), shape(&n.children))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[test]
    fn nests_by_indentation() {
        let mut errors = Vec::new();
        let nodes = lex(
            "a 1 ; comment\n  b\n    c\n    d\n  e\n\n   ; only a comment\nf\r\n  g(1, 2) h\n",
            &mut errors,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(shape(&nodes), "a 1[b[c, d], e], f[g(1, 2) h]");
        let g = nodes[1].children[0].line.tokens[0];
        assert_eq!((g.pos.line, g.pos.column), (9, 3));
        let h = nodes[1].children[0].line.tokens[1];
        assert_eq!((h.pos.line, h.pos.column), (9, 11));
    }

    #[test]
    fn columns_count_characters() {
        let mut errors = Vec::new();
        let nodes = lex("é ü x", &mut errors);
        let x = nodes[0].line.tokens[2];
        assert_eq!(x.pos.column, 5);
    }

    #[test]
    fn reports_bad_indentation() {
        let mut errors = Vec::new();
        let nodes = lex(" a\nb\n    c\n  d\n\te\n", &mut errors);
        let found: Vec<(usize, usize)> = errors
            .iter()
            .map(|e| (e.position.unwrap().line, e.position.unwrap().column))
            .collect();
        assert_eq!(found, vec![(1, 2), (4, 3), (5, 1)]);
        assert!(errors.iter().all(|e| e.code == ErrorCode::Syntax));
        assert_eq!(shape(&nodes), "b[c]");
    }
}
