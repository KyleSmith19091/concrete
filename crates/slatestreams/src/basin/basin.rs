use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

use crate::{
    basin::error::BasinError,
    key::key::{BytesBuilder, META_BASIN_PREFIX_KEY, prefix_range},
    storage::storage::{Record as StorageRecord, Storage},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasinView {
    pub basin_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Basin {
    basin_id: u128,
    basin_name: String,
}

impl Basin {
    pub async fn new(
        meta_storage: Arc<dyn Storage>,
        basin_name: String,
    ) -> Result<Self, BasinError> {
        let basin_id = Uuid::new_v4().as_u128();
        meta_storage
            .apply(vec![StorageRecord {
                key: Self::construct_metadata_key(&basin_name),
                value: BytesBuilder::new(16).add_u128(basin_id).build(),
            }])
            .await
            .map_err(|e| {
                BasinError::StorageError("error applying basin metadata".to_string(), e)
            })?;

        Ok(Self {
            basin_id,
            basin_name,
        })
    }

    pub async fn load(
        meta_storage: Arc<dyn Storage>,
        basin_name: String,
    ) -> Result<Option<Self>, BasinError> {
        let record = meta_storage
            .get(Self::construct_metadata_key(&basin_name))
            .await
            .map_err(|e| BasinError::StorageError("error loading basin metadata".to_string(), e))?;

        let Some(record) = record else {
            return Ok(None);
        };

        Ok(Some(Self {
            basin_id: Self::deserialise_basin_id(&record.value)?,
            basin_name,
        }))
    }

    pub async fn list(meta_storage: &Arc<dyn Storage>) -> Result<Vec<BasinView>, BasinError> {
        let prefix = Self::metadata_prefix();
        let mut iter = meta_storage
            .scan_iter(prefix_range(prefix.clone()))
            .await
            .map_err(|e| BasinError::StorageError("error listing basins".to_string(), e))?;

        let mut basins = Vec::new();
        while let Some(record) = iter
            .next()
            .await
            .map_err(|e| BasinError::StorageError("error listing basins".to_string(), e))?
        {
            Self::deserialise_basin_id(&record.value)?;
            let basin_name = std::str::from_utf8(&record.key[prefix.len()..])
                .map_err(|e| BasinError::DeserialiseError(e.to_string()))?
                .to_string();
            basins.push(BasinView { basin_name });
        }
        basins.sort_by(|a, b| a.basin_name.cmp(&b.basin_name));
        Ok(basins)
    }

    pub fn id(&self) -> u128 {
        self.basin_id
    }

    pub fn name(&self) -> &str {
        &self.basin_name
    }

    fn deserialise_basin_id(data: &[u8]) -> Result<u128, BasinError> {
        if data.len() < 16 {
            return Err(BasinError::DeserialiseError(format!(
                "basin metadata should be at least 16 bytes, got {}",
                data.len()
            )));
        }

        Ok(u128::from_be_bytes(data[0..16].try_into().unwrap()))
    }

    pub(crate) fn construct_metadata_key(basin_name: &str) -> Bytes {
        BytesBuilder::new(1 + basin_name.len())
            .add_u8(META_BASIN_PREFIX_KEY)
            .put_str(basin_name)
            .build()
    }

    fn metadata_prefix() -> Bytes {
        BytesBuilder::new(1).add_u8(META_BASIN_PREFIX_KEY).build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::in_memory::InMemoryStorage;

    #[tokio::test]
    async fn list_basins_from_metadata_storage() {
        let meta_storage: Arc<dyn Storage> = Arc::new(InMemoryStorage::new());

        Basin::new(meta_storage.clone(), "b".to_string())
            .await
            .unwrap();
        Basin::new(meta_storage.clone(), "a".to_string())
            .await
            .unwrap();

        let basins = Basin::list(&meta_storage).await.unwrap();

        assert_eq!(
            basins,
            vec![
                BasinView {
                    basin_name: "a".to_string()
                },
                BasinView {
                    basin_name: "b".to_string()
                }
            ]
        );
    }
}
