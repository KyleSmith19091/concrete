use slatestreams::pb::slatestreams::v1::stream_service_client::StreamServiceClient;
use tonic::transport::Channel;

const SERVER_ADDR: &str = "http://localhost:50051";
const RETRY_DELAY_MS: u64 = 500;
const MAX_RETRIES: u32 = 20;

/// Connect to the slatestreams gRPC server with retries.
/// Handles transient network faults during Antithesis fault injection.
pub async fn connect() -> StreamServiceClient<Channel> {
    let mut attempts = 0;
    loop {
        match StreamServiceClient::connect(SERVER_ADDR).await {
            Ok(client) => return client,
            Err(e) => {
                attempts += 1;
                if attempts >= MAX_RETRIES {
                    panic!("failed to connect to {SERVER_ADDR} after {MAX_RETRIES} attempts: {e}");
                }
                eprintln!("connection attempt {attempts} failed: {e}, retrying...");
                tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            }
        }
    }
}
