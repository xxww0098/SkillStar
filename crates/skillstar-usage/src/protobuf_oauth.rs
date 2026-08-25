//! Minimal protobuf helpers for Antigravity IDE unified OAuth token blobs.

use base64::{Engine as _, engine::general_purpose};

const OAUTH_SENTINEL_KEY: &str = "oauthTokenInfoSentinelKey";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedOAuthToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: Option<i64>,
    pub email: Option<String>,
}

pub fn extract_oauth_token_from_unified_oauth_token(data: &[u8]) -> Option<UnifiedOAuthToken> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == 1 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            let entry = &data[content_offset..content_offset + length];
            if let Some(token) = extract_oauth_token_from_unified_entry(entry) {
                return Some(token);
            }
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }

    None
}

pub fn extract_refresh_token_from_unified_oauth_token(data: &[u8]) -> Option<String> {
    extract_oauth_token_from_unified_oauth_token(data).map(|token| token.refresh_token)
}

/// Build the `antigravityUnifiedStateSync.oauthToken` value used by the
/// desktop IDE. The outer message is a repeated Topic.data entry; the row
/// stores a base64-encoded OAuthTokenInfo protobuf.
pub fn create_unified_oauth_token(
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    email: Option<&str>,
) -> Vec<u8> {
    let oauth_info = create_oauth_info(access_token, refresh_token, expiry, email);
    create_unified_topic_entry(OAUTH_SENTINEL_KEY, &oauth_info)
}

fn create_oauth_info(
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    email: Option<&str>,
) -> Vec<u8> {
    let mut oauth_info = [
        encode_string_field(1, access_token),
        encode_string_field(2, "Bearer"),
        encode_string_field(3, refresh_token),
    ]
    .concat();

    let mut timestamp = encode_varint_field(1, expiry.max(0) as u64);
    timestamp.extend(encode_varint_field(2, 0));
    oauth_info.extend(encode_len_delimited_field(4, &timestamp));
    if let Some(email) = email.map(str::trim).filter(|value| !value.is_empty()) {
        oauth_info.extend(encode_string_field(5, email));
    }
    oauth_info
}

fn create_unified_topic_entry(sentinel_key: &str, payload: &[u8]) -> Vec<u8> {
    let row = encode_string_field(1, &general_purpose::STANDARD.encode(payload));
    let entry = [
        encode_string_field(1, sentinel_key),
        encode_len_delimited_field(2, &row),
    ]
    .concat();
    encode_len_delimited_field(1, &entry)
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 0x80 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}

fn encode_varint_field(field_num: u32, value: u64) -> Vec<u8> {
    let mut field = encode_varint((field_num << 3) as u64);
    field.extend(encode_varint(value));
    field
}

fn encode_string_field(field_num: u32, value: &str) -> Vec<u8> {
    encode_len_delimited_field(field_num, value.as_bytes())
}

fn encode_len_delimited_field(field_num: u32, value: &[u8]) -> Vec<u8> {
    let mut field = encode_varint(((field_num << 3) | 2) as u64);
    field.extend(encode_varint(value.len() as u64));
    field.extend_from_slice(value);
    field
}

fn extract_oauth_token_from_unified_entry(data: &[u8]) -> Option<UnifiedOAuthToken> {
    let mut offset = 0;
    let mut sentinel_matched = false;
    let mut row_data: Option<Vec<u8>> = None;

    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            let value = &data[content_offset..content_offset + length];
            if field_num == 1 {
                sentinel_matched = std::str::from_utf8(value).ok()? == "oauthTokenInfoSentinelKey";
            } else if field_num == 2 {
                row_data = Some(value.to_vec());
            }
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }

    if !sentinel_matched {
        return None;
    }

    let row_data = row_data?;
    let oauth_info_b64 = extract_string_field(&row_data, 1)?;
    let oauth_info = general_purpose::STANDARD.decode(oauth_info_b64).ok()?;
    let access_token = extract_string_field(&oauth_info, 1)?;
    let refresh_token = extract_string_field(&oauth_info, 3)?;
    let expires_at = extract_bytes_field(&oauth_info, 4)
        .and_then(|timestamp| extract_varint_field(timestamp, 1).map(|seconds| seconds as i64));
    let email = extract_string_field(&oauth_info, 5);
    Some(UnifiedOAuthToken {
        access_token,
        refresh_token,
        expires_at,
        email,
    })
}

fn extract_string_field(data: &[u8], target_field: u32) -> Option<String> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            return std::str::from_utf8(&data[content_offset..content_offset + length])
                .ok()
                .map(str::to_string);
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }
    None
}

fn extract_bytes_field(data: &[u8], target_field: u32) -> Option<&[u8]> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            return Some(&data[content_offset..content_offset + length]);
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }
    None
}

fn extract_varint_field(data: &[u8], target_field: u32) -> Option<u64> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 0 {
            return read_varint(data, new_offset).ok().map(|(value, _)| value);
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }
    None
}

fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), ()> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;
    loop {
        if pos >= data.len() {
            return Err(());
        }
        let byte = data[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok((result, pos))
}

fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, ()> {
    match wire_type {
        0 => {
            let (_, new_offset) = read_varint(data, offset)?;
            Ok(new_offset)
        }
        1 => Ok(offset + 8),
        2 => {
            let (length, content_offset) = read_varint(data, offset)?;
            Ok(content_offset + length as usize)
        }
        5 => Ok(offset + 4),
        _ => Err(()),
    }
}
