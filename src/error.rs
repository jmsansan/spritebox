use std::fmt;

/// Stable error categories. Match on the code rather than the message.
///
/// [`ErrorCode::as_str`] gives the name used in the command-line tool's JSON
/// diagnostics, which is stable too.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Malformed text: an unknown directive, a missing value, bad indentation.
    Syntax,
    /// A value of the right shape but out of range, such as a color or a size.
    InvalidArgument,
    /// A reference to a palette, sprite or palette key that is not defined.
    UnknownName,
    /// A second definition of a name already defined.
    DuplicateName,
    /// A grid whose rows or columns do not match its declared size.
    SizeMismatch,
    /// Definitions that refer to each other in a circle.
    Cycle,
    /// Input or output beyond the bounds of [`Limits`](crate::Limits).
    LimitExceeded,
    /// Malformed or truncated binary input, such as a PNG file.
    DecodeError,
    /// Output that cannot be encoded.
    EncodeError,
    /// Binary input of a kind the crate does not read.
    UnsupportedFormat,
    /// A file could not be read or written.
    Io,
}

impl ErrorCode {
    /// The code's stable name in `snake_case`, such as `"unknown_name"`.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Syntax => "syntax",
            ErrorCode::InvalidArgument => "invalid_argument",
            ErrorCode::UnknownName => "unknown_name",
            ErrorCode::DuplicateName => "duplicate_name",
            ErrorCode::SizeMismatch => "size_mismatch",
            ErrorCode::Cycle => "cycle",
            ErrorCode::LimitExceeded => "limit_exceeded",
            ErrorCode::DecodeError => "decode_error",
            ErrorCode::EncodeError => "encode_error",
            ErrorCode::UnsupportedFormat => "unsupported_format",
            ErrorCode::Io => "io",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A place in text: a 1-based line, and a 1-based column counted in characters.
///
/// Positions in an imported file say which file through [`Error::file`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Position {
    /// Which file of a parse: 0 for the text parsed, then imported files in the
    /// order loaded. First, so that errors sort by file, then line, then column.
    pub(crate) file: u32,
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            file: 0,
            line,
            column,
        }
    }
    pub(crate) fn in_file(file: u32, line: usize, column: usize) -> Self {
        Self { file, line, column }
    }
}

/// An error with a stable category, human-readable context and, for errors in text,
/// the position where it was found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    /// Where in the text the error is. Every parse and validation error has one;
    /// errors in binary input and I/O errors do not.
    pub position: Option<Position>,
    /// The imported file that `position` is in, by the name its `import` resolves
    /// to; `None` for the text parsed itself.
    pub file: Option<String>,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            position: None,
            file: None,
        }
    }
    /// An error at `position` in text.
    pub fn at(position: Position, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            position: Some(position),
            file: None,
        }
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }
    pub(crate) fn decode(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::DecodeError, message)
    }
    pub(crate) fn limit(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::LimitExceeded, message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}, ")?;
        }
        match self.position {
            Some(p) => write!(f, "line {}, column {}: {}", p.line, p.column, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
