# CLAUDE.md - signalr-client

## Project Overview

A cross-platform Rust library for calling SignalR hubs, supporting both native (Tokio) and WebAssembly (WASM) targets. Published on crates.io as `signalr-client`.

**Repository**: https://github.com/danielleiszen/rust_signalr_client
**Version**: 0.1.2
**License**: MIT

## Architecture

### Module Structure

```
src/
├── lib.rs                      # Public API exports
├── client/                     # Public-facing client API
│   ├── client.rs               # SignalRClient - main entry point
│   ├── configuration.rs        # ConnectionConfiguration builder
│   └── context.rs              # InvocationContext for callbacks
├── communication/              # Transport layer (platform-specific)
│   ├── common.rs               # Communication trait, HttpClient, ConnectionData
│   ├── client_tokio.rs         # Tokio WebSocket client (non-WASM)
│   ├── client_wasm.rs          # Browser WebSocket client (WASM)
│   └── reconnection.rs         # Reconnection policies
├── execution/                  # Action dispatch & storage
│   ├── storage.rs              # Storage trait, message routing
│   ├── storage_tokio.rs        # Arc<Mutex<HashMap>> storage (non-WASM)
│   ├── storage_wasm.rs         # Rc<RefCell<HashMap>> storage (WASM)
│   ├── actions.rs              # UpdatableAction trait
│   ├── invocation.rs           # Request-response actions
│   ├── enumerable.rs           # Streaming actions (IAsyncEnumerable)
│   ├── callback.rs             # Server-initiated callback actions
│   └── arguments.rs            # ArgumentConfiguration builder
├── completer/                  # Async primitives
│   ├── manual_future.rs        # ManualFuture<T> (waker-based)
│   ├── manual_stream.rs        # ManualStream<T> (queue-based)
│   └── completed_future.rs     # Synchronous completed future
├── protocol/                   # SignalR protocol messages
│   ├── messages.rs             # MessageParser, RECORD_SEPARATOR (\u{001E})
│   ├── negotiate.rs            # NegotiateResponse, HandshakeRequest, Ping
│   ├── invoke.rs               # Invocation, Completion messages
│   ├── streaming.rs            # StreamItem message
│   └── close.rs                # Close message
└── tests/
    ├── tests_tokio.rs          # Integration test (requires running .NET backend)
    └── tests_wasm.rs           # WASM test
```

### Platform Abstraction

The library uses `#[cfg(target_arch = "wasm32")]` for platform-specific code:
- **Non-WASM**: Tokio runtime, `tokio-websockets`, `Arc<Mutex<>>` for thread safety
- **WASM**: `wasm-sockets` PollingClient, `Rc<RefCell<>>` for single-threaded access, `setInterval` polling loop

Key traits that abstract platform differences:
- `Communication` - Transport connect/send/disconnect
- `Storage` - Action storage and message routing

### Connection Flow

1. HTTP POST to `{url}/negotiate?negotiateVersion=1` via `ehttp`
2. Parse `NegotiateResponseV0`, verify WebSocket+Text transport
3. Open WebSocket connection (TLS if `wss://`)
4. Send/receive JSON handshake (`{"protocol":"json","version":1}`)
5. Spawn receive loop (tokio task or setInterval in WASM)
6. Route incoming messages to stored actions by invocation ID

### Key Types

- **SignalRClient**: Public API. Holds `UpdatableActionStorage` + `CommunicationClient`. Cloneable (shares underlying connection).
- **CommunicationClient**: Platform-specific WebSocket wrapper with connection state machine.
- **UpdatableActionStorage**: Registry of pending actions keyed by invocation ID.
- **UpdatableAction** trait: Polymorphic actions - `InvocationAction<T>`, `EnumerableAction<T>`, `CallbackAction`.
- **ManualFuture<T>** / **ManualStream<T>**: Custom async primitives bridging sync message handling to async completion.

### Reconnection (features/reconnection branch)

Two modes:
1. **Automatic**: No `DisconnectionHandler` set - retries using configured `ReconnectionPolicy`
2. **Manual**: `DisconnectionHandler` provided - user receives `ReconnectionHandler` for full control

Policies: `NoReconnectPolicy` (default), `ConstantDelayPolicy`, `LinearBackoffPolicy`, `ExponentialBackoffPolicy`

Reconnection is not supported in WASM.

## Build & Test

```bash
# Build (native)
cargo build

# Build (WASM)
trunk build

# Run tests (requires .NET SignalR test backend at localhost:5220)
# Start backend first: cd dotnet/SignalRTestService && dotnet run
cargo test
```

### Test Backend

The `dotnet/SignalRTestService/` directory contains an ASP.NET Core SignalR hub for integration testing. It must be running on `localhost:5220` for tests to pass.

## Known Issues

- **Clone + Drop bug**: `SignalRClient::drop()` calls `disconnect()` which checks `Arc::strong_count` on the inner `CommunicationConnection`, but since `CommunicationClient` clones share the same `_state` Arc (not the inner connection Arc), the ref count check is incorrect. Dropping a cloned client can disconnect the shared connection. This causes the test to fail at the callback section after `drop(secondclient)`.

## Dependencies

**Common**: serde, serde_json, futures, ehttp, base64, log
**WASM**: wasm-bindgen, wasm-sockets, wasm-timer, async-std
**Tokio**: tokio (full), tokio-websockets, tokio-native-tls, http

## Conventions

- Builder pattern via closures: `connect_with(domain, hub, |config| { ... })`
- Platform selection via `cfg` attributes at module level
- Actions auto-remove from storage when completed
- Messages separated by `RECORD_SEPARATOR` (`\u{001E}`)
- Invocation IDs formatted as `{MethodName}_{counter}`
