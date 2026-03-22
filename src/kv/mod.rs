use bytes::{Buf, Bytes};
use enum_ordinalize::Ordinalize;
use thiserror::Error;

pub mod stream_id;
pub mod stream_tail_position;
pub mod bash;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ordinalize)]
pub enum KeyType {
    StreamTailPosition = 1,
}

#[derive(Debug, Clone, Error)]
pub enum DeserializationError {
    #[error("invalid ordinal: {0}")]
    InvalidOrdinal(u8),
    #[error("invalid size: expected {expected} bytes, got {actual}")]
    InvalidSize { expected: usize, actual: usize },
    #[error("invalid value '{name}': {error}")]
    InvalidValue { name: &'static str, error: String },
    #[error("missing field separator")]
    MissingFieldSeparator,
    #[error("json serialization error: {0}")]
    JsonSerialization(String),
    #[error("json deserialization error: {0}")]
    JsonDeserialization(String),
}

fn check_exact_size(bytes: &Bytes, expected: usize) -> Result<(), DeserializationError> {
    if bytes.remaining() != expected {
        return Err(DeserializationError::InvalidSize {
            expected,
            actual: bytes.remaining(),
        });
    }
    Ok(())
}