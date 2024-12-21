# Rusdis: A Redis-like Key-Value Store in Rust

Rusdis is a lightweight, high-performance, in-memory key-value store built from scratch in Rust. It mimics the basic functionalities of Redis, with a focus on simplicity, performance, and leveraging Rust's safety guarantees.

## Features

- **In-Memory Storage**: Fast data retrieval using an in-memory architecture.
- **Key-Value Operations**:
  - `SET` and `GET` commands for storing and retrieving values.
  - `DEL` for deleting keys.
- **Data Persistence**:
  - Append-only file (AOF) support for durable storage.
  - Periodic snapshots using a custom implementation of RDB-like persistence.
- **Concurrency**: Multi-threaded request handling using Rust's async capabilities.
- **Customizable**: Configurable memory limits, persistence intervals, and more.
- **Extensible**: Modular design allows easy extension of features.

## Getting Started

### Prerequisites

- Rust (minimum version: 1.65.0)

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/rusdis.git
   cd rusdis
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. Run the server:
   ```bash
   cargo run --release
   ```

4. Use any Redis-compatible client (e.g., `redis-cli`):
   ```bash
   redis-cli -h 127.0.0.1 -p 6379
   ```

### Usage

#### Example Commands:

```bash
SET key value
GET key
DEL key
```

#### Configuration:

- `port`: Port number for the server (default: `6379`)
- - `dir`: Directory to store RDB file
- `dbfilename`: Name of the RDB filekj
- `replicaof`: Master's port number to listen to

```bash
cargo run --release -- --dir /path/to/rdbfile --dbfilename backup.rdb --port 6380 --replicaof 6379
```

## Architecture

- **Storage Engine**: Implements a hash map for fast key-value access, with optional expiration times.
- **Persistence**:
  - AOF: Logs all write operations for recovery after restarts.
  - Snapshots: Periodic full-disk dumps of the in-memory state.
- **Networking**: Uses `tokio` for asynchronous request handling.
- **Command Parser**: Parses Redis-like commands from clients.

## Roadmap

- [ ] Add support for pub/sub functionality.
- [ ] Implement clustering for horizontal scaling.
- [ ] Introduce more advanced data structures like lists, sets, and sorted sets.
- [ ] Enhance security with authentication mechanisms.
- [ ] Improve AOF compression and performance.



