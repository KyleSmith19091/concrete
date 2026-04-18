use bytes::{BufMut, Bytes, BytesMut};

use crate::storage::{error::DeserializeError, storage::StorageError};

pub const DEFAULT_BLOCK_SIZE: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqBlock {
    pub start: u64,
    pub block_size: u64,
}

impl SeqBlock {
    pub fn new(start: u64, block_size: u64) -> Self {
        Self { start, block_size }
    }

    pub fn serialise(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(16);
        buf.put_u64(self.start);
        buf.put_u64(self.block_size);
        buf.freeze()
    }

    pub const fn block_capacity() -> usize {
        16
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, DeserializeError> {
        if data.len() < Self::block_capacity() {
            return Err(DeserializeError {
                message: format!(
                    "buffer too short for SeqBlock, needs to be 16 bytes, got {}",
                    data.len()
                ),
            });
        }

        let start = u64::from_be_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let block_size = u64::from_be_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);

        Ok(SeqBlock { start, block_size })
    }

    // the range a block defines is [start, start + block_size)
    pub fn next_base(&self) -> u64 {
        self.start + self.block_size
    }
}

pub struct SequenceAllocator {
    next_sequence_number: u64,
    current_block: SeqBlock,
}

/// Error type for sequence allocation operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceError {
    /// Storage operation failed
    Storage(StorageError),
    /// Deserialization failed
    Deserialize(DeserializeError),
}

impl std::error::Error for SequenceError {}

impl std::fmt::Display for SequenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SequenceError::Deserialize(error) => {
                write!(f, "SequenceError::Deserialize failed: {}", error)
            }
            SequenceError::Storage(error) => {
                write!(f, "SequenceError::Storage failed: {}", error)
            }
        }
    }
}

impl SequenceAllocator {
    pub fn new(block: SeqBlock) -> Self {
        Self {
            next_sequence_number: block.next_base(),
            current_block: block,
        }
    }

    pub fn default() -> Self {
        Self {
            next_sequence_number: 0,
            current_block: SeqBlock::new(0, DEFAULT_BLOCK_SIZE),
        }
    }

    /// start  curr   start + block_size (not including)
    ///   |----^----------|
    pub fn allocate(&mut self, count: u64) -> (u64, Option<SeqBlock>) {
        assert!(
            count <= self.current_block.block_size,
            "allocation count ({count}) exceeds block size ({})",
            self.current_block.block_size
        );

        let remaining = self.current_block.next_base() - self.next_sequence_number;

        // if the number we need to allocate is less than the remaining elements then we just move forward the current pointer
        if count < remaining {
            let start = self.next_sequence_number;
            self.next_sequence_number += count;
            return (start, None);
        }
        // at this point count is greater than what is remaining in the current block

        // fill up the rest of the block
        let start = self.next_sequence_number;
        let next_block_allocation = count - remaining;
        let new_block = SeqBlock::new(
            self.current_block.start + self.current_block.block_size,
            self.current_block.block_size,
        );

        self.next_sequence_number = self.current_block.next_base() + next_block_allocation;
        self.current_block = new_block;

        (start, Some(self.current_block.clone()))
    }

    pub fn allocation(&self) -> SeqBlock {
        self.current_block.clone()
    }

    pub fn next_sequence_number(&self) -> u64 {
        self.next_sequence_number
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_block_serialize_roundtrip() {
        let block = SeqBlock::new(42, 1024);
        let bytes = block.serialise();
        let decoded = SeqBlock::deserialize(&bytes).unwrap();
        assert_eq!(block, decoded);
    }

    #[test]
    fn seq_block_deserialize_too_short() {
        let result = SeqBlock::deserialize(&[0u8; 15]);
        assert!(result.is_err());
    }

    #[test]
    fn seq_block_next_base() {
        let block = SeqBlock::new(100, 50);
        assert_eq!(block.next_base(), 150);
    }

    #[test]
    fn allocate_at_block_boundary_advances_to_next_block() {
        let mut allocator = SequenceAllocator::new(SeqBlock::new(0, 2));

        let (seq_num, block) = allocator.allocate(1);

        assert_eq!(seq_num, 2);
        assert_eq!(block, Some(SeqBlock::new(2, 2)));
        assert_eq!(allocator.next_sequence_number(), 3);
    }
}
