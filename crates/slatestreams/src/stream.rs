use std::sync::Arc;

use bytes::{BufMut, Bytes, BytesMut};
use uuid::Uuid;

use crate::{
    error::StreamError,
    key::key::{
        BytesBuilder, META_STREAM_PREFIX_KEY, RECORD_SEQUENCE_NUMBER_PREFIX_KEY,
        RECORD_STREAM_RECORD_PREFIX_KEY, SEQ_STREAM_PREFIX_KEY, prefix_range,
    },
    record::Record,
    storage::{
        sequence::{SeqBlock, SequenceAllocator},
        storage::{Record as StorageRecord, Storage},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamView {
    pub stream_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppendResult {
    pub record: Record,
    pub next_seq_num: u64,
}

pub struct Stream {
    sequence_number_allocator: SequenceAllocator,
    metadata: StreamMetadata,
    record_storage: Arc<dyn Storage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StreamMetadata {
    basin_id: u128,
    stream_id: u128,
}

impl StreamMetadata {
    pub fn serialise(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(Self::size());
        buf.put_u128(self.basin_id);
        buf.put_u128(self.stream_id);
        buf.freeze()
    }

    const fn size() -> usize {
        16 + 16
    }

    pub(crate) fn deserialise(data: &[u8]) -> Result<Self, StreamError> {
        if data.len() < Self::size() {
            return Err(StreamError::DeserialiseError(
                "data buffer smaller than StreamMetadata size".to_string(),
            ));
        }

        let encoded_basin_id = u128::from_be_bytes(data[0..16].try_into().unwrap());
        let encoded_stream_id = u128::from_be_bytes(data[16..32].try_into().unwrap());

        Ok(Self {
            basin_id: encoded_basin_id,
            stream_id: encoded_stream_id,
        })
    }
}

impl PartialEq for Stream {
    fn eq(&self, other: &Self) -> bool {
        self.metadata.basin_id == other.metadata.basin_id
    }
}

impl Eq for Stream {}

impl std::hash::Hash for Stream {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.metadata.basin_id.hash(state);
    }
}

impl Stream {
    pub async fn new(
        basin_id: u128,
        stream_name: String,
        meta_storage: Arc<dyn Storage>,
        record_storage: Arc<dyn Storage>,
    ) -> Result<Self, StreamError> {
        let sequence_number_allocator = SequenceAllocator::default();
        let stream_id = Uuid::new_v4().as_u128();
        let metadata = StreamMetadata {
            basin_id,
            stream_id,
        };

        meta_storage
            .apply(vec![StorageRecord {
                key: Self::construct_metadata_key(basin_id, &stream_name),
                value: metadata.serialise(),
            }])
            .await
            .map_err(StreamError::StorageError)?;

        record_storage
            .apply(vec![StorageRecord {
                key: Self::construct_sequence_block_key(basin_id, stream_id),
                value: sequence_number_allocator.allocation().serialise(),
            }])
            .await
            .map_err(StreamError::StorageError)?;

        Ok(Self {
            sequence_number_allocator,
            metadata,
            record_storage,
        })
    }

    pub async fn load(
        basin_id: u128,
        stream_name: String,
        meta_storage: Arc<dyn Storage>,
        record_storage: Arc<dyn Storage>,
    ) -> Result<Self, StreamError> {
        let metadata = Self::load_metadata(basin_id, stream_name, &meta_storage).await?;

        let seq_block = record_storage
            .get(Self::construct_sequence_block_key(
                basin_id,
                metadata.stream_id,
            ))
            .await
            .map_err(|e| StreamError::StorageError(e))?;

        let sequence_number_allocator = match seq_block {
            Some(record) => {
                let seq_block = SeqBlock::deserialize(&record.value)
                    .map_err(|e| StreamError::DeserialiseError(e.message))?;
                SequenceAllocator::new(seq_block)
            }
            None => SequenceAllocator::default(),
        };

        Ok(Self {
            metadata,
            record_storage,
            sequence_number_allocator,
        })
    }

    async fn load_metadata(
        basin_id: u128,
        stream_name: String,
        meta_storage: &Arc<dyn Storage>,
    ) -> Result<StreamMetadata, StreamError> {
        match meta_storage
            .get(Self::construct_metadata_key(basin_id, &stream_name))
            .await
            .map_err(StreamError::StorageError)?
        {
            Some(record) => {
                let metadata = StreamMetadata::deserialise(&record.value)?;
                Ok(metadata)
            }
            None => Err(StreamError::StreamDoesNotExist(stream_name)),
        }
    }

    pub async fn append(&mut self, data: Bytes) -> Result<AppendResult, StreamError> {
        // prepare batch container
        let mut records: Vec<StorageRecord> = Vec::new();

        // allocate sequence number
        let (seq_num, block) = self.sequence_number_allocator.allocate(1);

        // check if we need to persist new sequence number block
        if let Some(block) = block {
            records.push(StorageRecord {
                key: Self::construct_sequence_block_key(
                    self.metadata.basin_id,
                    self.metadata.stream_id,
                ),
                value: block.serialise(),
            });
        }

        // prepare stream record
        let write_record = Record { seq_num, data };

        // prepare storage record
        records.push(StorageRecord {
            // Key: [STREAM_RECORD_TAG, BASIN_ID, STREAM_ID, SEQUENCE_NUMBER..]
            key: Self::construct_stream_record_key(
                self.metadata.basin_id,
                self.metadata.stream_id,
                seq_num,
            ),
            value: write_record.serialise(),
        });

        // apply record(s) (for now we require that it be durable before returning)
        let _ = self
            .record_storage
            .apply(records)
            .await
            .map_err(StreamError::StorageError)?;

        Ok(AppendResult {
            record: write_record,
            next_seq_num: self.sequence_number_allocator.next_sequence_number(),
        })
    }

    pub async fn read(&self, seq_num: u64) -> Result<Option<Record>, StreamError> {
        let result = self
            .record_storage
            .get(Self::construct_stream_record_key(
                self.metadata.basin_id,
                self.metadata.stream_id,
                seq_num,
            ))
            .await
            .map_err(StreamError::StorageError)?;

        match result {
            Some(record) => {
                let stream_record = Record::deserialise(&record.value)
                    .map_err(|e| StreamError::DeserialiseError(e.to_string()))?;

                Ok(Some(stream_record))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn next_seq_num(&self) -> u64 {
        self.sequence_number_allocator.next_sequence_number()
    }

    pub(crate) async fn list(
        basin_id: u128,
        meta_storage: &Arc<dyn Storage>,
    ) -> Result<Vec<StreamView>, StreamError> {
        let prefix = Self::metadata_prefix(basin_id);
        let mut iter = meta_storage
            .scan_iter(prefix_range(prefix.clone()))
            .await
            .map_err(StreamError::StorageError)?;

        let mut streams = Vec::new();
        while let Some(record) = iter.next().await.map_err(StreamError::StorageError)? {
            StreamMetadata::deserialise(&record.value)?;
            let stream_name = std::str::from_utf8(&record.key[prefix.len()..])
                .map_err(|e| StreamError::DeserialiseError(e.to_string()))?
                .to_string();
            streams.push(StreamView { stream_name });
        }
        streams.sort_by(|a, b| a.stream_name.cmp(&b.stream_name));
        Ok(streams)
    }

    fn construct_sequence_block_key(basin_id: u128, stream_id: u128) -> Bytes {
        BytesBuilder::new(1 + 1 + 16 + 16)
            .add_u8(RECORD_SEQUENCE_NUMBER_PREFIX_KEY)
            .add_u8(SEQ_STREAM_PREFIX_KEY)
            .add_u128(basin_id)
            .add_u128(stream_id)
            .build()
    }

    fn construct_stream_record_key(basin_id: u128, stream_id: u128, sequence_number: u64) -> Bytes {
        BytesBuilder::new(1 + 16 + 16 + 8)
            .add_u8(RECORD_STREAM_RECORD_PREFIX_KEY)
            .add_u128(basin_id)
            .add_u128(stream_id)
            .add_u64(sequence_number)
            .build()
    }

    pub(crate) fn construct_metadata_key(basin_id: u128, stream_name: &str) -> Bytes {
        BytesBuilder::new(1 + 16 + stream_name.len())
            .add_u8(META_STREAM_PREFIX_KEY)
            .add_u128(basin_id)
            .put_str(&stream_name)
            .build()
    }

    fn metadata_prefix(basin_id: u128) -> Bytes {
        BytesBuilder::new(1 + 16)
            .add_u8(META_STREAM_PREFIX_KEY)
            .add_u128(basin_id)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::in_memory::InMemoryStorage;

    #[tokio::test]
    async fn append_returns_tail_after_write() {
        let meta_storage: Arc<dyn Storage> = Arc::new(InMemoryStorage::new());
        let record_storage: Arc<dyn Storage> = Arc::new(InMemoryStorage::new());
        let mut stream = Stream::new(7, "events".to_string(), meta_storage, record_storage)
            .await
            .unwrap();

        let first = stream.append(Bytes::from_static(b"one")).await.unwrap();
        let second = stream.append(Bytes::from_static(b"two")).await.unwrap();

        assert_eq!(first.record.seq_num, 0);
        assert_eq!(first.next_seq_num, 1);
        assert_eq!(second.record.seq_num, 1);
        assert_eq!(second.next_seq_num, 2);
    }

    #[tokio::test]
    async fn list_streams_from_metadata_storage() {
        let meta_storage: Arc<dyn Storage> = Arc::new(InMemoryStorage::new());
        let record_storage: Arc<dyn Storage> = Arc::new(InMemoryStorage::new());

        Stream::new(
            7,
            "b".to_string(),
            meta_storage.clone(),
            record_storage.clone(),
        )
        .await
        .unwrap();
        Stream::new(7, "a".to_string(), meta_storage.clone(), record_storage)
            .await
            .unwrap();

        let streams = Stream::list(7, &meta_storage).await.unwrap();

        assert_eq!(
            streams,
            vec![
                StreamView {
                    stream_name: "a".to_string()
                },
                StreamView {
                    stream_name: "b".to_string()
                }
            ]
        );
    }
}
