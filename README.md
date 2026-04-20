# Concrete

Concrete is a durable streaming library built on object storage ([SlateDB](https://github.com/slatedb/slatedb)).

## Concepts

- **Basins** — Top-level namespaces (like databases)
- **Streams** — Append-only ordered logs within a basin
- **Records** — Individual entries in a stream

## Features

- gRPC API for efficient client/server communication
- Persistent storage via SlateDB (S3, local filesystem, or in-memory)
- Append, read, and tail streams in real time
- Record retention policies and TTL-based expiry
- Fencing tokens for optimistic concurrency control

## Project Structure

| Crate | Description |
|---|---|
| `slatestreams` | Core library |
| `slatestreams-server` | gRPC server (port 50051) |
| `slatestreams-client` | Client library |

## Quick Start

```bash
# Run the server
cargo run --bin slatestreams-server

# Run the example client
cargo run --example client
```

## Examples

### Video Conferencing

A real-time video conferencing app where participants join rooms and stream video to each other through Concrete.

**Stack:** Next.js (React) client + Go server connected to Concrete via gRPC.

**How it works:**

1. Client joins a room via HTTP signaling and gets a participant ID
2. Client captures video with `MediaRecorder` and sends binary frames over WebSocket
3. Server appends frames to a Concrete stream and tails the stream to fan out video to other participants
4. Clients decode incoming frames with `MediaSource` for playback

```bash
# Start Concrete
cargo run --bin slatestreams-server

# Start the Go server (from examples/video-conferencing/server)
make run

# Start the Next.js client (from examples/video-conferencing/client)
npm run dev
```
