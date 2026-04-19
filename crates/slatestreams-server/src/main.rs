use std::{net::SocketAddr, sync::Arc};

use serde_json::json;
use slatedb::{Db, object_store::local::LocalFileSystem};
use slatestreams::{
    grpc::GrpcStreamService, pb::slatestreams::v1::stream_service_server::StreamServiceServer,
    storage::slatedb::SlateDbStorage, stream_controller::StreamController,
};
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    antithesis_sdk::antithesis_init();

    let data_dir = std::env::var("SLATESTREAMS_DATA_DIR").unwrap_or_else(|_| "/Users/kylesmith/Development/slatestreams/data".to_string());
    let object_store = Arc::new(LocalFileSystem::new_with_prefix(&data_dir)?);

    let meta_db = Db::open("meta", object_store.clone()).await?;
    let record_db = Db::open("records", object_store).await?;

    let addr: SocketAddr = "0.0.0.0:50051".parse()?;
    let meta_storage = Arc::new(SlateDbStorage::new(meta_db));
    let record_storage = Arc::new(SlateDbStorage::new(record_db));
    let controller = Arc::new(StreamController::new(meta_storage, record_storage));
    let service = GrpcStreamService::new(controller);

    antithesis_sdk::assert_reachable!("server startup path executed", &json!({"addr": addr.to_string()}));

    println!("slatestreams gRPC server listening on {addr}");

    antithesis_sdk::lifecycle::setup_complete(&json!({"message": "slatestreams server ready"}));

    Server::builder()
        .add_service(StreamServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
