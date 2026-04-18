## S2 Lite — Architecture Overview

S2 Lite is a **lightweight, self-hostable streaming data service** written in Rust. It provides an HTTP API for managing **basins** (namespaces), **streams** (append-only logs), and **records** (individual data entries), backed by [SlateDB](https://slatedb.io/) — an embedded LSM-tree key-value store that persists to object storage (S3, local filesystem, or in-memory).

---

### High-Level Architecture

```
┌─────────────────────────────────────────────────────┐
│                   HTTP Clients                      │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│              Axum HTTP Server (server.rs)           │
│  Routes: /health, /metrics, /v1/*                   │
│  Middleware: CORS, Compression, Tracing             │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│              Handlers Layer (handlers/)             │
│  v1/basins.rs   — Basin CRUD                        │
│  v1/streams.rs  — Stream CRUD                       │
│  v1/records.rs  — Append, Read, CheckTail           │
│  v1/access_tokens.rs, v1/metrics.rs                 │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│                Backend (backend/)                   │
│  core.rs     — Backend struct, streamer management  │
│  basins.rs   — Basin logic (create/list/delete)     │
│  streams.rs  — Stream logic (create/list/delete)    │
│  append.rs   — Append single + session              │
│  read.rs     — Read (historical scan + live follow) │
│  streamer.rs — Per-stream actor (the core engine)   │
│  store.rs    — SlateDB read/write helpers           │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│           KV Schema Layer (backend/kv/)             │
│  Typed key/value serialization for all entities     │
│  basin_meta, stream_meta, stream_record_data,       │
│  stream_record_timestamp, stream_tail_position,     │
│  stream_trim_point, stream_fencing_token, etc.      │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│                   SlateDB                           │
│  Embedded LSM-tree KV store                         │
│  Transactions (SerializableSnapshot isolation)      │
│  TTL-based record expiry (retention policies)       │
│  Durability: flush to object store                  │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│              Object Store (S3 / Local / Memory)     │
└─────────────────────────────────────────────────────┘
```

---

### Core Concepts

#### 1. Basins (`backend/basins.rs`)
A **basin** is a top-level namespace (like a database). Each basin has:
- A `BasinConfig` with a default stream config
- A `created_at` timestamp and optional `deleted_at` (soft-delete)
- An idempotency key for safe retries on creation

Operations: **list**, **create**, **get config**, **reconfigure**, **delete** (soft-delete, then background cleanup).

#### 2. Streams (`backend/streams.rs`)
A **stream** is an append-only, ordered log within a basin. Each stream has:
- A `StreamConfig` (retention policy, timestamping, delete-on-empty settings)
- A unique `StreamId` (deterministic hash of basin + stream name)
- A tail position tracking the next writable sequence number

Streams support **auto-creation on append or read** if configured. Operations: **list**, **create**, **get config**, **reconfigure**, **delete**.

#### 3. Records (`backend/read.rs`, `backend/append.rs`, `handlers/v1/records.rs`)
Records are the actual data entries in streams. Each record has:
- A `StreamPosition` (seq_num + timestamp)
- A body (data record or command record like Fence/Trim)

---

### The Streamer — The Core Engine (`backend/streamer.rs`)

The **Streamer** is the most important component. It's a **per-stream actor** (tokio task) that serializes all writes to a stream:

```
                  ┌──────────────────────┐
  Append ────────►│                      │──── db_writes_pending ───► SlateDB
  Follow ────────►│      Streamer        │                              │
  CheckTail ─────►│   (tokio task)       │◄── DurabilityStatus ────────┘
  Reconfigure ───►│                      │──── follow_tx ───► Followers
                  └──────────────────────┘
```

**Lifecycle:**
1. Spawned on-demand when a stream is first accessed (`core.rs` manages a map of active `StreamerClient`s)
2. Receives messages via an unbounded mpsc channel
3. Goes dormant and exits after `DORMANT_TIMEOUT` if no active followers
4. Terminates on a terminal trim (`SeqNum::MAX`)

**Append flow:**
1. Client acquires a **semaphore permit** (backpressure via `append_inflight_bytes`)
2. Message sent to the Streamer actor
3. Streamer **sequences** the records (assigns seq_num + timestamp, validates fencing token / match_seq_num)
4. Applies any embedded commands (Fence, Trim)
5. Submits a `WriteBatch` to SlateDB (non-blocking, `await_durable: false`)
6. Tracks as **inflight** until a `DurabilityNotifier` confirms the write is flushed to object storage
7. Only then acknowledges the append back to the client

**Read flow:**
1. Historical records: scanned directly from SlateDB using key range queries
2. Live tail (follow): if the reader catches up to the tail, it subscribes to the Streamer's `broadcast::Sender` for real-time records
3. Three response formats: **Unary JSON/Proto** (single batch), **SSE** (Server-Sent Events stream), **S2S** (custom framed proto stream)

---

### KV Schema (`backend/kv/`)

All data is stored in SlateDB as typed key-value pairs. Keys are prefixed with a `KeyType` byte discriminant:

| KeyType | Key | Value | Purpose |
|---|---|---|---|
| `BasinMeta` | basin_name | config, timestamps | Basin metadata |
| `BasinDeletionPending` | basin_name | cursor | Tracks deletion progress |
| `StreamMeta` | basin \0 stream | config, timestamps | Stream metadata |
| `StreamIdMapping` | stream_id | basin \0 stream | Reverse lookup |
| `StreamTailPosition` | stream_id | seq_num, timestamp | Current tail |
| `StreamFencingToken` | stream_id | token bytes | Optimistic concurrency |
| `StreamTrimPoint` | stream_id | seq_num | Pending trim marker |
| `StreamRecordData` | stream_id + position | record bytes | Actual record data |
| `StreamRecordTimestamp` | stream_id + timestamp + seq_num | (empty) | Timestamp index |
| `StreamDeleteOnEmptyDeadline` | deadline + stream_id | min_age | DOE scheduling |

This schema enables efficient range scans (e.g., listing all basins, scanning records in order, finding expired DOE entries).

---

### Background Tasks (`backend/bgtasks/`)

Three background tasks run on a ~60-second interval (with jitter), also triggered by events:

| Task | Trigger | What it does |
|---|---|---|
| **stream-trim** | `StreamTrim` | Scans `StreamTrimPoint` keys, deletes record data + timestamp index entries in batches of 10K, then cleans up metadata. A full delete (`SeqNum::MAX`) also removes stream meta/id mapping. |
| **stream-delete-on-empty** | `StreamDeleteOnEmpty` | Scans expired `StreamDeleteOnEmptyDeadline` entries. If the stream has no records and meets age criteria, triggers stream deletion. |
| **basin-deletion** | `BasinDeletion` | Iterates all streams in a pending-deletion basin, deletes them one by one (resumable via cursor), then removes basin metadata. |

---

### Storage & Durability (`backend/store.rs`, `server.rs`)

**Storage backends** (configured at startup):
- `S3Bucket(name)` — production: S3-compatible object store
- `LocalFileSystem(path)` — local directory
- `InMemory` — ephemeral, for testing

**Durability model:**
- Appends are written to SlateDB with `await_durable: false` for throughput
- The `DurabilityNotifier` subscribes to SlateDB's flush notifications
- Acknowledgements to clients are only sent **after** data is confirmed durable (flushed to object store)
- Metadata operations (create/delete basin/stream, reconfigure) use `await_durable: true` for immediate consistency
- Reads from metadata use `DurabilityLevel::Remote` to ensure they see flushed data
- Transactional reads within a txn use `DurabilityLevel::Memory` to include uncommitted txn writes

**Startup fencing:** On boot, the server sleeps for `manifest_poll_interval` to ensure any prior instance is fenced out by SlateDB's manifest-based leader election.

---

### API Layer (`handlers/`)

The HTTP API is built with **Axum** and organized as:

```
/health          — GET  (DB status check)
/metrics         — GET  (Prometheus format)
/v1/
  basins/        — list, create, get_config, create_or_reconfigure, delete, reconfigure
  streams/       — list, create, get_config, create_or_reconfigure, delete, reconfigure
  records/       — check_tail, read, append
  access_tokens/ — token management
  metrics/       — stream-level metrics
```

Request/response compression is applied for payloads > 1024 bytes (excluding SSE and s2s/proto).

---

### Key Design Patterns

1. **Actor-per-stream**: Each active stream gets a dedicated Streamer tokio task, ensuring serialized writes without global locks
2. **Backpressure**: A global `Semaphore` limits total in-flight append bytes across all streams
3. **Optimistic concurrency**: Fencing tokens and match_seq_num enable conditional appends
4. **Soft-delete + background cleanup**: Deletes are marked immediately, actual data removal happens asynchronously
5. **SSE resumability**: Read sessions emit `LastEventId` with (seq_num, count, bytes) so clients can resume from where they left off
6. **TTL-based retention**: Record data is written with SlateDB TTLs matching the stream's retention policy, so expired records are automatically compacted away

