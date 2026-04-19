use serde_json::json;
use slatestreams::pb::slatestreams::v1::{AppendRequest, ReadRequest};
use tokio_stream::StreamExt;

const BASIN: &str = "test-basin";
const STREAM: &str = "test-stream";
const ITERATIONS: u32 = 50;

#[tokio::main]
async fn main() {
    antithesis_sdk::antithesis_init();

    let mut client = slatestreams_workload::helper::connect().await;

    for i in 0..ITERATIONS {
        let payload = format!("acked-write-check-{i}-{}", std::process::id());
        let payload_bytes = payload.as_bytes().to_vec();

        // --- Append ---
        let append_resp = match client
            .append(AppendRequest {
                basin_name: BASIN.into(),
                stream_name: STREAM.into(),
                data: payload_bytes.clone(),
            })
            .await
        {
            Ok(r) => r.into_inner(),
            Err(e) => {
                // Transient fault — skip this iteration.
                eprintln!("append failed (transient): {e}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };

        let written_seq_num = append_resp.next_seq_num - 1;

        // --- Read back the record we just wrote ---
        let read_resp = match client
            .read(ReadRequest {
                basin_name: BASIN.into(),
                stream_name: STREAM.into(),
                start_seq_num: written_seq_num,
            })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("read stream open failed (transient): {e}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };

        // Take the first record from the streaming response.
        let mut stream = read_resp.into_inner();
        let record = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stream.next(),
        )
        .await
        {
            Ok(Some(Ok(r))) => r,
            Ok(Some(Err(e))) => {
                eprintln!("read record error (transient): {e}");
                continue;
            }
            Ok(None) => {
                // Stream ended without delivering the record.
                antithesis_sdk::assert_always!(
                    false,
                    "acknowledged write readable: stream ended without record",
                    &json!({
                        "seq_num": written_seq_num,
                        "basin": BASIN,
                        "stream": STREAM,
                    })
                );
                continue;
            }
            Err(_) => {
                eprintln!("read timed out for seq_num={written_seq_num}");
                continue;
            }
        };

        // --- Property: acked-writes-readable ---
        // The record at the written seq_num must exist.
        antithesis_sdk::assert_always!(
            record.seq_num == written_seq_num,
            "acknowledged write readable: correct seq_num returned",
            &json!({
                "expected_seq_num": written_seq_num,
                "actual_seq_num": record.seq_num,
                "basin": BASIN,
                "stream": STREAM,
            })
        );

        // --- Property: append-data-integrity (bonus — free to check here) ---
        // The data must match what was appended.
        antithesis_sdk::assert_always!(
            record.data == payload_bytes,
            "acknowledged write readable: data integrity preserved",
            &json!({
                "seq_num": written_seq_num,
                "expected_len": payload_bytes.len(),
                "actual_len": record.data.len(),
                "data_matches": record.data == payload_bytes,
            })
        );

        // Track that we exercised the happy path at least once.
        antithesis_sdk::assert_reachable!(
            "append then read-back completed successfully",
            &json!({"iteration": i, "seq_num": written_seq_num})
        );
    }
}
