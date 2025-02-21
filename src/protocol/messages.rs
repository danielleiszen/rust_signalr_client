use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

pub const RECORD_SEPARATOR: &str = "\u{001E}";

pub struct MessageParser {

}

impl MessageParser {
    pub fn to_json<T: ?Sized + Serialize>(value: &T) -> Result<String, serde_json::Error> {
        let serialized = serde_json::to_string(value)?;
        Ok(serialized + RECORD_SEPARATOR)
    }

    pub fn to_json_value<T: ?Sized + Serialize>(value: &T) -> Result<Value, serde_json::Error> {
        let serialized = serde_json::to_value(value)?;
        Ok(serialized)
    }

    pub fn strip_record_separator(input: &str) -> &str {
        input.trim_end_matches(RECORD_SEPARATOR)
    }

    pub fn parse_message<T: DeserializeOwned>(message: &str) -> Result<T, String> {
        let response= serde_json::from_str::<T>(message);

        if response.is_ok() {
            Ok(response.unwrap())
        } else {
            Err(response.err().unwrap().to_string())
        }
    }
}