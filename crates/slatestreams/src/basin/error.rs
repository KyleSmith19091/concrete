use crate::{error::StreamError, storage::storage::StorageError};

#[derive(Debug)]
pub enum BasinError {
    StreamError(u128, StreamError),
    DeserialiseError(String),
    StorageError(String, StorageError),
}

impl std::error::Error for BasinError {}

impl std::fmt::Display for BasinError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BasinError::StreamError(id, error) => {
                write!(f, "Stream in basin {} error: {}", id, error)
            }
            BasinError::DeserialiseError(message) => {
                write!(f, "{}", message)
            }
            BasinError::StorageError(message, error) => {
                write!(f, "{}: {}", message, error)
            }
        }
    }
}
