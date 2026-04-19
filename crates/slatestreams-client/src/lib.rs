use slatestreams::pb::slatestreams::v1::{
    stream_service_client::StreamServiceClient, AppendRequest, ListBasinsRequest,
    ListStreamsRequest, ReadRequest, StreamRecord,
};
use tokio_stream::StreamExt;
use tonic::transport::Channel;

pub struct Client {
    inner: StreamServiceClient<Channel>,
}

impl Client {
    /// Connect to a slatestreams server at the given URI (e.g. "http://localhost:50051").
    pub async fn connect(uri: impl Into<String>) -> Result<Self, tonic::transport::Error> {
        let inner = StreamServiceClient::connect(uri.into()).await?;
        Ok(Self { inner })
    }

    /// Append data to a stream, creating the basin/stream if they don't exist.
    /// Returns the next sequence number.
    pub async fn append(
        &mut self,
        basin: impl Into<String>,
        stream: impl Into<String>,
        data: impl Into<Vec<u8>>,
    ) -> Result<u64, tonic::Status> {
        let resp = self
            .inner
            .append(AppendRequest {
                basin_name: basin.into(),
                stream_name: stream.into(),
                data: data.into(),
            })
            .await?;
        Ok(resp.into_inner().next_seq_num)
    }

    /// Read records from a stream starting at `start_seq_num`.
    /// Returns all records as a collected Vec. The server stream is consumed to completion
    /// (or until the server closes it).
    pub async fn read(
        &mut self,
        basin: impl Into<String>,
        stream: impl Into<String>,
        start_seq_num: u64,
    ) -> Result<RecordStream, tonic::Status> {
        let resp = self
            .inner
            .read(ReadRequest {
                basin_name: basin.into(),
                stream_name: stream.into(),
                start_seq_num,
            })
            .await?;
        Ok(RecordStream {
            inner: resp.into_inner(),
        })
    }

    /// List all basins. Returns basin names.
    pub async fn list_basins(&mut self) -> Result<Vec<String>, tonic::Status> {
        let resp = self.inner.list_basins(ListBasinsRequest {}).await?;
        Ok(resp
            .into_inner()
            .basins
            .into_iter()
            .map(|b| b.basin_name)
            .collect())
    }

    /// List all streams in a basin. Returns stream names.
    pub async fn list_streams(
        &mut self,
        basin: impl Into<String>,
    ) -> Result<Vec<String>, tonic::Status> {
        let resp = self
            .inner
            .list_streams(ListStreamsRequest {
                basin_name: basin.into(),
            })
            .await?;
        Ok(resp
            .into_inner()
            .streams
            .into_iter()
            .map(|s| s.stream_name)
            .collect())
    }
}

/// A streaming reader that yields records one at a time.
pub struct RecordStream {
    inner: tonic::Streaming<StreamRecord>,
}

impl RecordStream {
    /// Get the next record, or None if the stream is done.
    pub async fn next(&mut self) -> Option<Result<Record, tonic::Status>> {
        self.inner.next().await.map(|r| r.map(Record::from))
    }

    /// Collect all remaining records into a Vec.
    pub async fn collect(mut self) -> Result<Vec<Record>, tonic::Status> {
        let mut records = Vec::new();
        while let Some(result) = self.next().await {
            records.push(result?);
        }
        Ok(records)
    }
}

/// A record read from a stream.
#[derive(Debug, Clone)]
pub struct Record {
    pub seq_num: u64,
    pub data: Vec<u8>,
}

impl Record {
    /// Interpret the data as a UTF-8 string.
    pub fn data_as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.data)
    }
}

impl From<StreamRecord> for Record {
    fn from(r: StreamRecord) -> Self {
        Self {
            seq_num: r.seq_num,
            data: r.data,
        }
    }
}
