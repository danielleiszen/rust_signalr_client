# SignalR Client for Rust - Claude Integration Documentation

## Project Overview

This is a Rust SignalR client library designed to provide cross-platform support for calling SignalR hubs, with first-class support for both native (tokio) and WebAssembly targets. The library provides a high-level API for invoking hub methods, streaming data, and registering server-to-client callbacks.

**Repository:** https://github.com/danielleiszen/rust_signalr_client
**Current Version:** 0.1.1
**Key Features:**
- Cross-platform (native and WASM)
- Async/await support
- Streaming data support via `IAsyncEnumerable`
- Server-to-client callbacks
- Multiple authentication methods (Basic, Bearer)

## Architecture Overview

### Core Components

#### 1. Client Layer (`src/client/`)
- **`SignalRClient`** (`client.rs`): Main user-facing API
  - Manages connection lifecycle
  - Provides methods for `invoke`, `send`, `enumerate`, and `register` (callbacks)
  - Cloneable - multiple instances share the same underlying connection
  - Holds references to `CommunicationClient` and `UpdatableActionStorage`

#### 2. Communication Layer (`src/communication/`)
- **`HttpClient`** (`common.rs`): Handles negotiation via HTTP POST
- **`CommunicationClient`** (platform-specific):
  - **Tokio** (`client_tokio.rs`): WebSocket client using `tokio-websockets`
  - **WASM** (`client_wasm.rs`): WebSocket client using `wasm-sockets` with polling

#### 3. Execution Layer (`src/execution/`)
- **`Storage`** trait: Manages pending invocations, streams, and callbacks
- **Action types:**
  - `InvocationAction`: Single request-response
  - `EnumerableAction`: Streaming responses
  - `CallbackAction`: Server-initiated calls

#### 4. Protocol Layer (`src/protocol/`)
- Message serialization/deserialization
- Handshake, Ping, Close messages
- Invocation and streaming protocols

---

## Current Connection Process

### Connection Flow

```
1. User calls SignalRClient::connect() or connect_with()
   ↓
2. HttpClient::negotiate() - HTTP POST to /negotiate endpoint
   ↓
3. Receives ConnectionData (endpoint URL, connection ID)
   ↓
4. CommunicationClient::connect()
   ↓
5. WebSocket connection established
   ↓
6. Handshake sent (protocol: "json", version: 1)
   ↓
7. Handshake response received
   ↓
8. Message receive loop starts
   ↓
9. SignalRClient ready for use
```

### Connection State Management

#### Tokio Implementation
```rust
enum ConnectionState {
    NotConnected,
    Connected(Arc<Mutex<CommunicationConnection>>)
}
```

#### WASM Implementation
```rust
enum ConnectionState {
    Connect(ManualFutureState),
    Handshake(ManualFutureState),
    Process(UpdatableActionStorage),
}
```

### Disconnect Behavior

**Current limitations:**
- `disconnect()` is final - no way to reconnect
- Reference counting determines when to actually close
- Pending invocations are lost
- Callbacks are cleared
- No graceful handling of unexpected disconnections

---

## Refactoring Opportunities for Reconnection Support

### Problem Analysis

The current architecture has several issues preventing reconnection:

1. **Tight Coupling**: Connection state is tightly coupled with application state (Storage)
2. **One-Shot Design**: Connection is created once and cannot be re-established
3. **No Monitoring**: No detection of connection health or unexpected disconnects
4. **State Loss**: Pending invocations and callbacks are lost on disconnect
5. **No Retry Logic**: No automatic or manual reconnection mechanisms
6. **Immutable Configuration**: Connection parameters can't be updated post-creation

### Proposed Refactoring Strategy

#### 1. **Introduce Connection Manager**

Create a new `ConnectionManager` struct that handles connection lifecycle:

```rust
pub struct ConnectionManager {
    config: Arc<RwLock<ConnectionConfiguration>>,
    state: Arc<RwLock<ConnectionState>>,
    reconnection_policy: ReconnectionPolicy,
    connection_handle: Option<CommunicationClient>,
    event_handler: Option<Box<dyn ConnectionEventHandler>>,
}

pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected { since: Instant },
    Reconnecting { attempt: u32, next_retry: Instant },
    Failed { error: String },
}

pub struct ReconnectionPolicy {
    enabled: bool,
    max_attempts: Option<u32>,
    initial_delay: Duration,
    max_delay: Duration,
    backoff_multiplier: f64, // e.g., 1.5 for exponential backoff
    reconnect_on_close_allowed: bool, // Honor server's allowReconnect
}
```

**Benefits:**
- Centralized connection logic
- State machine for connection lifecycle
- Configurable reconnection behavior
- Event-driven architecture

#### 2. **Separate Transport from Application State**

Currently, `UpdatableActionStorage` is created fresh for each connection. Instead:

```rust
pub struct SignalRClient {
    connection_manager: ConnectionManager,
    storage: Arc<RwLock<UpdatableActionStorage>>, // Persistent across reconnections
    invocation_timeout: Duration,
}
```

**Changes needed:**
- Storage persists across reconnections
- Pending invocations can be:
  - **Replayed** after reconnection (with timeout)
  - **Failed** if timeout exceeded
  - **Cleared** based on policy
- Callbacks remain registered across reconnections

#### 3. **Implement Connection Monitoring**

Add health checking and connection quality tracking:

```rust
pub struct ConnectionMonitor {
    last_ping: Instant,
    last_pong: Instant,
    ping_interval: Duration,
    ping_timeout: Duration,
    metrics: ConnectionMetrics,
}

pub struct ConnectionMetrics {
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    reconnection_count: AtomicU32,
    latency_samples: VecDeque<Duration>,
}
```

**Features:**
- Automatic ping/pong monitoring
- Detect stale connections
- Track connection quality
- Trigger reconnection on timeout

#### 4. **Add Reconnection Logic**

Implement automatic reconnection with exponential backoff:

```rust
impl ConnectionManager {
    async fn reconnect_loop(&mut self) -> Result<(), String> {
        let mut attempt = 0;
        let mut delay = self.reconnection_policy.initial_delay;

        loop {
            attempt += 1;

            if let Some(max) = self.reconnection_policy.max_attempts {
                if attempt > max {
                    return Err("Max reconnection attempts reached".to_string());
                }
            }

            // Update state
            *self.state.write().unwrap() = ConnectionState::Reconnecting {
                attempt,
                next_retry: Instant::now() + delay,
            };

            // Wait before retry
            tokio::time::sleep(delay).await;

            // Attempt reconnection
            match self.connect_internal().await {
                Ok(_) => {
                    self.on_reconnected(attempt).await;
                    return Ok(());
                }
                Err(e) => {
                    self.on_reconnection_failed(attempt, &e).await;

                    // Calculate next delay with exponential backoff
                    delay = std::cmp::min(
                        Duration::from_secs_f64(
                            delay.as_secs_f64() * self.reconnection_policy.backoff_multiplier
                        ),
                        self.reconnection_policy.max_delay,
                    );
                }
            }
        }
    }
}
```

#### 5. **Connection Event Hooks**

Allow users to respond to connection lifecycle events:

```rust
pub trait ConnectionEventHandler: Send + Sync {
    fn on_connecting(&self) {}
    fn on_connected(&self) {}
    fn on_disconnected(&self, reason: DisconnectReason) {}
    fn on_reconnecting(&self, attempt: u32) {}
    fn on_reconnected(&self, attempt: u32) {}
    fn on_reconnection_failed(&self, error: &str) {}
}

pub enum DisconnectReason {
    UserRequested,
    ServerClosed { allow_reconnect: bool, error: Option<String> },
    NetworkError(String),
    Timeout,
}
```

**Use cases:**
- Update UI connection status
- Log connection events
- Implement custom retry logic
- Clear/preserve application state

#### 6. **Graceful Degradation**

Handle connection loss without crashing:

```rust
impl SignalRClient {
    pub async fn invoke<T>(&mut self, target: String) -> Result<T, SignalRError> {
        // Check connection state
        let state = self.connection_manager.state.read().unwrap().clone();

        match state {
            ConnectionState::Connected { .. } => {
                // Normal flow
                self.invoke_internal(target).await
            }
            ConnectionState::Reconnecting { .. } => {
                // Queue invocation or return error
                Err(SignalRError::Reconnecting)
            }
            _ => {
                // Try to reconnect if policy allows
                if self.connection_manager.reconnection_policy.enabled {
                    self.connection_manager.reconnect().await?;
                    self.invoke_internal(target).await
                } else {
                    Err(SignalRError::NotConnected)
                }
            }
        }
    }
}
```

#### 7. **Message Queue During Reconnection**

Buffer outgoing messages during reconnection:

```rust
pub struct OutgoingMessageQueue {
    queue: VecDeque<QueuedMessage>,
    max_size: usize,
    strategy: QueueStrategy,
}

pub enum QueueStrategy {
    DropOldest,
    DropNewest,
    Fail,
}

struct QueuedMessage {
    data: String,
    queued_at: Instant,
    timeout: Duration,
}
```

**Behavior:**
- Queue messages when disconnected/reconnecting
- Flush queue after successful reconnection
- Drop messages based on strategy when queue is full

#### 8. **Configuration API Updates**

Add reconnection configuration to `ConnectionConfiguration`:

```rust
impl ConnectionConfiguration {
    pub fn with_reconnection(&mut self, policy: ReconnectionPolicy) -> &mut Self {
        self.reconnection_policy = Some(policy);
        self
    }

    pub fn disable_auto_reconnect(&mut self) -> &mut Self {
        self.reconnection_policy = None;
        self
    }

    pub fn with_connection_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.connection_timeout = timeout;
        self
    }
}
```

---

## Implementation Plan

### Phase 1: Foundation (Non-breaking)
- [ ] Add `ConnectionState` enum with more granular states
- [ ] Create `ConnectionMonitor` for health checking
- [ ] Implement `ConnectionMetrics` tracking
- [ ] Add `ConnectionEventHandler` trait (optional hooks)

### Phase 2: Reconnection Core
- [ ] Implement `ConnectionManager` with reconnection logic
- [ ] Add `ReconnectionPolicy` configuration
- [ ] Implement exponential backoff retry logic
- [ ] Honor server's `allowReconnect` from Close messages

### Phase 3: State Preservation
- [ ] Decouple `Storage` from connection lifetime
- [ ] Implement pending invocation timeout/replay logic
- [ ] Preserve callbacks across reconnections
- [ ] Add `OutgoingMessageQueue` for buffering

### Phase 4: Public API
- [ ] Add `SignalRClient::reconnect()` manual method
- [ ] Add `SignalRClient::connection_state()` getter
- [ ] Update `ConnectionConfiguration` with reconnection options
- [ ] Add connection event callback registration

### Phase 5: Platform-Specific Implementation
- [ ] Refactor `client_tokio.rs` to use `ConnectionManager`
- [ ] Refactor `client_wasm.rs` to use `ConnectionManager`
- [ ] Ensure consistent behavior across platforms

### Phase 6: Testing & Documentation
- [ ] Add reconnection unit tests
- [ ] Add integration tests with simulated disconnections
- [ ] Update README with reconnection examples
- [ ] Add migration guide for existing users

---

## Key Design Decisions

### 1. **Automatic vs Manual Reconnection**
**Decision:** Support both
- Automatic reconnection via `ReconnectionPolicy` (opt-in)
- Manual reconnection via `client.reconnect()` method
- Users can disable automatic and implement custom logic via event handlers

### 2. **Pending Invocations During Reconnection**
**Decision:** Configurable behavior
- Default: Fail pending invocations with `SignalRError::ConnectionLost`
- Optional: Queue and replay (with timeout)
- Streaming invocations always fail (cannot be replayed)

### 3. **Callback Preservation**
**Decision:** Always preserve
- Callbacks represent long-lived subscriptions
- Re-registered automatically after reconnection
- User can unregister manually if needed

### 4. **Connection State Visibility**
**Decision:** Expose via getter
- Users can query current connection state
- State changes trigger event handlers
- Useful for UI updates

### 5. **Backward Compatibility**
**Decision:** Non-breaking changes where possible
- Existing API continues to work
- New features opt-in via configuration
- Default behavior: no auto-reconnect (matches current)

---

## Example Usage After Refactoring

```rust
use signalr_client::{SignalRClient, ReconnectionPolicy};
use std::time::Duration;

// Configure client with reconnection
let client = SignalRClient::connect_with("localhost", "chatHub", |config| {
    config
        .with_port(5220)
        .unsecure()
        .with_reconnection(ReconnectionPolicy {
            enabled: true,
            max_attempts: Some(5),
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            reconnect_on_close_allowed: true,
        })
        .with_connection_event_handler(Box::new(MyEventHandler));
}).await.unwrap();

// Register connection event handler
struct MyEventHandler;

impl ConnectionEventHandler for MyEventHandler {
    fn on_disconnected(&self, reason: DisconnectReason) {
        println!("Disconnected: {:?}", reason);
    }

    fn on_reconnecting(&self, attempt: u32) {
        println!("Reconnecting... attempt {}", attempt);
    }

    fn on_reconnected(&self, attempt: u32) {
        println!("Reconnected after {} attempts", attempt);
    }
}

// Check connection state
match client.connection_state() {
    ConnectionState::Connected { since } => {
        println!("Connected for {:?}", since.elapsed());
    }
    ConnectionState::Reconnecting { attempt, next_retry } => {
        println!("Reconnecting (attempt {}), next retry in {:?}",
                 attempt, next_retry - Instant::now());
    }
    _ => {}
}

// Manual reconnection
if client.connection_state().is_disconnected() {
    client.reconnect().await?;
}
```

---

## Performance Considerations

### Memory
- `ConnectionManager` adds minimal overhead (~200 bytes)
- `OutgoingMessageQueue` size is configurable
- `ConnectionMetrics` uses atomic operations (no locks)

### CPU
- Reconnection logic runs in background task
- No polling in tokio implementation (event-driven)
- WASM polling interval unchanged (100ms)

### Network
- Exponential backoff reduces server load during outages
- Configurable max retry attempts prevents infinite loops
- Ping/pong monitoring detects stale connections early

---

## Testing Strategy

### Unit Tests
- Connection state transitions
- Backoff calculation correctness
- Message queue behavior
- Callback preservation logic

### Integration Tests
- Simulate server disconnections
- Test reconnection success/failure
- Verify invocation timeout behavior
- Cross-platform consistency

### Manual Testing
- Network interruption scenarios
- Server restart scenarios
- Long-running connections
- High message volume during reconnection

---

## Migration Path for Existing Users

### Version 0.2.0 (Backward Compatible)
- Add reconnection features
- Default: auto-reconnect **disabled**
- Existing code works unchanged

### Version 1.0.0 (Breaking Changes - Optional)
- Consider enabling auto-reconnect by default
- Simplify API based on 0.2.0 feedback
- Remove deprecated methods (if any)

---

## Related Files

### Core Connection Logic
- `src/client/client.rs:134-219` - `SignalRClient::connect_internal()`
- `src/communication/client_tokio.rs:163-213` - Tokio connection establishment
- `src/communication/client_wasm.rs:111-170` - WASM connection establishment
- `src/communication/common.rs:39-55` - HTTP negotiation

### State Management
- `src/execution/storage.rs:43-137` - Storage trait and message processing
- `src/communication/client_tokio.rs:68-80` - Tokio ConnectionState enum
- `src/communication/client_wasm.rs:19-24` - WASM ConnectionState enum

### Protocol
- `src/protocol/negotiate.rs:82-91` - Close message with `allow_reconnect`
- `src/protocol/negotiate.rs:57-74` - Ping message handling

### Disconnect Logic
- `src/client/client.rs:524-526` - Client disconnect method
- `src/communication/client_tokio.rs:126-148` - Tokio disconnect with reference counting
- `src/communication/client_wasm.rs:262-282` - WASM disconnect with reference counting

---

## Questions for Consideration

1. **Should reconnection preserve the same connection ID?**
   - Likely requires re-negotiation with the server
   - May need special server-side support

2. **How to handle version mismatches after reconnection?**
   - Server may have been upgraded during downtime
   - Should handshake be re-validated?

3. **Should streaming operations be restartable?**
   - Complex: need to track stream position
   - May require server-side support for resumption

4. **What about authentication token expiry during long reconnection periods?**
   - Add token refresh callback?
   - Fail gracefully and require user re-authentication?

5. **Should there be a "connected" guarantee for invoke operations?**
   - Wait for connection before executing?
   - Or fail immediately if not connected?

---

## Conclusion

Adding robust reconnection support to the SignalR client requires thoughtful refactoring to separate connection lifecycle from application state. The proposed architecture introduces a `ConnectionManager` that handles automatic reconnection with configurable policies while preserving backward compatibility. This will make the library more resilient to network issues and provide a better developer experience for real-world applications.

The implementation can be done incrementally across multiple versions, starting with non-breaking additions and progressing to more advanced features based on user feedback.
