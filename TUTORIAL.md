# Building a Mini Stream Store: Learning Tokio Through S2 Lite

This tutorial builds a simplified append-only stream store step by step. Each section introduces one concurrency primitive and shows why it's needed.

## What We're Building

A service where clients can:
1. **Append** records to named streams (write path)
2. **Read** records back by position (read path — simplified)

The architecture mirrors S2 Lite:
```
HTTP Request → Backend → StreamerClient → [channel] → Streamer Actor → Storage
```

## Prerequisites

```toml
# Cargo.toml
[package]
name = "mini-stream"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
parking_lot = "0.12"
```

---

## Part 1: The Actor Pattern with `mpsc` and `oneshot`

### The Problem

Multiple HTTP requests hit the same stream concurrently. We need to assign monotonically increasing sequence numbers. With a mutex, every writer blocks every other writer. With an actor, writes are serialized by a channel — no locks.

### The Core Idea

```
           ┌──────────┐
Writer A ──┤          │
           │  mpsc    ├──► Actor (single task, owns all state)
Writer B ──┤ channel  │
           └──────────┘
```

### Build It

```rust
// src/streamer.rs

use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

/// The data stored per record
#[derive(Debug, Clone)]
pub struct Record {
    pub seq_num: u64,
    pub data: Vec<u8>,
}

/// What callers send to the actor
enum Message {
    Append {
        data: Vec<u8>,
        reply_tx: oneshot::Sender<Record>,
    },
    Read {
        seq_num: u64,
        reply_tx: oneshot::Sender<Option<Record>>,
    },
}
```

The `Message` enum is the actor's API. Each variant carries a `oneshot::Sender` — the actor sends its response back through this one-time channel.

```rust
/// The actor — owns all mutable state, runs in a single tokio task
struct Streamer {
    next_seq: u64,
    records: HashMap<u64, Record>,
}

impl Streamer {
    fn new() -> Self {
        Self {
            next_seq: 0,
            records: HashMap::new(),
        }
    }

    /// The actor's event loop
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Message>) {
        // This loop IS the actor. It processes one message at a time.
        // No locks needed — only this task touches `self`.
        while let Some(msg) = rx.recv().await {
            match msg {
                Message::Append { data, reply_tx } => {
                    let record = Record {
                        seq_num: self.next_seq,
                        data,
                    };
                    self.next_seq += 1;
                    self.records.insert(record.seq_num, record.clone());
                    // If the caller disconnected, this send fails — that's fine
                    let _ = reply_tx.send(record);
                }
                Message::Read { seq_num, reply_tx } => {
                    let _ = reply_tx.send(self.records.get(&seq_num).cloned());
                }
            }
        }
        // Channel closed — all StreamerClients dropped. Actor exits.
    }
}
```

**Why `while let Some(msg) = rx.recv().await`?**
- `recv()` yields `None` when all senders are dropped — the actor exits cleanly
- Each iteration processes exactly one message — sequencing is guaranteed
- Between messages, this task is **parked** (uses zero CPU)

Now the handle:

```rust
/// The handle — cheap to clone, safe to share across tasks
#[derive(Clone)]
pub struct StreamerClient {
    tx: mpsc::UnboundedSender<Message>,
}

impl StreamerClient {
    /// Spawn the actor and return a client handle
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let streamer = Streamer::new();
        tokio::spawn(streamer.run(rx));
        Self { tx }
    }

    pub async fn append(&self, data: Vec<u8>) -> Result<Record, &'static str> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Message::Append { data, reply_tx })
            .map_err(|_| "actor dead")?;
        reply_rx.await.map_err(|_| "actor dropped reply")
    }

    pub async fn read(&self, seq_num: u64) -> Result<Option<Record>, &'static str> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Message::Read { seq_num, reply_tx })
            .map_err(|_| "actor dead")?;
        reply_rx.await.map_err(|_| "actor dropped reply")
    }
}
```

**Key insight: `mpsc::UnboundedSender::send()` is not async.** It never blocks the caller. The message is pushed into the channel immediately. Only the reply (`reply_rx.await`) is async. This is what S2 Lite does too — fire the message, then await the oneshot.

### Test it

```rust
#[tokio::test]
async fn test_basic_append() {
    let client = StreamerClient::spawn();
    let r1 = client.append(b"hello".to_vec()).await.unwrap();
    let r2 = client.append(b"world".to_vec()).await.unwrap();
    assert_eq!(r1.seq_num, 0);
    assert_eq!(r2.seq_num, 1);

    let read = client.read(0).await.unwrap().unwrap();
    assert_eq!(read.data, b"hello");
}
```

### What you've learned

| Primitive | Role |
|---|---|
| `mpsc::unbounded_channel` | Many writers send to one actor |
| `oneshot::channel` | Actor sends single reply back to specific caller |
| `tokio::spawn` | Actor runs as independent task |

---

## Part 2: Managing Multiple Streams with `DashMap`

### The Problem

We have one actor per stream, but streams are created on-demand. We need a concurrent map from stream name → actor handle.

### Build It

Add to `Cargo.toml`:
```toml
dashmap = "6"
```

```rust
// src/backend.rs

use std::sync::Arc;
use dashmap::DashMap;
use crate::streamer::StreamerClient;

#[derive(Clone)]
pub struct Backend {
    streamers: Arc<DashMap<String, StreamerClient>>,
}

impl Backend {
    pub fn new() -> Self {
        Self {
            streamers: Arc::new(DashMap::new()),
        }
    }

    /// Get or create a streamer for the given stream name
    pub fn streamer(&self, stream_name: &str) -> StreamerClient {
        // entry() locks only this key's shard — other streams are uncontested
        self.streamers
            .entry(stream_name.to_string())
            .or_insert_with(StreamerClient::spawn)
            .clone()
    }
}
```

**Why `DashMap` instead of `Mutex<HashMap>`?**
- `DashMap` is sharded — concurrent access to different keys doesn't contend
- `entry()` is atomic — two concurrent requests for the same new stream won't spawn two actors
- S2 Lite uses exactly this pattern (`core.rs:64`), with an extra `StreamerClientSlot` enum to handle async initialization

### What S2 Lite adds that we're skipping

In the real codebase, `streamer_client_slot()` returns a `StreamerClientSlot::Initializing` with a `Shared<BoxFuture>`. This is because starting a streamer requires async work (reading state from SlateDB). Multiple callers that arrive during initialization share the same future via `.shared()` — they all await the same init, and only one does the actual work. For our tutorial, `StreamerClient::spawn()` is synchronous, so we don't need this.

---

## Part 3: Backpressure with `Semaphore`

### The Problem

Without backpressure, clients can submit data faster than storage can flush it. Memory grows unbounded until OOM.

### The Solution

A counting semaphore where each permit = 1 byte. Callers must acquire permits equal to their payload size before the actor will process their append. Permits are returned when the append is "done."

```rust
// src/streamer.rs — updated

use std::sync::Arc;
use tokio::sync::{Semaphore, SemaphorePermit};

const MAX_INFLIGHT_BYTES: usize = 16 * 1024 * 1024; // 16 MiB for tutorial

#[derive(Clone)]
pub struct StreamerClient {
    tx: mpsc::UnboundedSender<Message>,
    inflight_bytes: Arc<Semaphore>,
}

/// Holds acquired permits — dropped when append completes
pub struct AppendPermit<'a> {
    _permit: SemaphorePermit<'a>,
    client: &'a StreamerClient,
    data: Vec<u8>,
}

impl StreamerClient {
    pub fn spawn(inflight_bytes: Arc<Semaphore>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(Streamer::new().run(rx));
        Self { tx, inflight_bytes }
    }

    /// Step 1: Acquire backpressure permit (may block)
    pub async fn append_permit(&self, data: Vec<u8>) -> Result<AppendPermit<'_>, &'static str> {
        let size = (data.len() as u32).max(1);

        // This is where backpressure happens.
        // If 16 MiB is already in-flight, this blocks until
        // earlier appends complete and release their permits.
        let permit = tokio::select! {
            res = self.inflight_bytes.acquire_many(size) => {
                res.map_err(|_| "semaphore closed")
            }
            // If actor dies while we're waiting, fail fast
            _ = self.tx.closed() => {
                Err("actor dead")
            }
        }?;

        Ok(AppendPermit {
            _permit: permit,
            client: self,
            data,
        })
    }
}

impl AppendPermit<'_> {
    /// Step 2: Submit the append (permit released on drop when ack received)
    pub async fn submit(self) -> Result<Record, &'static str> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.client.tx
            .send(Message::Append {
                data: self.data,
                reply_tx,
            })
            .map_err(|_| "actor dead")?;
        let record = reply_rx.await.map_err(|_| "actor dropped reply")?;
        // self._permit drops here — releases the bytes back to the semaphore
        Ok(record)
    }
}
```

**The two-step API is the key design.** In S2 Lite this is `append_permit().await?.submit().await?` — identical pattern. The split means:

1. **`append_permit()`** — blocks until there's room. This is the admission gate.
2. **`submit()`** — sends to actor and waits for reply. Permit held the whole time.

**Why `tokio::select!` with `tx.closed()`?** If the actor crashes while we're waiting for semaphore permits, `tx.closed()` resolves immediately, and we fail fast. Without this, the caller would hang forever waiting for permits that will never be released. S2 Lite does exactly this at `streamer.rs:572-578`.

### Shared across all streams

```rust
// In Backend::new():
let semaphore = Arc::new(Semaphore::new(MAX_INFLIGHT_BYTES));

// Pass to each StreamerClient — they all share it
StreamerClient::spawn(semaphore.clone())
```

One semaphore bounds total memory across all streams. A single stream can't starve others because the semaphore is FIFO-fair.

---

## Part 4: Simulating Async Durability

### The Problem

In the real system, SlateDB writes to a memtable instantly but flushes to object storage asynchronously. The client shouldn't get an ack until the data is durable. This is the hardest concurrency pattern in the codebase.

### The Concept

```
append()
  → write to memtable (fast)        ← actor can keep processing
  → wait for flush (slow)           ← client blocks here
  → ack                             ← only now is the client unblocked
```

We'll simulate this with a delayed flush mechanism.

```rust
// src/durability.rs

use std::collections::VecDeque;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::watch;

type Callback = Box<dyn FnOnce(u64) + Send>;

struct State {
    durable_seq: u64,
    waiters: VecDeque<(u64, Callback)>, // (target_seq, callback)
}

#[derive(Clone)]
pub struct DurabilityNotifier {
    state: Arc<Mutex<State>>,
}

impl DurabilityNotifier {
    /// Start the notifier, watching a channel for flush progress
    pub fn spawn(mut flush_rx: watch::Receiver<u64>) -> Self {
        let state = Arc::new(Mutex::new(State {
            durable_seq: 0,
            waiters: VecDeque::new(),
        }));
        let state_clone = state.clone();

        tokio::spawn(async move {
            // watch::changed() suspends until the sender publishes a new value.
            // This is how SlateDB communicates flush progress.
            while flush_rx.changed().await.is_ok() {
                let new_seq = *flush_rx.borrow();
                let ready: Vec<Callback> = {
                    let mut s = state_clone.lock();
                    s.durable_seq = new_seq;
                    // Drain all waiters whose target is now met
                    let split = s.waiters.partition_point(|(seq, _)| *seq <= new_seq);
                    s.waiters
                        .drain(..split)
                        .map(|(_, cb)| cb)
                        .collect()
                };
                // Fire callbacks OUTSIDE the lock
                for cb in ready {
                    cb(new_seq);
                }
            }
        });

        Self { state }
    }

    /// Register interest: "call me when target_seq is durable"
    pub fn subscribe(&self, target_seq: u64, callback: impl FnOnce(u64) + Send + 'static) {
        let mut state = self.state.lock();
        // Already durable? Fire immediately.
        if state.durable_seq >= target_seq {
            let seq = state.durable_seq;
            drop(state); // release lock before calling back
            callback(seq);
            return;
        }
        // Insert sorted (binary search for position)
        let pos = state
            .waiters
            .partition_point(|(seq, _)| *seq <= target_seq);
        state.waiters.insert(pos, (target_seq, Box::new(callback)));
    }
}
```

**Why `parking_lot::Mutex` and not `tokio::Mutex`?**

This is a common question. The rule is:
- **`tokio::Mutex`** — use when you hold the lock across `.await` points
- **`parking_lot::Mutex`** — use when the critical section is purely synchronous

Here, the lock just pushes/pops from a VecDeque — nanoseconds. Using `tokio::Mutex` would add unnecessary overhead (it's slower for short critical sections). S2 Lite follows this same rule everywhere.

**Why callbacks instead of giving each actor its own `watch::Receiver`?**

A `watch::Receiver` wakes up on *every* update, even if its target isn't met yet. With 1000 streams, that's 1000 tasks waking up per flush. The callback pattern lets one task do the filtering and wake only the relevant actors.

### Wire it into the Streamer

Now the actor needs to track pending writes:

```rust
// src/streamer.rs — updated actor

use crate::durability::DurabilityNotifier;
use futures::future::BoxFuture;
use futures::FutureExt;
use std::collections::VecDeque;

struct PendingReply {
    reply_tx: oneshot::Sender<Record>,
    record: Record,
    write_seq: u64, // must be durable before we reply
}

enum Message {
    Append {
        data: Vec<u8>,
        reply_tx: oneshot::Sender<Record>,
    },
    Read {
        seq_num: u64,
        reply_tx: oneshot::Sender<Option<Record>>,
    },
    Durable(u64), // write_seq that is now durable
}

struct Streamer {
    next_seq: u64,
    next_write_seq: u64,
    records: HashMap<u64, Record>,
    pending_replies: VecDeque<PendingReply>,
    notifier: DurabilityNotifier,
    msg_tx: mpsc::UnboundedSender<Message>, // to send messages to ourselves
}

impl Streamer {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Message>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                Message::Append { data, reply_tx } => {
                    let record = Record {
                        seq_num: self.next_seq,
                        data,
                    };
                    self.next_seq += 1;
                    self.records.insert(record.seq_num, record.clone());

                    // "Write" to storage — in real code this is a WriteBatch
                    let write_seq = self.next_write_seq;
                    self.next_write_seq += 1;

                    // Don't reply yet! Queue it.
                    self.pending_replies.push_back(PendingReply {
                        reply_tx,
                        record,
                        write_seq,
                    });

                    // Subscribe: when this write_seq is durable, notify us
                    let tx = self.msg_tx.clone();
                    self.notifier.subscribe(write_seq, move |seq| {
                        let _ = tx.send(Message::Durable(seq));
                    });
                }
                Message::Read { seq_num, reply_tx } => {
                    let _ = reply_tx.send(self.records.get(&seq_num).cloned());
                }
                Message::Durable(durable_seq) => {
                    // Drain all pending replies that are now durable
                    while self
                        .pending_replies
                        .front()
                        .is_some_and(|p| p.write_seq <= durable_seq)
                    {
                        let pending = self.pending_replies.pop_front().unwrap();
                        let _ = pending.reply_tx.send(pending.record);
                        // ^ This unblocks the client's reply_rx.await
                        //   which then drops the SemaphorePermit
                        //   which releases backpressure
                    }
                }
            }
        }
    }
}
```

**The crucial chain reaction:**
```
DurabilityNotifier fires callback
  → sends Message::Durable to actor (via msg_tx)
  → actor drains pending_replies
  → reply_tx.send(record)
  → caller's reply_rx.await resolves
  → AppendPermit drops
  → SemaphorePermit drops
  → Semaphore permits freed
  → next blocked append_permit() unblocks
```

This is a **pipeline** — data flows through admission → sequencing → storage → durability → ack → release, and backpressure propagates backwards through the semaphore.

---

## Part 5: The `biased` Select Loop

### The Problem

Our current actor uses a simple `while let`. But what if we want the actor to:
1. Process incoming messages
2. Complete pending async storage writes
3. Self-terminate after idle timeout

We need `tokio::select!` with priority.

```rust
use futures::future::OptionFuture;
use tokio::time::{Duration, Instant};

const DORMANT_TIMEOUT: Duration = Duration::from_secs(60);

impl Streamer {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Message>) {
        let dormancy = tokio::time::sleep(Duration::MAX);
        tokio::pin!(dormancy);

        loop {
            // Reset dormancy timer every iteration
            dormancy.as_mut().reset(Instant::now() + DORMANT_TIMEOUT);

            tokio::select! {
                biased;

                // PRIORITY 1: Incoming messages
                Some(msg) = rx.recv() => {
                    self.handle_message(msg);
                }

                // PRIORITY 2: Dormancy — shut down if idle
                _ = dormancy.as_mut() => {
                    break; // exit the actor
                }
            }
        }
        // Actor exits. Channel drops. All StreamerClients see tx.closed().
    }
}
```

**`biased;`** means: always check branches top-to-bottom. Without it, tokio randomly picks among ready branches. With it:
- Messages are always processed before the timer fires
- The timer only fires when `rx.recv()` has been pending (no messages) for 60 seconds

**`tokio::pin!(dormancy)`** — `Sleep` must be pinned to be polled in `select!`. We reuse the same `Sleep` instance (resetting it) instead of creating a new one each loop to avoid allocation.

**`dormancy.as_mut().reset()`** — `as_mut()` is needed because `Pin<&mut Sleep>` requires a mutable reference. This resets the timer without reallocating.

In S2 Lite (`streamer.rs:378-473`), the real select has three branches: DB write completions (highest priority), messages, and dormancy (lowest). The `biased` ensures writes are always drained first.

---

## Part 6: Streaming Sessions with `FuturesOrdered`

### The Problem

A client sends many appends over a single HTTP connection. We want:
- **Pipelining**: start append N+1 before N is acked
- **Ordered responses**: acks come back in submission order
- **Poison on failure**: if append N fails, N+1, N+2, ... are rejected

```rust
// src/session.rs

use futures::stream::{FuturesOrdered, Stream, StreamExt};
use futures::future::OptionFuture;

pub async fn append_session(
    client: &StreamerClient,
    mut inputs: impl Stream<Item = Vec<u8>> + Unpin,
) -> impl Stream<Item = Result<Record, &'static str>> + '_ {
    async_stream::stream! {
        let mut permit_opt: Option<_> = None;
        let mut pending_acks = FuturesOrdered::new();

        loop {
            tokio::select! {
                // Branch A: Read next input from client
                // ONLY if we're not already waiting for a permit
                Some(data) = inputs.next(), if permit_opt.is_none() => {
                    permit_opt = Some(Box::pin(client.append_permit(data)));
                }

                // Branch B: Permit acquired — submit to actor
                Some(res) = OptionFuture::from(permit_opt.as_mut()) => {
                    permit_opt = None;
                    match res {
                        Ok(permit) => {
                            // Push submit future into ordered queue
                            pending_acks.push_back(permit.submit());
                        }
                        Err(e) => {
                            yield Err(e);
                            break;
                        }
                    }
                }

                // Branch C: Collect acks in order
                Some(res) = pending_acks.next(), if !pending_acks.is_empty() => {
                    match res {
                        Ok(record) => yield Ok(record),
                        Err(e) => {
                            yield Err(e);
                            break; // poison: stop on first error
                        }
                    }
                }

                else => break,
            }
        }
    }
}
```

**`FuturesOrdered`** is the key primitive. It's a queue of futures that:
- Can be pushed to (`.push_back()`)
- Yields results **in insertion order**, not completion order
- Polls all futures concurrently

So if appends 1, 2, 3 complete in order 2, 1, 3 — the stream still yields 1, 2, 3.

**The `if permit_opt.is_none()` guard** prevents reading ahead too fast. We only read the next input when the previous one has acquired its permit. This means backpressure from the semaphore propagates to the HTTP body reader.

**`OptionFuture::from(permit_opt.as_mut())`** — when `permit_opt` is `None`, `OptionFuture` resolves to `None` which is never selected. When it's `Some`, it polls the inner future. This lets you conditionally include futures in a `select!`.

---

## Part 7: Wiring It All Up

```rust
// src/main.rs

use axum::{Router, routing::post, extract::State, Json};

#[derive(Clone)]
struct AppState {
    backend: Backend,
}

#[derive(serde::Deserialize)]
struct AppendRequest {
    stream: String,
    data: String,
}

#[derive(serde::Serialize)]
struct AppendResponse {
    seq_num: u64,
}

async fn append_handler(
    State(state): State<AppState>,
    Json(req): Json<AppendRequest>,
) -> Json<AppendResponse> {
    let client = state.backend.streamer(&req.stream);
    let permit = client
        .append_permit(req.data.into_bytes())
        .await
        .expect("permit");
    let record = permit.submit().await.expect("append");
    Json(AppendResponse {
        seq_num: record.seq_num,
    })
}

#[tokio::main]
async fn main() {
    let state = AppState {
        backend: Backend::new(),
    };
    let app = Router::new()
        .route("/append", post(append_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

## Recap: The Primitive → Problem Map

| Primitive | Problem | How It Helps |
|---|---|---|
| `tokio::spawn` | Need independent concurrent execution | Actor runs as its own task |
| `mpsc::unbounded_channel` | Many-to-one communication | Writers → single actor |
| `oneshot::channel` | Request/reply pattern | Actor replies to specific caller |
| `Semaphore` | Unbounded memory growth | Caps total inflight bytes |
| `select!` + `biased` | Multiple event sources with priority | Drain writes before new messages |
| `watch::channel` | Broadcast state changes | SlateDB flush progress → notifier |
| `broadcast::channel` | Fan-out to many readers | Durable records → followers |
| `parking_lot::Mutex` | Short sync critical sections | Nanosecond locks, no async overhead |
| `FuturesOrdered` | Pipelined ordered responses | Session acks in submission order |
| `OptionFuture` | Conditional future in select | Skip branch when state is None |
| `tokio::pin!` | Reuse Sleep/futures in loops | Avoid reallocation per iteration |

Each primitive exists because a simpler alternative breaks down at scale. The tutorial builds them up in the order you'd hit the problems.
