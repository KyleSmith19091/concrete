use slatestreams_client::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Client::connect("http://localhost:50051").await?;

    // Append some records
    let seq = client.append("test", "events", b"hello world").await?;
    println!("appended, next_seq_num={seq}");

    let seq = client.append("test", "events", b"second message").await?;
    println!("appended, next_seq_num={seq}");

    // List basins and streams
    let basins = client.list_basins().await?;
    println!("basins: {basins:?}");

    let streams = client.list_streams("test").await?;
    println!("streams: {streams:?}");

    // Read back all records (with a timeout since read_follow keeps the stream open)
    println!("reading records from seq 0...");
    let mut stream = client.read("test", "events", 0).await?;
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(2));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            record = stream.next() => {
                match record {
                    Some(Ok(r)) => println!("  seq={} data={:?}", r.seq_num, r.data_as_str().unwrap_or("<binary>")),
                    Some(Err(e)) => { eprintln!("error: {e}"); break; }
                    None => { println!("stream ended"); break; }
                }
            }
            _ = &mut timeout => {
                println!("done (timed out waiting for more records)");
                break;
            }
        }
    }

    Ok(())
}
