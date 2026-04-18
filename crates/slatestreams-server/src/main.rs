use std::{net::SocketAddr, sync::Arc};

use slatestreams::{
    grpc::GrpcStreamService, pb::slatestreams::v1::stream_service_server::StreamServiceServer,
    storage::in_memory::InMemoryStorage, stream_controller::StreamController,
};
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:50051".parse()?;
    let meta_storage = Arc::new(InMemoryStorage::new());
    let record_storage = Arc::new(InMemoryStorage::new());
    let controller = Arc::new(StreamController::new(meta_storage, record_storage));
    let service = GrpcStreamService::new(controller);

    println!("slatestreams gRPC server listening on {addr}");

    Server::builder()
        .add_service(StreamServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
