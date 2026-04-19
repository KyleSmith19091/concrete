use slatestreams::pb::slatestreams::v1::AppendRequest;

const BASIN: &str = "test-basin";
const STREAM: &str = "test-stream";

#[tokio::main]
async fn main() {
    let mut client = slatestreams_workload::helper::connect().await;

    let mut i: u64 = 0;
    loop {
        let payload = format!("message-{i}");
        match client
            .append(AppendRequest {
                basin_name: BASIN.into(),
                stream_name: STREAM.into(),
                data: payload.as_bytes().to_vec(),
            })
            .await
        {
            Ok(r) => {
                let seq = r.into_inner().next_seq_num - 1;
                println!("wrote seq={seq}: {payload}");
            }
            Err(e) => {
                eprintln!("append failed: {e}");
            }
        }

        i += 1;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
