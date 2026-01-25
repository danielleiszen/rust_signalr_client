mod completer;
mod tests;
mod execution;
mod protocol;
mod client;
mod communication;

pub use client::{InvocationContext, SignalRClient, DisconnectionHandler, ReconnectionHandler};
pub use execution::{ArgumentConfiguration, CallbackHandler};
pub use completer::{CompletedFuture, ManualFuture, ManualStream};
pub use communication::reconnection::{
    ReconnectionConfig, ReconnectionPolicy,
    NoReconnectPolicy, ConstantDelayPolicy, LinearBackoffPolicy, ExponentialBackoffPolicy
};