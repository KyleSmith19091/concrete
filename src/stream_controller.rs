use std::{collections::HashMap, fmt, sync::Arc};

use bytes::Bytes;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    basin::{
        basin::{Basin, BasinView},
        error::BasinError,
    },
    error::StreamError,
    record::Record,
    storage::storage::{Storage, StorageError},
    stream::{AppendResult, Stream, StreamView},
};

const STREAM_COMMAND_BUFFER: usize = 1024;
const STREAM_BROADCAST_BUFFER: usize = 1024;

#[derive(Debug)]
pub enum ControllerError {
    InvalidArgument(String),
    NotFound(String),
    Basin(BasinError),
    Stream(StreamError),
    Storage(StorageError),
    Internal(String),
    ReaderLagged,
}

impl fmt::Display for ControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControllerError::InvalidArgument(message) => write!(f, "invalid argument: {message}"),
            ControllerError::NotFound(message) => write!(f, "not found: {message}"),
            ControllerError::Basin(error) => write!(f, "{error}"),
            ControllerError::Stream(error) => write!(f, "{error}"),
            ControllerError::Storage(error) => write!(f, "{error}"),
            ControllerError::Internal(message) => write!(f, "internal error: {message}"),
            ControllerError::ReaderLagged => write!(f, "reader lagged behind live stream"),
        }
    }
}

impl std::error::Error for ControllerError {}

impl From<BasinError> for ControllerError {
    fn from(value: BasinError) -> Self {
        ControllerError::Basin(value)
    }
}

impl From<StreamError> for ControllerError {
    fn from(value: StreamError) -> Self {
        ControllerError::Stream(value)
    }
}

pub struct StreamController {
    meta_storage: Arc<dyn Storage>,
    record_storage: Arc<dyn Storage>,
    basins: RwLock<HashMap<String, Arc<BasinHandle>>>,
    basin_init: Mutex<()>,
}

impl StreamController {
    pub fn new(meta_storage: Arc<dyn Storage>, record_storage: Arc<dyn Storage>) -> Self {
        Self {
            meta_storage,
            record_storage,
            basins: RwLock::new(HashMap::new()),
            basin_init: Mutex::new(()),
        }
    }

    pub async fn append(
        &self,
        basin_name: String,
        stream_name: String,
        data: Bytes,
    ) -> Result<u64, ControllerError> {
        validate_name("basin_name", &basin_name)?;
        validate_name("stream_name", &stream_name)?;

        let basin = self.get_or_create_basin(basin_name).await?;
        let stream = basin
            .get_stream(
                stream_name,
                true,
                self.meta_storage.clone(),
                self.record_storage.clone(),
            )
            .await?;

        Ok(stream.append(data).await?.next_seq_num)
    }

    pub async fn read_follow(
        &self,
        basin_name: String,
        stream_name: String,
        start_seq_num: u64,
    ) -> Result<ReceiverStream<Result<Record, ControllerError>>, ControllerError> {
        validate_name("basin_name", &basin_name)?;
        validate_name("stream_name", &stream_name)?;

        let basin = self.get_existing_basin(basin_name.clone()).await?;
        let stream = basin
            .get_stream(
                stream_name.clone(),
                false,
                self.meta_storage.clone(),
                self.record_storage.clone(),
            )
            .await
            .map_err(|error| map_stream_not_found(error, &basin_name, &stream_name))?;

        let subscription = stream.subscribe().await?;
        let (tx, rx) = mpsc::channel(STREAM_COMMAND_BUFFER);
        let read_stream = stream.clone();

        tokio::spawn(async move {
            for seq_num in start_seq_num..subscription.next_seq_num {
                match read_stream.read(seq_num).await {
                    Ok(Some(record)) => {
                        if tx.send(Ok(record)).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = tx.send(Err(error)).await;
                        return;
                    }
                }
            }

            let mut live_records = subscription.receiver;
            loop {
                match live_records.recv().await {
                    Ok(record) if record.seq_num >= subscription.next_seq_num => {
                        if tx.send(Ok(record)).await.is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let _ = tx.send(Err(ControllerError::ReaderLagged)).await;
                        return;
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });

        Ok(ReceiverStream::new(rx))
    }

    pub async fn list_basins(&self) -> Result<Vec<BasinView>, ControllerError> {
        Basin::list(&self.meta_storage).await.map_err(Into::into)
    }

    pub async fn list_streams(
        &self,
        basin_name: String,
    ) -> Result<Vec<StreamView>, ControllerError> {
        validate_name("basin_name", &basin_name)?;
        let basin = self.get_existing_basin(basin_name.clone()).await?;
        Stream::list(basin.basin_id, &self.meta_storage)
            .await
            .map_err(Into::into)
    }

    async fn get_cached_basin(&self, basin_name: &str) -> Option<Arc<BasinHandle>> {
        self.basins.read().await.get(basin_name).cloned()
    }

    async fn get_or_create_basin(
        &self,
        basin_name: String,
    ) -> Result<Arc<BasinHandle>, ControllerError> {
        if let Some(basin) = self.get_cached_basin(&basin_name).await {
            return Ok(basin);
        }

        let _guard = self.basin_init.lock().await;
        if let Some(basin) = self.get_cached_basin(&basin_name).await {
            return Ok(basin);
        }

        let basin = match Basin::load(self.meta_storage.clone(), basin_name.clone()).await? {
            Some(basin) => basin,
            None => Basin::new(self.meta_storage.clone(), basin_name.clone()).await?,
        };
        let handle = Arc::new(BasinHandle::new(basin));
        self.basins.write().await.insert(basin_name, handle.clone());
        Ok(handle)
    }

    async fn get_existing_basin(
        &self,
        basin_name: String,
    ) -> Result<Arc<BasinHandle>, ControllerError> {
        if let Some(basin) = self.get_cached_basin(&basin_name).await {
            return Ok(basin);
        }

        let _guard = self.basin_init.lock().await;
        if let Some(basin) = self.get_cached_basin(&basin_name).await {
            return Ok(basin);
        }

        let basin = Basin::load(self.meta_storage.clone(), basin_name.clone())
            .await?
            .ok_or_else(|| ControllerError::NotFound(format!("basin {basin_name}")))?;
        let handle = Arc::new(BasinHandle::new(basin));
        self.basins.write().await.insert(basin_name, handle.clone());
        Ok(handle)
    }
}

struct BasinHandle {
    basin_id: u128,
    streams: RwLock<HashMap<String, StreamHandle>>,
    stream_init: Mutex<()>,
}

impl BasinHandle {
    fn new(basin: Basin) -> Self {
        Self {
            basin_id: basin.id(),
            streams: RwLock::new(HashMap::new()),
            stream_init: Mutex::new(()),
        }
    }

    async fn get_stream(
        &self,
        stream_name: String,
        create: bool,
        meta_storage: Arc<dyn Storage>,
        record_storage: Arc<dyn Storage>,
    ) -> Result<StreamHandle, ControllerError> {
        if let Some(stream) = self.streams.read().await.get(&stream_name).cloned() {
            return Ok(stream);
        }

        let _guard = self.stream_init.lock().await;
        if let Some(stream) = self.streams.read().await.get(&stream_name).cloned() {
            return Ok(stream);
        }

        let stream = match Stream::load(
            self.basin_id,
            stream_name.clone(),
            meta_storage.clone(),
            record_storage.clone(),
        )
        .await
        {
            Ok(stream) => stream,
            Err(StreamError::StreamDoesNotExist(_)) if create => {
                Stream::new(
                    self.basin_id,
                    stream_name.clone(),
                    meta_storage,
                    record_storage,
                )
                .await?
            }
            Err(error) => return Err(error.into()),
        };

        let handle = StreamHandle::spawn(stream);
        self.streams
            .write()
            .await
            .insert(stream_name, handle.clone());
        Ok(handle)
    }
}

#[derive(Clone)]
struct StreamHandle {
    tx: mpsc::Sender<StreamCommand>,
}

impl StreamHandle {
    fn spawn(stream: Stream) -> Self {
        let (tx, rx) = mpsc::channel(STREAM_COMMAND_BUFFER);
        let actor = StreamActor {
            stream,
            rx,
            broadcaster: broadcast::channel(STREAM_BROADCAST_BUFFER).0,
        };
        tokio::spawn(actor.run());
        Self { tx }
    }

    async fn append(&self, data: Bytes) -> Result<AppendResult, ControllerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(StreamCommand::Append { data, reply_tx })
            .await
            .map_err(|_| ControllerError::Internal("stream actor stopped".to_string()))?;
        reply_rx
            .await
            .map_err(|_| {
                ControllerError::Internal("stream actor dropped append reply".to_string())
            })?
            .map_err(Into::into)
    }

    async fn read(&self, seq_num: u64) -> Result<Option<Record>, ControllerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(StreamCommand::Read { seq_num, reply_tx })
            .await
            .map_err(|_| ControllerError::Internal("stream actor stopped".to_string()))?;
        reply_rx
            .await
            .map_err(|_| ControllerError::Internal("stream actor dropped read reply".to_string()))?
            .map_err(Into::into)
    }

    async fn subscribe(&self) -> Result<Subscription, ControllerError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(StreamCommand::Subscribe { reply_tx })
            .await
            .map_err(|_| ControllerError::Internal("stream actor stopped".to_string()))?;
        reply_rx.await.map_err(|_| {
            ControllerError::Internal("stream actor dropped subscribe reply".to_string())
        })
    }
}

struct StreamActor {
    stream: Stream,
    rx: mpsc::Receiver<StreamCommand>,
    broadcaster: broadcast::Sender<Record>,
}

impl StreamActor {
    async fn run(mut self) {
        while let Some(command) = self.rx.recv().await {
            match command {
                StreamCommand::Append { data, reply_tx } => {
                    let result = self.stream.append(data).await;
                    if let Ok(result) = &result {
                        let _ = self.broadcaster.send(result.record.clone());
                    }
                    let _ = reply_tx.send(result);
                }
                StreamCommand::Read { seq_num, reply_tx } => {
                    let _ = reply_tx.send(self.stream.read(seq_num).await);
                }
                StreamCommand::Subscribe { reply_tx } => {
                    let _ = reply_tx.send(Subscription {
                        receiver: self.broadcaster.subscribe(),
                        next_seq_num: self.stream.next_seq_num(),
                    });
                }
            }
        }
    }
}

enum StreamCommand {
    Append {
        data: Bytes,
        reply_tx: oneshot::Sender<Result<AppendResult, StreamError>>,
    },
    Read {
        seq_num: u64,
        reply_tx: oneshot::Sender<Result<Option<Record>, StreamError>>,
    },
    Subscribe {
        reply_tx: oneshot::Sender<Subscription>,
    },
}

struct Subscription {
    receiver: broadcast::Receiver<Record>,
    next_seq_num: u64,
}

fn validate_name(field: &str, value: &str) -> Result<(), ControllerError> {
    if value.trim().is_empty() {
        return Err(ControllerError::InvalidArgument(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn map_stream_not_found(
    error: ControllerError,
    basin_name: &str,
    stream_name: &str,
) -> ControllerError {
    match error {
        ControllerError::Stream(StreamError::StreamDoesNotExist(_)) => {
            ControllerError::NotFound(format!("stream {basin_name}/{stream_name}"))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::in_memory::InMemoryStorage;

    #[tokio::test]
    async fn concurrent_appends_are_contiguous_for_one_stream() {
        let controller = Arc::new(StreamController::new(
            Arc::new(InMemoryStorage::new()),
            Arc::new(InMemoryStorage::new()),
        ));

        let mut tasks = Vec::new();
        for idx in 0..32 {
            let controller = controller.clone();
            tasks.push(tokio::spawn(async move {
                controller
                    .append(
                        "basin".to_string(),
                        "stream".to_string(),
                        Bytes::from(format!("record-{idx}")),
                    )
                    .await
                    .unwrap()
            }));
        }

        let mut tails = Vec::new();
        for task in tasks {
            tails.push(task.await.unwrap());
        }
        tails.sort_unstable();

        assert_eq!(tails, (1..=32).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn append_auto_creates_basin_and_stream_for_read_only_views() {
        let controller = StreamController::new(
            Arc::new(InMemoryStorage::new()),
            Arc::new(InMemoryStorage::new()),
        );

        controller
            .append(
                "basin".to_string(),
                "stream".to_string(),
                Bytes::from_static(b"hello"),
            )
            .await
            .unwrap();

        assert_eq!(
            controller.list_basins().await.unwrap(),
            vec![BasinView {
                basin_name: "basin".to_string()
            }]
        );
        assert_eq!(
            controller.list_streams("basin".to_string()).await.unwrap(),
            vec![StreamView {
                stream_name: "stream".to_string()
            }]
        );
    }

    #[tokio::test]
    async fn read_stream_backfills_then_follows_tail() {
        let controller = Arc::new(StreamController::new(
            Arc::new(InMemoryStorage::new()),
            Arc::new(InMemoryStorage::new()),
        ));

        controller
            .append(
                "basin".to_string(),
                "stream".to_string(),
                Bytes::from_static(b"first"),
            )
            .await
            .unwrap();

        let mut records = controller
            .read_follow("basin".to_string(), "stream".to_string(), 0)
            .await
            .unwrap();

        use tokio_stream::StreamExt;
        let first = records.next().await.unwrap().unwrap();
        assert_eq!(first.seq_num, 0);
        assert_eq!(first.data, Bytes::from_static(b"first"));

        controller
            .append(
                "basin".to_string(),
                "stream".to_string(),
                Bytes::from_static(b"second"),
            )
            .await
            .unwrap();

        let second = records.next().await.unwrap().unwrap();
        assert_eq!(second.seq_num, 1);
        assert_eq!(second.data, Bytes::from_static(b"second"));
    }
}
