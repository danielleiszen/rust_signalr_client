use std::collections::HashMap;
use std::io::Cursor;

use base64::Engine;

use crate::protocol::negotiate::MessageType;

// ── VarInt Framing ──────────────────────────────────────────────────────

/// Encode a length as a VarInt (LEB128-style, MSB continuation bit).
pub fn encode_varint(mut value: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
    buf
}

/// Decode a VarInt from a byte slice. Returns (value, bytes_consumed).
pub fn decode_varint(data: &[u8]) -> Result<(usize, usize), String> {
    let mut result: usize = 0;
    let mut shift = 0;
    for (i, &byte) in data.iter().enumerate() {
        if i >= 5 {
            return Err("VarInt too long".to_string());
        }
        result |= ((byte & 0x7F) as usize) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
        shift += 7;
    }
    Err("Unexpected end of VarInt".to_string())
}

/// Frame a MessagePack message with VarInt length prefix.
pub fn frame_message(payload: &[u8]) -> Vec<u8> {
    let mut framed = encode_varint(payload.len());
    framed.extend_from_slice(payload);
    framed
}

/// Extract individual messages from a binary buffer with VarInt length prefixes.
pub fn split_framed_messages(data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut messages = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let (len, varint_size) = decode_varint(&data[offset..])?;
        offset += varint_size;
        if offset + len > data.len() {
            return Err("Incomplete message in frame".to_string());
        }
        messages.push(data[offset..offset + len].to_vec());
        offset += len;
    }
    Ok(messages)
}

// ── Message Type Extraction ─────────────────────────────────────────────

/// Read the message type from the first element of a MessagePack array.
pub fn read_message_type(data: &[u8]) -> Result<MessageType, String> {
    let mut cursor = Cursor::new(data);
    let _array_len = rmp::decode::read_array_len(&mut cursor)
        .map_err(|e| format!("Not a MessagePack array: {}", e))?;
    let msg_type = rmp::decode::read_int::<u8, _>(&mut cursor)
        .map_err(|e| format!("Cannot read message type: {}", e))?;

    match msg_type {
        1 => Ok(MessageType::Invocation),
        2 => Ok(MessageType::StreamItem),
        3 => Ok(MessageType::Completion),
        4 => Ok(MessageType::StreamInvocation),
        5 => Ok(MessageType::CancelInvocation),
        6 => Ok(MessageType::Ping),
        7 => Ok(MessageType::Close),
        _ => Ok(MessageType::Other),
    }
}

// ── Outbound Encoding (Manual Array Writing) ────────────────────────────

/// Encode headers as a MessagePack map. Empty headers = fixmap 0 (0x80).
fn encode_headers(buf: &mut Vec<u8>, headers: &Option<HashMap<String, String>>) -> Result<(), String> {
    match headers {
        Some(h) if !h.is_empty() => {
            rmp::encode::write_map_len(buf, h.len() as u32).map_err(|e| e.to_string())?;
            for (k, v) in h {
                rmp::encode::write_str(buf, k).map_err(|e| e.to_string())?;
                rmp::encode::write_str(buf, v).map_err(|e| e.to_string())?;
            }
        }
        _ => {
            rmp::encode::write_map_len(buf, 0).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Encode an Invocation (type 1) or StreamInvocation (type 4).
/// Layout: [Type, Headers, InvocationId?, Target, Arguments, StreamIds]
/// Always writes 6 elements to match the .NET SignalR MessagePack protocol.
pub fn encode_invocation(
    msg_type: u8,
    headers: &Option<HashMap<String, String>>,
    invocation_id: &Option<String>,
    target: &str,
    arguments: &[rmpv::Value],
    stream_ids: &Option<Vec<String>>,
) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();

    // Always 6 elements - .NET SignalR MessagePack protocol expects exactly 6
    rmp::encode::write_array_len(&mut buf, 6).map_err(|e| e.to_string())?;
    rmp::encode::write_uint(&mut buf, msg_type as u64).map_err(|e| e.to_string())?;
    encode_headers(&mut buf, headers)?;

    // InvocationId (nil if absent)
    match invocation_id {
        Some(id) => rmp::encode::write_str(&mut buf, id).map_err(|e| e.to_string())?,
        None => rmp::encode::write_nil(&mut buf).map_err(|e| e.to_string())?,
    }

    rmp::encode::write_str(&mut buf, target).map_err(|e| e.to_string())?;

    // Arguments array
    rmpv::encode::write_value(&mut buf, &rmpv::Value::Array(arguments.to_vec()))
        .map_err(|e| e.to_string())?;

    // StreamIds (always present, empty array if none)
    let ids = stream_ids.as_deref().unwrap_or(&[]);
    rmp::encode::write_array_len(&mut buf, ids.len() as u32).map_err(|e| e.to_string())?;
    for id in ids {
        rmp::encode::write_str(&mut buf, id).map_err(|e| e.to_string())?;
    }

    Ok(buf)
}

/// Encode Ping (type 6). Layout: [6]
#[allow(dead_code)]
pub fn encode_ping() -> Vec<u8> {
    // Ping is always exactly: fixarray(1) + fixint(6) = 0x91 0x06
    vec![0x91, 0x06]
}

// ── Inbound Decoding ────────────────────────────────────────────────────

/// Parse a full MessagePack message into an rmpv::Value array.
pub fn parse_msgpack_message(data: &[u8]) -> Result<Vec<rmpv::Value>, String> {
    let value = rmpv::decode::read_value(&mut Cursor::new(data))
        .map_err(|e| format!("Failed to parse MessagePack: {}", e))?;

    match value {
        rmpv::Value::Array(items) => Ok(items),
        _ => Err("MessagePack message is not an array".to_string()),
    }
}

/// Parsed Invocation from MessagePack.
pub struct MsgpackInvocation {
    pub invocation_id: Option<String>,
    pub target: String,
    pub arguments: Vec<rmpv::Value>,
}

/// Parse Invocation or StreamInvocation.
/// Layout: [Type, Headers, InvocationId?, Target, Arguments, StreamIds?]
pub fn parse_invocation(items: &[rmpv::Value]) -> Result<MsgpackInvocation, String> {
    if items.len() < 5 {
        return Err(format!("Invocation array too short: {}", items.len()));
    }

    let invocation_id = match &items[2] {
        rmpv::Value::Nil => None,
        rmpv::Value::String(s) => s.as_str().map(|s| s.to_string()),
        _ => return Err("Invalid invocation_id type".to_string()),
    };
    let target = items[3].as_str().ok_or("Invalid target")?.to_string();
    let arguments = match &items[4] {
        rmpv::Value::Array(args) => args.clone(),
        _ => return Err("Invalid arguments".to_string()),
    };

    Ok(MsgpackInvocation { invocation_id, target, arguments })
}

/// Parsed Completion from MessagePack.
pub struct MsgpackCompletion {
    pub invocation_id: String,
    pub result_kind: u8,
    pub payload: Option<rmpv::Value>,
}

/// Parse Completion.
/// Layout: [3, Headers, InvocationId, ResultKind, Result?]
/// ResultKind: 1=Error, 2=Void, 3=NonVoid
pub fn parse_completion(items: &[rmpv::Value]) -> Result<MsgpackCompletion, String> {
    if items.len() < 4 {
        return Err("Completion array too short".to_string());
    }
    let invocation_id = items[2].as_str().ok_or("Invalid invocation_id")?.to_string();
    let result_kind = items[3].as_u64().ok_or("Invalid ResultKind")? as u8;
    let payload = if items.len() > 4 { Some(items[4].clone()) } else { None };

    Ok(MsgpackCompletion { invocation_id, result_kind, payload })
}

/// Parsed StreamItem from MessagePack.
pub struct MsgpackStreamItem {
    pub invocation_id: String,
    pub item: rmpv::Value,
}

/// Parse StreamItem.
/// Layout: [2, Headers, InvocationId, Item]
pub fn parse_stream_item(items: &[rmpv::Value]) -> Result<MsgpackStreamItem, String> {
    if items.len() < 4 {
        return Err("StreamItem array too short".to_string());
    }
    let invocation_id = items[2].as_str().ok_or("Invalid invocation_id")?.to_string();
    let item = items[3].clone();

    Ok(MsgpackStreamItem { invocation_id, item })
}

// ── Value Conversion ────────────────────────────────────────────────────

/// Convert a serde_json::Value to an rmpv::Value.
pub fn json_value_to_msgpack(value: &serde_json::Value) -> rmpv::Value {
    match value {
        serde_json::Value::Null => rmpv::Value::Nil,
        serde_json::Value::Bool(b) => rmpv::Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rmpv::Value::Integer(rmpv::Integer::from(i))
            } else if let Some(u) = n.as_u64() {
                rmpv::Value::Integer(rmpv::Integer::from(u))
            } else if let Some(f) = n.as_f64() {
                rmpv::Value::F64(f)
            } else {
                rmpv::Value::Nil
            }
        }
        serde_json::Value::String(s) => rmpv::Value::String(rmpv::Utf8String::from(s.as_str())),
        serde_json::Value::Array(arr) => {
            rmpv::Value::Array(arr.iter().map(json_value_to_msgpack).collect())
        }
        serde_json::Value::Object(obj) => {
            rmpv::Value::Map(
                obj.iter()
                    .map(|(k, v)| (rmpv::Value::String(rmpv::Utf8String::from(k.as_str())), json_value_to_msgpack(v)))
                    .collect()
            )
        }
    }
}

/// Convert an rmpv::Value to a serde_json::Value.
pub fn msgpack_value_to_json(value: &rmpv::Value) -> serde_json::Value {
    match value {
        rmpv::Value::Nil => serde_json::Value::Null,
        rmpv::Value::Boolean(b) => serde_json::Value::Bool(*b),
        rmpv::Value::Integer(i) => {
            if let Some(v) = i.as_i64() {
                serde_json::Value::Number(serde_json::Number::from(v))
            } else if let Some(v) = i.as_u64() {
                serde_json::Value::Number(serde_json::Number::from(v))
            } else {
                serde_json::Value::Null
            }
        }
        rmpv::Value::F32(f) => {
            serde_json::Number::from_f64(*f as f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        rmpv::Value::F64(f) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        rmpv::Value::String(s) => {
            serde_json::Value::String(s.as_str().unwrap_or("").to_string())
        }
        rmpv::Value::Binary(b) => {
            serde_json::Value::String(base64::engine::general_purpose::STANDARD.encode(b))
        }
        rmpv::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(msgpack_value_to_json).collect())
        }
        rmpv::Value::Map(entries) => {
            let obj: serde_json::Map<String, serde_json::Value> = entries.iter()
                .filter_map(|(k, v)| {
                    k.as_str().map(|key| (key.to_string(), msgpack_value_to_json(v)))
                })
                .collect();
            serde_json::Value::Object(obj)
        }
        rmpv::Value::Ext(_, _) => serde_json::Value::Null,
    }
}

/// Convert PascalCase or any case to camelCase (lowercase first letter).
fn to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        result.push(first.to_ascii_lowercase());
    }
    result.extend(chars);
    result
}

/// Convert camelCase or any case to PascalCase (uppercase first letter).
#[allow(dead_code)]
fn to_pascal_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        result.push(first.to_ascii_uppercase());
    }
    result.extend(chars);
    result
}

fn transform_keys(value: &rmpv::Value, transform: fn(&str) -> String) -> rmpv::Value {
    match value {
        rmpv::Value::Map(entries) => {
            rmpv::Value::Map(
                entries.iter()
                    .map(|(k, v)| {
                        let new_key = match k {
                            rmpv::Value::String(s) => {
                                rmpv::Value::String(rmpv::Utf8String::from(
                                    transform(s.as_str().unwrap_or(""))
                                ))
                            },
                            _ => k.clone(),
                        };
                        (new_key, transform_keys(v, transform))
                    })
                    .collect()
            )
        },
        rmpv::Value::Array(arr) => {
            rmpv::Value::Array(arr.iter().map(|v| transform_keys(v, transform)).collect())
        },
        other => other.clone(),
    }
}

/// Normalize map keys in an rmpv::Value tree from PascalCase to camelCase (inbound from .NET).
fn normalize_keys_to_camel(value: &rmpv::Value) -> rmpv::Value {
    transform_keys(value, to_camel_case)
}

/// Normalize map keys in an rmpv::Value tree from camelCase to PascalCase (outbound to .NET).
#[allow(dead_code)]
pub fn normalize_keys_to_pascal(value: &rmpv::Value) -> rmpv::Value {
    transform_keys(value, to_pascal_case)
}

/// Deserialize an rmpv::Value into a concrete Rust type via rmp-serde.
/// Handles both array format (.NET StandardResolver) and map format (ContractlessStandardResolver).
/// For maps, normalizes PascalCase keys to camelCase for serde compatibility.
pub fn value_to_type<T: serde::de::DeserializeOwned>(value: &rmpv::Value) -> Result<T, String> {
    // First, try direct deserialization (works for arrays and primitive types)
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, value)
        .map_err(|e| format!("Failed to encode value: {}", e))?;

    if let Ok(result) = rmp_serde::from_slice::<T>(&buf) {
        return Ok(result);
    }

    // If direct deserialization fails, try normalizing map keys (PascalCase → camelCase)
    let normalized = normalize_keys_to_camel(value);
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &normalized)
        .map_err(|e| format!("Failed to encode value: {}", e))?;
    rmp_serde::from_slice::<T>(&buf)
        .map_err(|e| format!("Failed to deserialize from MessagePack: {}", e))
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        for &value in &[0, 1, 53, 127, 128, 5248, 16384, 2_147_483_647] {
            let encoded = encode_varint(value);
            let (decoded, consumed) = decode_varint(&encoded).unwrap();
            assert_eq!(decoded, value);
            assert_eq!(consumed, encoded.len());
        }
    }

    #[test]
    fn test_varint_known_values() {
        assert_eq!(encode_varint(53), vec![0x35]);
        assert_eq!(encode_varint(5248), vec![0x80, 0x29]);
    }

    #[test]
    fn test_frame_split_roundtrip() {
        let msg1 = vec![0x91, 0x06]; // Ping [6]
        let msg2 = vec![0x92, 0x01, 0x80]; // some array

        let mut framed = frame_message(&msg1);
        framed.extend(frame_message(&msg2));

        let messages = split_framed_messages(&framed).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], msg1);
        assert_eq!(messages[1], msg2);
    }

    #[test]
    fn test_ping_encoding() {
        let ping = encode_ping();
        assert_eq!(ping, vec![0x91, 0x06]);

        let msg_type = read_message_type(&ping).unwrap();
        assert_eq!(msg_type, MessageType::Ping);
    }

    #[test]
    fn test_read_message_type() {
        // Invocation: fixarray(5) + fixint(1) + ...
        let mut buf = Vec::new();
        rmp::encode::write_array_len(&mut buf, 5).unwrap();
        rmp::encode::write_uint(&mut buf, 1).unwrap();
        // remaining fields don't matter for type extraction
        assert_eq!(read_message_type(&buf).unwrap(), MessageType::Invocation);
    }

    #[test]
    fn test_json_msgpack_value_roundtrip() {
        let json = serde_json::json!({
            "name": "test",
            "number": 42,
            "flag": true,
            "nested": {"inner": [1, 2, 3]},
            "nothing": null
        });

        let msgpack = json_value_to_msgpack(&json);
        let back = msgpack_value_to_json(&msgpack);
        assert_eq!(json, back);
    }

    #[test]
    fn test_encode_decode_invocation() {
        let args = vec![
            rmpv::Value::String(rmpv::Utf8String::from("hello")),
            rmpv::Value::Integer(rmpv::Integer::from(42)),
        ];

        let encoded = encode_invocation(
            1,
            &None,
            &Some("inv_1".to_string()),
            "TestMethod",
            &args,
            &None,
        ).unwrap();

        let items = parse_msgpack_message(&encoded).unwrap();
        assert_eq!(items[0].as_u64().unwrap(), 1);

        let inv = parse_invocation(&items).unwrap();
        assert_eq!(inv.target, "TestMethod");
        assert_eq!(inv.invocation_id, Some("inv_1".to_string()));
        assert_eq!(inv.arguments.len(), 2);
        assert_eq!(inv.arguments[0].as_str().unwrap(), "hello");
        assert_eq!(inv.arguments[1].as_u64().unwrap(), 42);
    }

    #[test]
    fn test_value_to_type() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct TestStruct {
            name: String,
            value: i32,
        }

        // Build a MessagePack map value
        let val = rmpv::Value::Map(vec![
            (rmpv::Value::String("name".into()), rmpv::Value::String("test".into())),
            (rmpv::Value::String("value".into()), rmpv::Value::Integer(42.into())),
        ]);

        let result: TestStruct = value_to_type(&val).unwrap();
        assert_eq!(result, TestStruct { name: "test".to_string(), value: 42 });
    }

    #[test]
    fn test_full_invocation_encoding_flow() {
        // Simulate the exact PushTwoEntities encoding flow
        #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
        struct TestEntity {
            number: i32,
            text: String,
        }

        let entity1 = TestEntity { text: "entity1".to_string(), number: 200 };
        let entity2 = TestEntity { text: "entity2".to_string(), number: 300 };

        // Step 1: Serialize to JSON (as ArgumentConfiguration does)
        let json1 = serde_json::to_value(&entity1).unwrap();
        let json2 = serde_json::to_value(&entity2).unwrap();

        // Step 2: Convert JSON to msgpack value
        let raw1 = json_value_to_msgpack(&json1);
        let raw2 = json_value_to_msgpack(&json2);

        // Step 3: Normalize keys to PascalCase
        let norm1 = normalize_keys_to_pascal(&raw1);
        let norm2 = normalize_keys_to_pascal(&raw2);

        // Verify the normalized values have PascalCase keys and correct values
        if let rmpv::Value::Map(entries) = &norm1 {
            let keys: Vec<&str> = entries.iter()
                .filter_map(|(k, _)| k.as_str())
                .collect();
            assert!(keys.contains(&"Number"), "Expected 'Number' key, got: {:?}", keys);
            assert!(keys.contains(&"Text"), "Expected 'Text' key, got: {:?}", keys);

            for (k, v) in entries {
                if k.as_str() == Some("Number") {
                    assert_eq!(v.as_i64(), Some(200), "Number value mismatch");
                }
                if k.as_str() == Some("Text") {
                    assert_eq!(v.as_str(), Some("entity1"), "Text value mismatch");
                }
            }
        } else {
            panic!("Expected Map, got: {:?}", norm1);
        }

        // Step 4: Encode as invocation
        let args = vec![norm1.clone(), norm2.clone()];
        let payload = encode_invocation(
            1,
            &None,
            &Some("PushTwoEntities_4".to_string()),
            "PushTwoEntities",
            &args,
            &None,
        ).unwrap();

        // Step 5: Parse back and verify
        let items = parse_msgpack_message(&payload).unwrap();
        let inv = parse_invocation(&items).unwrap();
        assert_eq!(inv.target, "PushTwoEntities");
        assert_eq!(inv.arguments.len(), 2);

        // Verify argument values survive the round-trip
        let arg1 = &inv.arguments[0];
        let arg2 = &inv.arguments[1];

        if let rmpv::Value::Map(entries) = arg1 {
            for (k, v) in entries {
                if k.as_str() == Some("Number") {
                    assert_eq!(v.as_i64(), Some(200), "Arg1 Number mismatch after round-trip");
                }
            }
        } else {
            panic!("Arg1 not a Map: {:?}", arg1);
        }

        if let rmpv::Value::Map(entries) = arg2 {
            for (k, v) in entries {
                if k.as_str() == Some("Number") {
                    assert_eq!(v.as_i64(), Some(300), "Arg2 Number mismatch after round-trip");
                }
            }
        } else {
            panic!("Arg2 not a Map: {:?}", arg2);
        }

        // Step 6: Compare with direct rmp_serde serialization
        let direct_bytes1 = rmp_serde::to_vec_named(&entity1).unwrap();
        let direct_val1 = rmpv::decode::read_value(&mut Cursor::new(&direct_bytes1)).unwrap();
        let direct_norm1 = normalize_keys_to_pascal(&direct_val1);

        // Compare hex representation
        let mut our_bytes = Vec::new();
        rmpv::encode::write_value(&mut our_bytes, &norm1).unwrap();
        let mut direct_bytes_norm = Vec::new();
        rmpv::encode::write_value(&mut direct_bytes_norm, &direct_norm1).unwrap();

        // They should produce the same result
        assert_eq!(our_bytes, direct_bytes_norm, "Encoding mismatch between JSON path and direct rmp_serde path");
    }

    #[test]
    fn test_array_format_serialization() {
        // Verify that rmp_serde::to_vec (array format) produces arrays, not maps
        #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
        struct TestEntity {
            number: i32,
            text: String,
        }

        let entity = TestEntity { text: "entity1".to_string(), number: 200 };

        // Array format (what .NET StandardResolver expects)
        let array_bytes = rmp_serde::to_vec(&entity).unwrap();
        let array_val = rmpv::decode::read_value(&mut Cursor::new(&array_bytes)).unwrap();

        // Should be an array [200, "entity1"] (field declaration order)
        assert!(matches!(array_val, rmpv::Value::Array(_)),
            "Expected Array format, got: {:?}", array_val);

        if let rmpv::Value::Array(items) = &array_val {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].as_i64(), Some(200), "First element should be number");
            assert_eq!(items[1].as_str(), Some("entity1"), "Second element should be text");
        }

        // Map format (what ContractlessStandardResolver expects)
        let map_bytes = rmp_serde::to_vec_named(&entity).unwrap();
        let map_val = rmpv::decode::read_value(&mut Cursor::new(&map_bytes)).unwrap();

        // Should be a map {"number": 200, "text": "entity1"}
        assert!(matches!(map_val, rmpv::Value::Map(_)),
            "Expected Map format, got: {:?}", map_val);

        // Both formats should deserialize back correctly
        let from_array: TestEntity = rmp_serde::from_slice(&array_bytes).unwrap();
        assert_eq!(from_array, entity);

        let from_map: TestEntity = rmp_serde::from_slice(&map_bytes).unwrap();
        assert_eq!(from_map, entity);
    }

    #[test]
    fn test_value_to_type_handles_both_formats() {
        #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
        struct TestEntity {
            number: i32,
            text: String,
        }

        // Array format (from .NET StandardResolver)
        let array_val = rmpv::Value::Array(vec![
            rmpv::Value::Integer(42.into()),
            rmpv::Value::String("hello".into()),
        ]);
        let from_array: TestEntity = value_to_type(&array_val).unwrap();
        assert_eq!(from_array, TestEntity { number: 42, text: "hello".to_string() });

        // Map format with PascalCase keys (from .NET ContractlessStandardResolver)
        let map_val = rmpv::Value::Map(vec![
            (rmpv::Value::String("Number".into()), rmpv::Value::Integer(42.into())),
            (rmpv::Value::String("Text".into()), rmpv::Value::String("hello".into())),
        ]);
        let from_map: TestEntity = value_to_type(&map_val).unwrap();
        assert_eq!(from_map, TestEntity { number: 42, text: "hello".to_string() });
    }
}
