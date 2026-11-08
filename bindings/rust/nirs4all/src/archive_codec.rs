//! Shared neutral V1 path, integrity and charged JSON structure guards.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::archive_storage::ArchiveStoreError;

const MANIFEST: &str = "manifest.json";

/// Working bytes attributed to one JSON node, beyond its token bytes.
const JSON_NODE_BYTES: usize = 32;

/// Scan JSON structure and report the working cost of each node as it is
/// reached. `charge` returns `false` to stop the scan, so a caller can bound a
/// document whose nesting it does not limit. The cost is proportional to the
/// input and also stands for the size of the decoded document.
pub(crate) fn scan_json_structure_charged(
    bytes: &[u8],
    max_nesting: usize,
    charge: &mut dyn FnMut(usize) -> bool,
) -> Result<(), ArchiveStoreError> {
    let mut at = 0;
    let mut frames = Vec::new();
    let mut root_complete = false;
    scan_json_value_iterative(
        bytes,
        &mut at,
        &mut frames,
        &mut root_complete,
        max_nesting,
        charge,
    )?;
    while !root_complete {
        let frame = frames
            .pop()
            .ok_or_else(|| fmt_err("manifest has an incomplete JSON value"))?;
        match frame {
            JsonScanFrame::Object { mut keys, state } => match state {
                JsonObjectState::KeyOrEnd | JsonObjectState::KeyRequired => {
                    skip_json_whitespace(bytes, &mut at);
                    if bytes.get(at) == Some(&b'}') && matches!(state, JsonObjectState::KeyOrEnd) {
                        at += 1;
                        complete_scanned_json_value(&mut frames, &mut root_complete)?;
                    } else if bytes.get(at) == Some(&b'"') {
                        let (start, end) = scan_json_string(bytes, &mut at)?;
                        let key: String = serde_json::from_slice(&bytes[start..end])
                            .map_err(|_| fmt_err("manifest has an invalid object key"))?;
                        charge_json_node(charge, key.len())?;
                        if !keys.insert(key.clone()) {
                            return refuse(&format!("manifest has duplicate object key `{key}`"));
                        }
                        frames.push(JsonScanFrame::Object {
                            keys,
                            state: JsonObjectState::Colon,
                        });
                    } else {
                        return refuse("manifest object key must be a string");
                    }
                }
                JsonObjectState::Colon => {
                    skip_json_whitespace(bytes, &mut at);
                    if bytes.get(at) != Some(&b':') {
                        return refuse("manifest object key lacks ':'");
                    }
                    at += 1;
                    frames.push(JsonScanFrame::Object {
                        keys,
                        state: JsonObjectState::Value,
                    });
                    scan_json_value_iterative(
                        bytes,
                        &mut at,
                        &mut frames,
                        &mut root_complete,
                        max_nesting,
                        charge,
                    )?;
                }
                JsonObjectState::Value => return refuse("manifest object is truncated"),
                JsonObjectState::CommaOrEnd => {
                    skip_json_whitespace(bytes, &mut at);
                    match bytes.get(at) {
                        Some(b',') => {
                            at += 1;
                            frames.push(JsonScanFrame::Object {
                                keys,
                                state: JsonObjectState::KeyRequired,
                            });
                        }
                        Some(b'}') => {
                            at += 1;
                            complete_scanned_json_value(&mut frames, &mut root_complete)?;
                        }
                        _ => return refuse("manifest object is truncated"),
                    }
                }
            },
            JsonScanFrame::Array(state) => match state {
                JsonArrayState::ValueOrEnd | JsonArrayState::ValueRequired => {
                    skip_json_whitespace(bytes, &mut at);
                    if bytes.get(at) == Some(&b']') && matches!(state, JsonArrayState::ValueOrEnd) {
                        at += 1;
                        complete_scanned_json_value(&mut frames, &mut root_complete)?;
                    } else {
                        frames.push(JsonScanFrame::Array(JsonArrayState::Value));
                        scan_json_value_iterative(
                            bytes,
                            &mut at,
                            &mut frames,
                            &mut root_complete,
                            max_nesting,
                            charge,
                        )?;
                    }
                }
                JsonArrayState::Value => return refuse("manifest array is truncated"),
                JsonArrayState::CommaOrEnd => {
                    skip_json_whitespace(bytes, &mut at);
                    match bytes.get(at) {
                        Some(b',') => {
                            at += 1;
                            frames.push(JsonScanFrame::Array(JsonArrayState::ValueRequired));
                        }
                        Some(b']') => {
                            at += 1;
                            complete_scanned_json_value(&mut frames, &mut root_complete)?;
                        }
                        _ => return refuse("manifest array is truncated"),
                    }
                }
            },
        }
    }
    skip_json_whitespace(bytes, &mut at);
    if at != bytes.len() {
        return refuse("manifest has trailing JSON data");
    }
    Ok(())
}

enum JsonScanFrame {
    Object {
        keys: BTreeSet<String>,
        state: JsonObjectState,
    },
    Array(JsonArrayState),
}

#[derive(Clone, Copy)]
enum JsonObjectState {
    KeyOrEnd,
    KeyRequired,
    Colon,
    Value,
    CommaOrEnd,
}

#[derive(Clone, Copy)]
enum JsonArrayState {
    ValueOrEnd,
    ValueRequired,
    Value,
    CommaOrEnd,
}

fn scan_json_value_iterative(
    bytes: &[u8],
    at: &mut usize,
    frames: &mut Vec<JsonScanFrame>,
    root_complete: &mut bool,
    max_nesting: usize,
    charge: &mut dyn FnMut(usize) -> bool,
) -> Result<(), ArchiveStoreError> {
    skip_json_whitespace(bytes, at);
    match bytes.get(*at) {
        Some(b'{') => {
            charge_json_node(charge, 0)?;
            push_json_frame(
                frames,
                JsonScanFrame::Object {
                    keys: BTreeSet::new(),
                    state: JsonObjectState::KeyOrEnd,
                },
                max_nesting,
            )?;
            *at += 1;
            Ok(())
        }
        Some(b'[') => {
            charge_json_node(charge, 0)?;
            push_json_frame(
                frames,
                JsonScanFrame::Array(JsonArrayState::ValueOrEnd),
                max_nesting,
            )?;
            *at += 1;
            Ok(())
        }
        Some(b'"') => {
            let (start, end) = scan_json_string(bytes, at)?;
            charge_json_node(charge, end - start)?;
            complete_scanned_json_value(frames, root_complete)
        }
        Some(_) => {
            let start = *at;
            while bytes.get(*at).is_some_and(|byte| !is_json_delimiter(*byte)) {
                *at += 1;
            }
            if !valid_json_atom(&bytes[start..*at]) {
                return refuse("JSON has an invalid primitive token");
            }
            charge_json_node(charge, 0)?;
            complete_scanned_json_value(frames, root_complete)
        }
        None => refuse("manifest is truncated"),
    }
}

fn charge_json_node(
    charge: &mut dyn FnMut(usize) -> bool,
    token_bytes: usize,
) -> Result<(), ArchiveStoreError> {
    if !charge(JSON_NODE_BYTES.saturating_add(token_bytes)) {
        return refuse("JSON working memory budget refused the document");
    }
    Ok(())
}

fn push_json_frame(
    frames: &mut Vec<JsonScanFrame>,
    frame: JsonScanFrame,
    max_nesting: usize,
) -> Result<(), ArchiveStoreError> {
    if frames.len() >= max_nesting {
        return refuse("manifest nesting exceeds the V2 structural limit");
    }
    frames.push(frame);
    Ok(())
}

fn complete_scanned_json_value(
    frames: &mut [JsonScanFrame],
    root_complete: &mut bool,
) -> Result<(), ArchiveStoreError> {
    let Some(parent) = frames.last_mut() else {
        *root_complete = true;
        return Ok(());
    };
    match parent {
        JsonScanFrame::Object { state, .. } if matches!(state, JsonObjectState::Value) => {
            *state = JsonObjectState::CommaOrEnd;
            Ok(())
        }
        JsonScanFrame::Array(state) if matches!(state, JsonArrayState::Value) => {
            *state = JsonArrayState::CommaOrEnd;
            Ok(())
        }
        _ => refuse("manifest has an invalid JSON value position"),
    }
}

fn scan_json_string(bytes: &[u8], at: &mut usize) -> Result<(usize, usize), ArchiveStoreError> {
    let start = *at;
    *at += 1;
    loop {
        match bytes.get(*at) {
            Some(b'"') => {
                *at += 1;
                // String/escape syntax only. Dataset values are decoded by IO.
                serde_json::from_slice::<String>(&bytes[start..*at])
                    .map_err(|_| fmt_err("JSON has an invalid string"))?;
                return Ok((start, *at));
            }
            Some(b'\\') => {
                *at = at
                    .checked_add(2)
                    .ok_or_else(|| fmt_err("manifest string offset overflow"))?;
            }
            Some(_) => *at += 1,
            None => return refuse("manifest string is truncated"),
        }
    }
}

fn skip_json_whitespace(bytes: &[u8], at: &mut usize) {
    while bytes
        .get(*at)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        *at += 1;
    }
}

fn is_json_delimiter(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t' | b',' | b']' | b'}')
}

// Lexical JSON number validation, without converting or rounding the value.
// This extends the existing structure scanner, not the IO dataset decoder.
fn valid_json_atom(token: &[u8]) -> bool {
    if matches!(token, b"true" | b"false" | b"null") {
        return true;
    }
    let mut i = usize::from(token.first() == Some(&b'-'));
    match token.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            i += 1;
            while token.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        _ => return false,
    }
    if token.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while token.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if matches!(token.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(token.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while token.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == token.len()
}

/// Refuse every path shape unsafe for any archive family, independent of length.
pub(crate) fn path_shape_ok(path: &str) -> Result<(), ArchiveStoreError> {
    if path.is_empty()
        || path == MANIFEST
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
        || path.nfc().collect::<String>() != path
    {
        return refuse("unsafe archive member path");
    }
    Ok(())
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn refuse<T>(detail: &str) -> Result<T, ArchiveStoreError> {
    Err(fmt_err(detail))
}
fn fmt_err(detail: &str) -> ArchiveStoreError {
    ArchiveStoreError::Format(format!("archive V2 format refusal: {detail}"))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_manifest_preflight_retains_duplicate_and_depth_guards() {
        let mut charged = 0;
        assert!(
            scan_json_structure_charged(br#"{"a":[0]}"#, 2, &mut |bytes| {
                charged += bytes;
                true
            })
            .is_ok()
        );
        assert!(charged > 0);
        assert!(scan_json_structure_charged(br#"{"a":[[0]]}"#, 2, &mut |_| true).is_err());
        assert!(scan_json_structure_charged(br#"{"a":0,"\u0061":1}"#, 2, &mut |_| true).is_err());
        assert!(scan_json_structure_charged(b"{}", usize::MAX, &mut |_| false).is_err());
    }
}
