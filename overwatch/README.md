[apache-badge]: https://img.shields.io/badge/License-Apache%202.0-blue?style=for-the-badge

[apache-url]: https://github.com/logos-co/Overwatch/blob/main/LICENSE-APACHE2.0

[mit-badge]: https://img.shields.io/badge/License-MIT-blue?style=for-the-badge

[mit-url]: https://github.com/logos-co/Overwatch/blob/main/LICENSE-MIT]

[actions-badge]: https://img.shields.io/github/actions/workflow/status/logos-co/Overwatch/main.yml?style=for-the-badge&logo=github

[actions-url]: https://github.com/logos-co/Overwatch/actions/workflows/main.yml?query=workflow%3ACI+branch%3Amain

[codecov-badge]: https://img.shields.io/codecov/c/github/logos-co/Overwatch?style=for-the-badge&logo=codecov

[codecov-url]: https://codecov.io/github/logos-co/Overwatch

[crates-badge]: https://img.shields.io/crates/v/overwatch.svg?style=for-the-badge&color=fc8d62&logo=rust

[crates-url]: https://crates.io/crates/overwatch

[docs-badge]: https://img.shields.io/docsrs/overwatch?style=for-the-badge&logo=docs.rs

[docs-url]: https://docs.rs/overwatch

# Overwatch (Core)

[![MIT License][mit-badge]][mit-url]
[![Apache License][apache-badge]][apache-url]
[![Build Status][actions-badge]][actions-url]
[![Codecov Status][codecov-badge]][codecov-url]
[![crates.io][crates-badge]][crates-url]
[![docs.rs][docs-badge]][docs-url]

**The core library for the Overwatch framework.**

This crate provides the fundamental building blocks for creating modular, interconnected applications.

---

## 📦 What's Inside

### Core Components

| Component | Description |
|-----------|-------------|
| `OverwatchRunner` | Bootstraps and runs your application |
| `OverwatchHandle` | Control handle for lifecycle management |
| `ServiceCore` | Trait that defines service behavior |
| `ServiceData` | Trait that defines service types |

### Service Utilities

| Utility | Description |
|---------|-------------|
| `Relay` | Type-safe async message channels |
| `ServiceState` | Trait for persistent state |
| `StateOperator` | Logic for state persistence |
| `LifecycleNotifier` | Service lifecycle events |
| `StatusWatcher` | Monitor service status |

---

## 🏗️ Architecture Overview

```text
                    ┌─────────────────────────┐
                    │    OverwatchRunner      │
                    │  ─────────────────────  │
                    │  • Spawns services      │
                    │  • Manages lifecycle    │
                    │  • Routes messages      │
                    └───────────┬─────────────┘
                                │
            ┌───────────────────┼───────────────────┐
            │                   │                   │
            v                   v                   v
    ┌───────────────┐   ┌───────────────┐   ┌───────────────┐
    │ ServiceRunner │   │ ServiceRunner │   │ ServiceRunner │
    │ ───────────── │   │ ───────────── │   │ ───────────── │
    │ Your Service  │   │ Your Service  │   │ Your Service  │
    └───────────────┘   └───────────────┘   └───────────────┘
```

---

## 🚀 Quick Start

### Installation

```toml
[dependencies]
overwatch = "1"
overwatch-derive = "1"
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
```

### Tokio Task Names

Enable the `tokio-task-names` feature to name Overwatch-managed service and
state-handler tasks for Tokio tracing and profiling tools. Tokio requires its
unstable configuration for task names, so consumers must also build with
`RUSTFLAGS="--cfg tokio_unstable"`. Enabling the Cargo feature alone keeps the
existing unnamed spawn behavior.

```toml
[dependencies]
overwatch = { version = "1", features = ["tokio-task-names"] }
```

### Creating a Service

Every service implements two traits:

#### 1. `ServiceData` - Define Types

```rust
use overwatch::services::{ServiceData, state::{NoOperator, NoState}};

struct MyService;
type MySettings = ();
type MyState = NoState<MySettings>;
type MyOperator = NoOperator<MyState>;
type MyMessage = ();

impl ServiceData for MyService {
    type Settings = MySettings;      // Configuration
    type State = MyState;            // Persistent state
    type StateOperator = MyOperator; // State load/save logic
    type Message = MyMessage;        // Incoming message type
}
```

#### 2. `ServiceCore` - Define Behavior

This complete stateless service waits for messages without blocking the runtime:

```rust
use async_trait::async_trait;
use overwatch::{DynError, OpaqueServiceResourcesHandle};
use overwatch::services::{ServiceCore, ServiceData, state::{NoOperator, NoState}};

type RuntimeServiceId = String;

struct MyService {
    handle: OpaqueServiceResourcesHandle<Self, RuntimeServiceId>,
}

impl ServiceData for MyService {
    type Settings = ();
    type State = NoState<()>;
    type StateOperator = NoOperator<Self::State>;
    type Message = ();
}

#[async_trait]
impl ServiceCore<RuntimeServiceId> for MyService {
    fn init(
        handle: OpaqueServiceResourcesHandle<Self, RuntimeServiceId>,
        _initial_state: Self::State,
    ) -> Result<Self, DynError> {
        Ok(Self { handle })
    }

    async fn run(mut self) -> Result<(), DynError> {
        while let Some(()) = self.handle.inbound_relay.recv().await {
            // Handle the message.
        }
        Ok(())
    }
}
```

---

## 📬 Message Passing

Services communicate via **relays** - type-safe async channels:

```rust
use std::fmt::{Debug, Display};
use overwatch::{DynError, overwatch::OverwatchHandle};
use overwatch::services::{AsServiceId, ServiceData, relay::InboundRelay};

#[derive(Debug)]
enum MyMessage { Hello }

async fn exchange<OtherService, RuntimeServiceId>(
    handle: &OverwatchHandle<RuntimeServiceId>,
    inbound: &mut InboundRelay<MyMessage>,
) -> Result<(), DynError>
where
    OtherService: ServiceData<Message = MyMessage>,
    RuntimeServiceId: AsServiceId<OtherService> + Debug + Display + Sync,
{
    // Get a relay to another service in the same runtime.
    let other_relay = handle.relay::<OtherService>().await?;
    other_relay.send(MyMessage::Hello).await?;

    while let Some(message) = inbound.recv().await {
        // Handle incoming messages.
        println!("{message:?}");
    }
    Ok(())
}
```

---

## 💾 State Management

### No State (Stateless Services)

```rust
use overwatch::services::{ServiceData, state::{NoOperator, NoState}};

struct StatelessService;

impl ServiceData for StatelessService {
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = ();
}
```

### With State (Stateful Services)

```rust
use std::convert::Infallible;
use overwatch::services::state::ServiceState;

#[derive(Default, Clone)]
struct MyState {
    counter: u32,
}

impl ServiceState for MyState {
    type Settings = ();
    type Error = Infallible;
    
    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self::default())
    }
}
```

### State Operators

State operators provide loading and snapshot-handling hooks. This minimal
operator keeps only the latest snapshot in memory; it does not persist across
restarts. For durable storage, see the
[ping-pong state operator](https://github.com/logos-co/Overwatch/blob/main/examples/ping_pong/src/operators.rs).

```rust
use std::convert::Infallible;
use async_trait::async_trait;
use overwatch::overwatch::OverwatchHandle;
use overwatch::services::state::{ServiceState, StateOperator};

#[derive(Clone, Default)]
struct MyState { counter: u32 }

impl ServiceState for MyState {
    type Settings = ();
    type Error = Infallible;

    fn from_settings(_settings: &()) -> Result<Self, Self::Error> {
        Ok(Self::default())
    }
}

#[derive(Clone, Default)]
struct MyOperator { last_state: Option<MyState> }

#[async_trait]
impl<RuntimeServiceId> StateOperator<RuntimeServiceId> for MyOperator {
    type State = MyState;
    type LoadError = Infallible;
    
    fn try_load(_settings: &()) -> Result<Option<Self::State>, Self::LoadError> {
        Ok(None) // No durable snapshot: initialize via ServiceState::from_settings.
    }

    fn from_settings(
        _settings: &(),
        _overwatch_handle: OverwatchHandle<RuntimeServiceId>,
    ) -> Self {
        Self { last_state: None }
    }
    
    async fn run(&mut self, state: Self::State) {
        self.last_state = Some(state);
    }
}
```

---

## ⚙️ Lifecycle Management

Control services programmatically:

```rust
use std::fmt::{Debug, Display};
use overwatch::overwatch::{Error, Overwatch};
use overwatch::services::AsServiceId;

async fn manage<MyService, RuntimeServiceId>(
    app: &Overwatch<RuntimeServiceId>,
) -> Result<(), Error>
where
    RuntimeServiceId: AsServiceId<MyService> + Debug + Display + Sync,
{
    let handle = app.handle();

    // Start all services.
    handle.start_all_services().await?;

    // Stop a specific service.
    handle.stop_service::<MyService>().await?;

    // Shut down everything and propagate any failure.
    handle.shutdown().await?;
    Ok(())
}
```

---

## 📖 More Information

For complete documentation and examples, see the [main README](https://github.com/logos-co/Overwatch/blob/main/README.md).

---

## 📄 License

Dual-licensed under [Apache 2.0](LICENSE-APACHE2.0) and [MIT](LICENSE-MIT).
