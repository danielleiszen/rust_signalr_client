use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct TestEntity {
    pub number: i32,
    pub text: String,
}

#[cfg(test)]
#[cfg(not(target_arch="wasm32"))]
pub mod tests_tokio;

#[cfg(test)]
#[cfg(not(target_arch="wasm32"))]
#[cfg(feature = "messagepack")]
pub mod tests_msgpack;