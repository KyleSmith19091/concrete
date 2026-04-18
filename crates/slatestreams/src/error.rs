use crate::storage::{sequence::SequenceError, storage::StorageError};

#[derive(Debug)]
pub enum StreamError {
    StreamDoesNotExist(String),
    DeserialiseError(String),
    SequenceError(SequenceError),
    StorageError(StorageError),
    ReplyNotSent(String),
    Fenced(String),
}

impl std::error::Error for StreamError {}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            StreamError::StreamDoesNotExist(stream_name) => {
                write!(f, "Stream with name {} does not exist", stream_name)
            }
            StreamError::DeserialiseError(message) => {
                write!(f, "{}", message)
            }
            StreamError::SequenceError(error) => {
                write!(f, "{}", error)
            }
            StreamError::StorageError(error) => {
                write!(f, "{}", error.to_string())
            }
            StreamError::ReplyNotSent(message) => {
                write!(f, "{}", message)
            }
            StreamError::Fenced(message) => {
                write!(f, "{}", message)
            }
        }
    }
}
