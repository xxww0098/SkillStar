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
            let length = usize::try_from(length).ok()?;
            if content_offset.checked_add(length)? > data.len() {
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

/// Rotate only the OAuth fields, retaining client metadata at every nesting
/// level and any unrelated topic entries. An unknown/malformed shape is not
/// permission to replace the client's session with a synthetic one.
pub(crate) fn refresh_unified_oauth_token(
    data: &[u8],
    access: &str,
    refresh: &str,
    expiry: i64,
) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut offset = 0;
    let mut found = false;
    while offset < data.len() {
        let (tag, start) = read_varint(data, offset).ok()?;
        let end = skip_field(data, start, (tag & 7) as u8).ok()?;
        let original = data.get(offset..end)?;
        if tag == 10 {
            let entry = extract_bytes_field(original, 1)?;
            if extract_string_field(entry, 1).as_deref() == Some(OAUTH_SENTINEL_KEY) {
                if found {
                    return None;
                }
                let row = extract_bytes_field(entry, 2)?;
                let info = general_purpose::STANDARD
                    .decode(extract_string_field(row, 1)?)
                    .ok()?;
                let info = replace_field(&info, 1, &encode_string_field(1, access))?;
                let info = replace_field(&info, 3, &encode_string_field(3, refresh))?;
                let timestamp = replace_field(
                    extract_bytes_field(&info, 4).unwrap_or_default(),
                    1,
                    &encode_varint_field(1, expiry.max(0) as u64),
                )?;
                let info = replace_field(&info, 4, &encode_len_delimited_field(4, &timestamp))?;
                let row = replace_field(
                    row,
                    1,
                    &encode_string_field(1, &general_purpose::STANDARD.encode(info)),
                )?;
                let entry = replace_field(entry, 2, &encode_len_delimited_field(2, &row))?;
                result.extend(encode_len_delimited_field(1, &entry));
                found = true;
                offset = end;
                continue;
            }
        }
        result.extend_from_slice(original);
        offset = end;
    }
    found.then_some(result)
}

fn replace_field(data: &[u8], field: u32, replacement: &[u8]) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let (tag, start) = read_varint(data, offset).ok()?;
        let end = skip_field(data, start, (tag & 7) as u8).ok()?;
        let original = data.get(offset..end)?;
        if tag >> 3 != u64::from(field) {
            result.extend_from_slice(original);
        }
        offset = end;
    }
    result.extend_from_slice(replacement);
    Some(result)
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
            let length = usize::try_from(length).ok()?;
            if content_offset.checked_add(length)? > data.len() {
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
            let length = usize::try_from(length).ok()?;
            if content_offset.checked_add(length)? > data.len() {
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
            let length = usize::try_from(length).ok()?;
            if content_offset.checked_add(length)? > data.len() {
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
        if shift == 63 && byte > 1 {
            return Err(());
        }
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
        1 => offset.checked_add(8).ok_or(()),
        2 => {
            let (length, content_offset) = read_varint(data, offset)?;
            content_offset
                .checked_add(usize::try_from(length).map_err(|_| ())?)
                .ok_or(())
        }
        5 => offset.checked_add(4).ok_or(()),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_preserves_nested_client_fields_and_rejects_bad_blobs() {
        let mut info =
            create_oauth_info("old-access", "old-refresh", 100, Some("alice@example.com"));
        let extra = encode_string_field(90, "client-private");
        info.extend_from_slice(&extra);
        let mut row = encode_string_field(1, &general_purpose::STANDARD.encode(info));
        row.extend_from_slice(&extra);
        let mut entry = encode_string_field(1, OAUTH_SENTINEL_KEY);
        entry.extend(encode_len_delimited_field(2, &row));
        entry.extend_from_slice(&extra);
        let mut original = create_unified_topic_entry("unrelated", b"opaque");
        original.extend(encode_len_delimited_field(1, &entry));
        original.extend_from_slice(&extra);
        let updated =
            refresh_unified_oauth_token(&original, "new-access", "new-refresh", 200).unwrap();
        let token = extract_oauth_token_from_unified_oauth_token(&updated).unwrap();
        assert_eq!(token.access_token, "new-access");
        assert_eq!(token.refresh_token, "new-refresh");
        assert_eq!(token.expires_at, Some(200));
        assert_eq!(token.email.as_deref(), Some("alice@example.com"));
        // Decode each layer because protobuf field order is immaterial.
        let first_len = create_unified_topic_entry("unrelated", b"opaque").len();
        assert_eq!(&updated[..first_len], &original[..first_len]);
        assert!(updated.ends_with(&extra));
        let entry = extract_bytes_field(&updated[first_len..], 1).unwrap();
        assert_eq!(
            extract_string_field(entry, 90).as_deref(),
            Some("client-private")
        );
        let row = extract_bytes_field(entry, 2).unwrap();
        assert_eq!(
            extract_string_field(row, 90).as_deref(),
            Some("client-private")
        );
        let info = general_purpose::STANDARD
            .decode(extract_string_field(row, 1).unwrap())
            .unwrap();
        assert_eq!(
            extract_string_field(&info, 90).as_deref(),
            Some("client-private")
        );
        assert!(refresh_unified_oauth_token(&[0x80; 20], "a", "r", 1).is_none());
        assert!(refresh_unified_oauth_token(&[10, 127], "a", "r", 1).is_none());
        assert!(refresh_unified_oauth_token(b"", "a", "r", 1).is_none());
    }
}
