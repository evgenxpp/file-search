use std::{
    fmt::{self, Display, Formatter},
    io,
};

use redb::{DatabaseError, TransactionError};
use tantivy::{TantivyError, directory::error::OpenDirectoryError, query::QueryParserError};

#[derive(Debug)]
pub enum ErrorSource {
    Io,
    Redb,
    Tantivy,
}

#[derive(Debug)]
pub struct Error {
    source: ErrorSource,
    message: String,
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error {
            source: ErrorSource::Io,
            message: value.to_string(),
        }
    }
}

impl From<redb::Error> for Error {
    fn from(value: redb::Error) -> Self {
        Error {
            source: ErrorSource::Redb,
            message: value.to_string(),
        }
    }
}

impl From<redb::StorageError> for Error {
    fn from(value: redb::StorageError) -> Self {
        Error {
            source: ErrorSource::Redb,
            message: value.to_string(),
        }
    }
}

impl From<redb::TableError> for Error {
    fn from(value: redb::TableError) -> Self {
        Error {
            source: ErrorSource::Redb,
            message: value.to_string(),
        }
    }
}

impl From<redb::CommitError> for Error {
    fn from(value: redb::CommitError) -> Self {
        Error {
            source: ErrorSource::Redb,
            message: value.to_string(),
        }
    }
}

impl From<TantivyError> for Error {
    fn from(value: TantivyError) -> Self {
        Error {
            source: ErrorSource::Tantivy,
            message: value.to_string(),
        }
    }
}

impl From<QueryParserError> for Error {
    fn from(value: QueryParserError) -> Self {
        Error {
            source: ErrorSource::Tantivy,
            message: value.to_string(),
        }
    }
}

impl From<DatabaseError> for Error {
    fn from(value: DatabaseError) -> Self {
        Error {
            source: ErrorSource::Tantivy,
            message: value.to_string(),
        }
    }
}

impl From<OpenDirectoryError> for Error {
    fn from(value: OpenDirectoryError) -> Self {
        Error {
            source: ErrorSource::Tantivy,
            message: value.to_string(),
        }
    }
}

impl From<TransactionError> for Error {
    fn from(value: TransactionError) -> Self {
        Error {
            source: ErrorSource::Tantivy,
            message: value.to_string(),
        }
    }
}

impl Display for ErrorSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSource::Io => write!(f, "io"),
            ErrorSource::Redb => write!(f, "redb"),
            ErrorSource::Tantivy => write!(f, "tantivy"),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Source: {}, Message: {}", self.source, self.message)
    }
}

impl std::error::Error for Error {}
