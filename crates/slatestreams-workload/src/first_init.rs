use serde_json::json;
use slatestreams::pb::slatestreams::v1::AppendRequest;

#[tokio::main]
async fn main() {
    antithesis_sdk::antithesis_init();

    let mut client = slatestreams_workload::helper::connect().await;

    // Seed the stream with one record so subsequent commands have a baseline.
    let resp = client
        .append(AppendRequest {
            basin_name: "test-basin".into(),
            stream_name: "test-stream".into(),
            data: b"init-record".to_vec(),
        })
        .await;

    match resp {
        Ok(r) => {
            let next = r.into_inner().next_seq_num;
            antithesis_sdk::assert_reachable!(
                "first_init seeded stream successfully",
                &json!({"next_seq_num": next})
            );
            println!("first_init: seeded test-basin/test-stream, next_seq_num={next}");
        }
        Err(e) => {
            // During fault injection the server may be temporarily unavailable.
            eprintln!("first_init: append failed (may be transient): {e}");
        }
    }
}
