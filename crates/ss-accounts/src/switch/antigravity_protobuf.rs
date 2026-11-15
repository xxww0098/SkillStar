//! Antigravity 的 unified OAuth token：base64 包着的极简 protobuf。
//!
//! `antigravityUnifiedStateSync.oauthToken` 的值是 `base64(Topic)`。Topic 里
//! 每条 entry 是 `field1 哨兵键 + field2 Row`，Row 的 field1 是 `base64(Info)`：
//!
//! ```text
//! Topic = repeated field 1 entry
//! Entry = field 1 "oauthTokenInfoSentinelKey", field 2 Row
//! Row   = field 1 base64(Info)
//! Info  = field 1 access_token, field 2 "Bearer", field 3 refresh_token,
//!         field 4 Timestamp{ field 1 seconds, field 2 nanos }, field 5 email
//! ```
//!
//! 只覆盖这套结构（varint + length-delimited），不引 prost：多一个依赖只为读
//! 五个字段不值得。编码时每一层的未知字段和其它 topic 条目都原样保留——这是
//! 写回客户端自己的存储，丢掉客户端塞的字段等于把它的会话弄坏。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// 这条 entry 的哨兵键。
pub(crate) const SENTINEL_KEY: &str = "oauthTokenInfoSentinelKey";

/// Info 的 field 2：google 的 token_type 永远是 Bearer。
const TOKEN_TYPE: &str = "Bearer";

/// 从 blob 里读出来的一份会话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredSession {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    pub(crate) expires_at: Option<i64>,
    pub(crate) email: Option<String>,
}

/// 要写进 blob 的会话。email 传 None 表示"保留客户端原来那个"。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Projection<'a> {
    pub(crate) access_token: &'a str,
    pub(crate) refresh_token: &'a str,
    pub(crate) expires_at: i64,
    pub(crate) email: Option<&'a str>,
}

// ---------------------------------------------------------------------------
// 读
// ---------------------------------------------------------------------------

/// 从 `antigravityUnifiedStateSync.oauthToken` 的值里读出会话。
pub(crate) fn read(blob: &[u8]) -> Option<StoredSession> {
    let fields = split(&sentinel_info(blob)?)?;
    let access_token = string_field(&fields, 1)?;
    let refresh_token = string_field(&fields, 3)?;
    if access_token.trim().is_empty() || refresh_token.trim().is_empty() {
        return None;
    }
    Some(StoredSession {
        access_token,
        refresh_token,
        expires_at: timestamp_seconds(&fields),
        email: string_field(&fields, 5).filter(|email| !email.trim().is_empty()),
    })
}

/// 哨兵 entry 里的 Info 字节。
fn sentinel_info(blob: &[u8]) -> Option<Vec<u8>> {
    let row = sentinel_row(blob)?;
    let encoded = read_row_string(&row)?;
    STANDARD.decode(encoded.trim()).ok()
}

/// 哨兵 entry 里 Row 的 payload。
fn sentinel_row(blob: &[u8]) -> Option<Vec<u8>> {
    for entry in split(blob)?.iter().filter(|field| is_entry(field)) {
        if string_field(&split(&entry.payload)?, 1).as_deref() != Some(SENTINEL_KEY) {
            continue;
        }
        return split(&entry.payload)?
            .into_iter()
            .find(|field| field.number == 2)
            .map(|field| field.payload);
    }
    None
}

/// Row 的 field1（base64 过的 Info）。
fn read_row_string(row: &[u8]) -> Option<String> {
    string_field(&split(row)?, 1)
}

fn timestamp_seconds(fields: &[Field]) -> Option<i64> {
    let inner = split(
        &fields
            .iter()
            .find(|field| field.number == 4)?
            .payload,
    )?;
    inner
        .iter()
        .find(|field| field.number == 1 && field.wire_type == WIRE_VARINT)
        .and_then(|field| read_varint(&field.payload, 0))
        .map(|(seconds, _)| seconds as i64)
}

// ---------------------------------------------------------------------------
// 写
// ---------------------------------------------------------------------------

/// 把会话投影进旧 blob：只替换哨兵 entry 里的 OAuth 字段。
///
/// 旧 blob 缺失或解不出结构时新建一条完整 entry——正是"客户端第一次见到这个
/// 账号"的形态。旧 blob 里其它 topic 条目与各层未知字段都原样保留。
pub(crate) fn project(existing: Option<&[u8]>, session: &Projection<'_>) -> Vec<u8> {
    let info = project_info(existing.and_then(sentinel_info).as_deref(), session);
    let row = replace_field(
        &existing.and_then(sentinel_row).unwrap_or_default(),
        1,
        len_field(1, STANDARD.encode(&info).as_bytes()),
    );
    let row_field = len_field(2, &row);
    match existing {
        Some(blob) => project_entry(blob, row_field),
        None => len_field(1, &entry_with_sentinel(&row_field)),
    }
}

/// 新 Info：老字段照抄，只换 token 相关的那几个（email 为 None 时保留原值）。
fn project_info(existing: Option<&[u8]>, session: &Projection<'_>) -> Vec<u8> {
    let empty: &[u8] = &[];
    let old = existing.unwrap_or(empty);
    let mut info = replace_field(old, 1, len_field(1, session.access_token.as_bytes()));
    info = replace_field(&info, 2, len_field(2, TOKEN_TYPE.as_bytes()));
    info = replace_field(&info, 3, len_field(3, session.refresh_token.as_bytes()));
    let expires_at = session.expires_at.max(0) as u64;
    let timestamp = match split(&info)
        .unwrap_or_default()
        .into_iter()
        .find(|field| field.number == 4)
    {
        Some(field) => project_timestamp(&field.payload, expires_at),
        None => project_timestamp(empty, expires_at),
    };
    info = replace_field(&info, 4, timestamp);
    if let Some(email) = session.email {
        info = replace_field(&info, 5, len_field(5, email.as_bytes()));
    }
    info
}

/// Timestamp 只换秒；纳秒字段（和客户端塞进 Timestamp 的未知字段）保留。
fn project_timestamp(existing: &[u8], expires_at: u64) -> Vec<u8> {
    let fields = split(existing).unwrap_or_default();
    let mut timestamp = varint_field(1, expires_at);
    for field in fields.iter().filter(|field| field.number != 1) {
        timestamp.extend(field.raw.clone());
    }
    if !fields.iter().any(|field| field.number == 2) {
        timestamp.extend(varint_field(2, 0));
    }
    len_field(4, &timestamp)
}

fn project_entry(blob: &[u8], row_field: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut replaced = false;
    for field in split(blob).unwrap_or_default() {
        if !replaced && is_entry(&field) && entry_is_ours(&field) {
            let entry = replace_field(&field.payload, 2, row_field.clone());
            out.extend(len_field(1, &entry));
            replaced = true;
            continue;
        }
        out.extend(field.raw);
    }
    if !replaced {
        out.extend(len_field(1, &entry_with_sentinel(&row_field)));
    }
    out
}

fn entry_with_sentinel(row_field: &[u8]) -> Vec<u8> {
    let mut entry = len_field(1, SENTINEL_KEY.as_bytes());
    entry.extend_from_slice(row_field);
    entry
}

fn is_entry(field: &Field) -> bool {
    field.number == 1 && field.wire_type == WIRE_LEN
}

fn entry_is_ours(field: &Field) -> bool {
    split(&field.payload)
        .map(|inner| string_field(&inner, 1).as_deref() == Some(SENTINEL_KEY))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// wire format
// ---------------------------------------------------------------------------

const WIRE_VARINT: u8 = 0;
const WIRE_64BIT: u8 = 1;
const WIRE_LEN: u8 = 2;
const WIRE_32BIT: u8 = 5;

/// 一个字段的全部原始字节，加上解出来的编号与内容。
#[derive(Debug, Clone)]
struct Field {
    number: u32,
    wire_type: u8,
    /// 原始字节（tag + 长度前缀 + payload）：未知字段照抄用。
    raw: Vec<u8>,
    /// length-delimited 的内容，或 varint 的数值字节。
    payload: Vec<u8>,
}

/// 换掉某个编号的全部字段，新字段追加在末尾（protobuf 不要求顺序）。
fn replace_field(data: &[u8], number: u32, replacement: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    for field in split(data).unwrap_or_default() {
        if field.number != number {
            out.extend(field.raw);
        }
    }
    out.extend(replacement);
    out
}

fn split(data: &[u8]) -> Option<Vec<Field>> {
    let mut fields = Vec::new();
    let mut offset = 0;
    while offset < data.len() {
        let start = offset;
        let (tag, after_tag) = read_varint(data, offset)?;
        let wire_type = (tag & 7) as u8;
        let number = u32::try_from(tag >> 3).ok()?;
        let (payload, end) = match wire_type {
            WIRE_VARINT => {
                let (_, end) = read_varint(data, after_tag)?;
                (data.get(after_tag..end)?.to_vec(), end)
            }
            WIRE_64BIT => (data.get(after_tag..after_tag + 8)?.to_vec(), after_tag + 8),
            WIRE_LEN => {
                let (length, content) = read_varint(data, after_tag)?;
                let length = usize::try_from(length).ok()?;
                let end = content.checked_add(length)?;
                (data.get(content..end)?.to_vec(), end)
            }
            WIRE_32BIT => (data.get(after_tag..after_tag + 4)?.to_vec(), after_tag + 4),
            _ => return None,
        };
        fields.push(Field {
            number,
            wire_type,
            raw: data.get(start..end)?.to_vec(),
            payload,
        });
        offset = end;
    }
    Some(fields)
}

fn string_field(fields: &[Field], number: u32) -> Option<String> {
    let field = fields
        .iter()
        .find(|field| field.number == number && field.wire_type == WIRE_LEN)?;
    std::str::from_utf8(&field.payload).ok().map(str::to_string)
}

fn read_varint(data: &[u8], offset: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    let mut position = offset;
    loop {
        let byte = *data.get(position)?;
        if shift == 63 && byte > 1 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        position += 1;
        if byte & 0x80 == 0 {
            return Some((value, position));
        }
        shift += 7;
    }
}

fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 0x80 {
        bytes.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}

fn len_field(number: u32, payload: &[u8]) -> Vec<u8> {
    let mut field = varint(u64::from(number) << 3 | u64::from(WIRE_LEN));
    field.extend(varint(payload.len() as u64));
    field.extend_from_slice(payload);
    field
}

fn varint_field(number: u32, value: u64) -> Vec<u8> {
    let mut field = varint(u64::from(number) << 3 | u64::from(WIRE_VARINT));
    field.extend(varint(value));
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projection<'a>(access: &'a str, refresh: &'a str, email: Option<&'a str>) -> Projection<'a> {
        Projection {
            access_token: access,
            refresh_token: refresh,
            expires_at: 1_700_000_000,
            email,
        }
    }

    /// 一份客户端自己写的 blob：每层都带一个未知字段，另有一条无关 entry。
    fn client_blob() -> Vec<u8> {
        let info = [
            len_field(1, b"old-access"),
            len_field(2, b"Bearer"),
            len_field(3, b"old-refresh"),
            project_timestamp(&[], 100),
            len_field(5, b"old@example.com"),
            len_field(90, b"client-private"),
        ]
        .concat();
        let row = [
            len_field(1, STANDARD.encode(&info).as_bytes()),
            len_field(90, b"client-private"),
        ]
        .concat();
        let entry = [
            len_field(1, SENTINEL_KEY.as_bytes()),
            len_field(2, &row),
            len_field(90, b"client-private"),
        ]
        .concat();
        let other = [len_field(1, b"userStatusSentinelKey"), len_field(2, b"opaque")].concat();
        [
            len_field(1, &other),
            len_field(1, &entry),
            len_field(90, b"client-private"),
        ]
        .concat()
    }

    fn payload_of(fields: &[Field], number: u32) -> Option<Vec<u8>> {
        fields
            .iter()
            .find(|field| field.number == number)
            .map(|field| field.payload.clone())
    }

    #[test]
    fn a_fresh_blob_round_trips() {
        let blob = project(None, &projection("at-1", "rt-1", Some("dev@example.com")));
        let session = read(&blob).expect("session");
        assert_eq!(session.access_token, "at-1");
        assert_eq!(session.refresh_token, "rt-1");
        assert_eq!(session.expires_at, Some(1_700_000_000));
        assert_eq!(session.email.as_deref(), Some("dev@example.com"));
    }

    #[test]
    fn the_info_carries_the_bearer_type_and_the_timestamp() {
        let blob = project(None, &projection("at-1", "rt-1", None));
        let info = split(&sentinel_info(&blob).expect("info")).expect("info fields");
        assert_eq!(string_field(&info, 2).as_deref(), Some("Bearer"));
        let timestamp = payload_of(&info, 4)
            .and_then(|payload| split(&payload))
            .expect("timestamp");
        assert_eq!(
            payload_of(&timestamp, 1),
            Some(varint(1_700_000_000)),
            "seconds as a varint"
        );
        assert_eq!(payload_of(&timestamp, 2), Some(varint(0)), "nanos are zero");
        assert_eq!(string_field(&info, 5), None, "no email was written");
    }

    #[test]
    fn a_projection_keeps_unknown_fields_and_other_entries() {
        let existing = client_blob();
        let blob = project(
            Some(&existing),
            &projection("new-access", "new-refresh", None),
        );

        let session = read(&blob).expect("session");
        assert_eq!(session.access_token, "new-access");
        assert_eq!(session.refresh_token, "new-refresh");
        assert_eq!(session.expires_at, Some(1_700_000_000));
        assert_eq!(
            session.email.as_deref(),
            Some("old@example.com"),
            "an absent email keeps what the client had"
        );

        // The unrelated entry stays byte-for-byte first.
        let other = [len_field(1, b"userStatusSentinelKey"), len_field(2, b"opaque")].concat();
        assert!(blob.starts_with(&len_field(1, &other)), "the other topic entry is gone");

        let topic = split(&blob).expect("topic");
        assert_eq!(
            payload_of(&topic, 90).as_deref(),
            Some(&b"client-private"[..]),
            "an unknown topic field was dropped"
        );
        let entry = split(&sentinel_row(&blob).expect("row")).expect("row fields");
        assert_eq!(
            payload_of(&entry, 90).as_deref(),
            Some(&b"client-private"[..]),
            "an unknown row field was dropped"
        );
        let info = split(&sentinel_info(&blob).expect("info")).expect("info fields");
        assert_eq!(
            payload_of(&info, 90).as_deref(),
            Some(&b"client-private"[..]),
            "an unknown info field was dropped"
        );
        // The sentinel entry itself is still the only one we touched.
        let entries: Vec<&Field> = topic.iter().filter(|field| is_entry(field)).collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn an_email_projection_replaces_the_stored_one() {
        let blob = project(
            Some(&client_blob()),
            &projection("new-access", "new-refresh", Some("new@example.com")),
        );
        assert_eq!(
            read(&blob).expect("session").email.as_deref(),
            Some("new@example.com")
        );
    }

    #[test]
    fn a_client_blob_without_the_sentinel_gains_one() {
        let other = [len_field(1, b"userStatusSentinelKey"), len_field(2, b"opaque")].concat();
        let existing = len_field(1, &other);
        let blob = project(Some(&existing), &projection("at-2", "rt-2", None));
        assert_eq!(read(&blob).expect("session").access_token, "at-2");
        assert!(blob.starts_with(&existing), "the existing topic entry is gone");
    }

    #[test]
    fn garbage_is_neither_readable_nor_projected() {
        let garbage = [0x80_u8; 20];
        assert!(read(&garbage).is_none());
        assert!(read(b"").is_none());
        // A malformed value is replaced by a clean one instead of failing the switch.
        let blob = project(Some(&garbage), &projection("at-3", "rt-3", None));
        assert_eq!(read(&blob).expect("session").access_token, "at-3");
    }

    #[test]
    fn an_info_without_both_tokens_is_not_a_session() {
        let access_only = [len_field(1, b"at-1"), len_field(3, b"")].concat();
        let row = len_field(1, STANDARD.encode(&access_only).as_bytes());
        let entry = [len_field(1, SENTINEL_KEY.as_bytes()), len_field(2, &row)].concat();
        let blob = len_field(1, &entry);
        assert!(read(&blob).is_none(), "an empty refresh token is not a session");

        let unknown_sentinel = {
            let row = len_field(1, STANDARD.encode(&access_only).as_bytes());
            let entry = [len_field(1, b"anotherKey"), len_field(2, &row)].concat();
            len_field(1, &entry)
        };
        assert!(read(&unknown_sentinel).is_none());
    }
}
