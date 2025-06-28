use std::fmt::{self, Display, Formatter};

use tantivy::TantivyError;

#[derive(Debug)]
pub enum Error {
    TantivyError(TantivyError),
}

impl From<TantivyError> for Error {
    fn from(value: TantivyError) -> Self {
        Error::TantivyError(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::TantivyError(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for Error {}
