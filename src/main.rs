use slatedb::bytes::Bytes;
use slatedb::{Db, Error, object_store::memory::InMemory};
use std::sync::Arc;
use std::time::{SystemTime};

struct Record {
    seq_num: u64,
    timestamp: SystemTime,
    header: &'static[(Bytes, Bytes)],
    body: Bytes,
}

struct Stream {
    uri: String,
    head: Option<Record>,
    tail: Option<Record>,
}

struct Namespace {
    uri: String,
}

struct StreamManager {
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Setup
    let object_store = Arc::new(InMemory::new());
    let kv_store = Db::open("/tmp/slatedb_full_example", object_store).await?;

    // Append
    //
    // Get/Create streamer client instance for stream
    // In-memory concurrent map <StreamID, StreamerClientSlot>
    // enum StreamerClientSlot {
      // Initializing { init_id, future },  // startup in progress
      // Ready { client: StreamerClient },  // actor running
    // }
    // StreamerClient is initialised async
    // Subsequent calls to same stream share that same slot
    //
    // Semaphore
    //
    // 

    Ok(())
}
