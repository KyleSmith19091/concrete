use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use bytes::Bytes;

use crate::storage::storage::{
    BytesRange, Record, Storage, StorageError, StorageIterator, StorageRead, StorageResult,
    WriteResult,
};

pub struct InMemoryStorage {
    tree: Arc<RwLock<BTreeMap<Bytes, Bytes>>>,
    seqnum: AtomicU64,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            tree: Arc::new(RwLock::new(BTreeMap::new())),
            seqnum: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl Storage for InMemoryStorage {
    async fn apply(&self, batch: Vec<Record>) -> StorageResult<WriteResult> {
        let mut tree = self
            .tree
            .write()
            .map_err(|e| StorageError::Internal(format!("lock poisoned: {e}")))?;

        for record in batch {
            tree.insert(record.key, record.value);
        }

        let seqnum = self.seqnum.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(WriteResult { seqnum })
    }
}

#[async_trait]
impl StorageRead for InMemoryStorage {
    async fn get(&self, key: Bytes) -> StorageResult<Option<Record>> {
        let tree = self
            .tree
            .read()
            .map_err(|e| StorageError::Internal(format!("lock poisoned: {e}")))?;

        Ok(tree
            .get(&key)
            .map(|value| Record::new(key.clone(), value.clone())))
    }

    async fn scan_iter(&self, range: BytesRange) -> StorageResult<Box<dyn StorageIterator + Send>> {
        let tree = self
            .tree
            .read()
            .map_err(|e| StorageError::Internal(format!("lock poisoned: {e}")))?;

        let records: Vec<Record> = tree
            .range::<Bytes, _>(range)
            .map(|(k, v)| Record::new(k.clone(), v.clone()))
            .collect();

        Ok(Box::new(InMemoryIterator {
            records,
            position: 0,
        }))
    }
}

struct InMemoryIterator {
    records: Vec<Record>,
    position: usize,
}

#[async_trait]
impl StorageIterator for InMemoryIterator {
    async fn next(&mut self) -> StorageResult<Option<Record>> {
        if self.position >= self.records.len() {
            return Ok(None);
        }
        let record = self.records[self.position].clone();
        self.position += 1;
        Ok(Some(record))
    }
}
