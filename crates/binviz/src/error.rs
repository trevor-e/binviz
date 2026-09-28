use std::fmt;

/// Error type for everything in binviz.
#[derive(Debug, Clone)]
pub struct Error(String);

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<object::read::Error> for Error {
    fn from(e: object::read::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<gimli::Error> for Error {
    fn from(e: gimli::Error) -> Self {
        Error(format!("DWARF: {e}"))
    }
}

macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::error::Error::new(format!($($arg)*)))
    };
}
pub(crate) use bail;
