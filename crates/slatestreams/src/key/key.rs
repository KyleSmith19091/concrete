use bytes::{BufMut, Bytes, BytesMut};

use crate::storage::storage::BytesRange;

/// Storage record type tags
pub const RECORD_SEQUENCE_NUMBER_PREFIX_KEY: u8 = 0x01;
pub const RECORD_BASIN_PREFIX_KEY: u8 = 0x02;
pub const RECORD_STREAM_RECORD_PREFIX_KEY: u8 = 0x03;

// Meta record type tags
pub const META_STREAM_PREFIX_KEY: u8 = 0x01;
pub const META_BASIN_PREFIX_KEY: u8 = 0x02;

/// Sequence Allocator Record tags
pub const SEQ_BASIN_PREFIX_KEY: u8 = 0x01;
pub const SEQ_STREAM_PREFIX_KEY: u8 = 0x02;

/// build_bytes is a utility function to make it easier to construct Bytes buffer for a string
pub fn build_bytes(prefix: u8, s: &str) -> Bytes {
    let mut buf = BytesMut::with_capacity(1 + s.len());

    buf.put_u8(prefix);
    buf.extend_from_slice(s.as_bytes());

    buf.freeze()
}

pub fn build_bytes_from_string(s: &str) -> Bytes {
    let mut buf = BytesMut::with_capacity(s.len());
    buf.extend_from_slice(s.as_bytes());
    buf.freeze()
}

pub fn prefix_range(prefix: Bytes) -> BytesRange {
    let start = std::ops::Bound::Included(prefix.clone());
    let end = match next_prefix(&prefix) {
        Some(end) => std::ops::Bound::Excluded(end),
        None => std::ops::Bound::Unbounded,
    };

    BytesRange::new(start, end)
}

fn next_prefix(prefix: &[u8]) -> Option<Bytes> {
    let mut end = prefix.to_vec();
    for idx in (0..end.len()).rev() {
        if end[idx] != u8::MAX {
            end[idx] += 1;
            end.truncate(idx + 1);
            return Some(Bytes::from(end));
        }
    }
    None
}

pub struct BytesBuilder<State> {
    buf: BytesMut,
    _state: std::marker::PhantomData<State>,
}

pub struct Start;
pub struct Done;

impl BytesBuilder<Start> {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: BytesMut::with_capacity(capacity),
            _state: std::marker::PhantomData,
        }
    }

    pub fn add_u8(mut self, v: u8) -> BytesBuilder<Start> {
        self.buf.put_u8(v);
        self
    }

    pub fn add_u64(mut self, v: u64) -> BytesBuilder<Start> {
        self.buf.put_u64(v);
        self
    }

    pub fn add_u128(mut self, v: u128) -> BytesBuilder<Start> {
        self.buf.put_u128(v);
        self
    }

    pub fn put_str(mut self, s: &str) -> BytesBuilder<Start> {
        self.buf.extend_from_slice(s.as_bytes());
        self
    }

    pub fn build(self) -> Bytes {
        self.buf.freeze()
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

    use super::*;

    #[test]
    fn prefix_range_ends_at_next_lexicographic_prefix() {
        let range = prefix_range(Bytes::from_static(&[0x01, 0x02]));

        assert!(range.contains(&[0x01, 0x02]));
        assert!(range.contains(&[0x01, 0x02, 0x00]));
        assert!(!range.contains(&[0x01, 0x03]));
    }

    #[test]
    fn prefix_range_without_successor_is_unbounded_at_end() {
        let range = prefix_range(Bytes::from_static(&[0xff]));

        assert!(matches!(range.start, Bound::Included(_)));
        assert!(matches!(range.end, Bound::Unbounded));
    }
}
