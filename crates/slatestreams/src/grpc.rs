use std::{pin::Pin, sync::Arc};

use bytes::Bytes;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::{
    pb::slatestreams::v1::{
        AppendRequest, AppendResponse, BasinView, ListBasinsRequest, ListBasinsResponse,
        ListStreamsRequest, ListStreamsResponse, ReadRequest, StreamRecord, StreamView,
        stream_service_server::StreamService,
    },
    stream_controller::{ControllerError, StreamController},
};

pub struct GrpcStreamService {
    controller: Arc<StreamController>,
}

impl GrpcStreamService {
    pub fn new(controller: Arc<StreamController>) -> Self {
        Self { controller }
    }
}

#[tonic::async_trait]
impl StreamService for GrpcStreamService {
    async fn append(
        &self,
        request: Request<AppendRequest>,
    ) -> Result<Response<AppendResponse>, Status> {
        let request = request.into_inner();
        let next_seq_num = self
            .controller
            .append(
                request.basin_name,
                request.stream_name,
                Bytes::from(request.data),
            )
            .await
            .map_err(controller_error_to_status)?;

        Ok(Response::new(AppendResponse { next_seq_num }))
    }

    type ReadStream = Pin<Box<dyn Stream<Item = Result<StreamRecord, Status>> + Send + 'static>>;

    async fn read(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<Self::ReadStream>, Status> {
        let request = request.into_inner();
        let records = self
            .controller
            .read_follow(
                request.basin_name,
                request.stream_name,
                request.start_seq_num,
            )
            .await
            .map_err(controller_error_to_status)?
            .map(|record| {
                record
                    .map(|record| StreamRecord {
                        seq_num: record.seq_num,
                        data: record.data.to_vec(),
                    })
                    .map_err(controller_error_to_status)
            });

        Ok(Response::new(Box::pin(records) as Self::ReadStream))
    }

    async fn list_basins(
        &self,
        _request: Request<ListBasinsRequest>,
    ) -> Result<Response<ListBasinsResponse>, Status> {
        let basins = self
            .controller
            .list_basins()
            .await
            .map_err(controller_error_to_status)?
            .into_iter()
            .map(|basin| BasinView {
                basin_name: basin.basin_name,
            })
            .collect();

        Ok(Response::new(ListBasinsResponse { basins }))
    }

    async fn list_streams(
        &self,
        request: Request<ListStreamsRequest>,
    ) -> Result<Response<ListStreamsResponse>, Status> {
        let request = request.into_inner();
        let streams = self
            .controller
            .list_streams(request.basin_name)
            .await
            .map_err(controller_error_to_status)?
            .into_iter()
            .map(|stream| StreamView {
                stream_name: stream.stream_name,
            })
            .collect();

        Ok(Response::new(ListStreamsResponse { streams }))
    }
}

fn controller_error_to_status(error: ControllerError) -> Status {
    match error {
        ControllerError::InvalidArgument(message) => Status::invalid_argument(message),
        ControllerError::NotFound(message) => Status::not_found(message),
        ControllerError::ReaderLagged => Status::resource_exhausted(error.to_string()),
        ControllerError::Stream(crate::error::StreamError::StreamDoesNotExist(stream_name)) => {
            Status::not_found(format!("stream {stream_name}"))
        }
        other => Status::internal(other.to_string()),
    }
}
