use slatestreams::pb::slatestreams::v1::ReadRequest;
use tokio_stream::StreamExt;

const BASIN: &str = "test-basin";
const STREAM: &str = "test-stream";

#[tokio::main]
async fn main() {
    let mut client = slatestreams_workload::helper::connect().await;

    let start_seq_num = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    println!("following {BASIN}/{STREAM} from seq {start_seq_num}...");

    let response = client
        .read(ReadRequest {
            basin_name: BASIN.into(),
            stream_name: STREAM.into(),
            start_seq_num,
        })
        .await
        .expect("failed to open read stream");

    let mut stream = response.into_inner();
    while let Some(result) = stream.next().await {
        match result {
            Ok(record) => {
                let data = String::from_utf8_lossy(&record.data);
                println!("seq={}: {data}", record.seq_num);
            }
            Err(e) => {
                eprintln!("stream error: {e}");
                break;
            }
        }
    }

    println!("stream closed");
}
