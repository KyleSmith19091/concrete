use async_trait::async_trait;
use bytes::Bytes;
use slatedb::{Db, DbIterator, WriteBatch};

use crate::storage::storage::{
    BytesRange, Record, Storage, StorageError, StorageIterator, StorageRead, StorageResult,
    WriteResult,
};

pub struct SlateDbStorage {
    db: Db,
}

impl SlateDbStorage {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Storage for SlateDbStorage {
    async fn apply(&self, batch: Vec<Record>) -> StorageResult<WriteResult> {
        let mut wb = WriteBatch::new();
        for record in batch {
            wb.put(&record.key, &record.value);
        }
        let handle = self
            .db
            .write(wb)
            .await
            .map_err(StorageError::from_storage)?;
        Ok(WriteResult {
            seqnum: handle.seqnum(),
        })
    }
}

#[async_trait]
impl StorageRead for SlateDbStorage {
    async fn get(&self, key: Bytes) -> StorageResult<Option<Record>> {
        let value = self
            .db
            .get(&key)
            .await
            .map_err(StorageError::from_storage)?;
        Ok(value.map(|v| Record::new(key, v)))
    }

    async fn scan_iter(&self, range: BytesRange) -> StorageResult<Box<dyn StorageIterator + Send>> {
        let iter = self
            .db
            .scan::<Bytes, BytesRange>(range)
            .await
            .map_err(StorageError::from_storage)?;
        Ok(Box::new(SlateDbIterator { iter }))
    }
}

struct SlateDbIterator {
    iter: DbIterator,
}

#[async_trait]
impl StorageIterator for SlateDbIterator {
    async fn next(&mut self) -> StorageResult<Option<Record>> {
        let kv = self.iter.next().await.map_err(StorageError::from_storage)?;
        Ok(kv.map(|kv| Record::new(kv.key, kv.value)))
    }
}
