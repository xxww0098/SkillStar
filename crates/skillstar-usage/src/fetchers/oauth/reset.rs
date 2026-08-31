//! Grok consumer-billing reset-credit transport.

use chrono::Utc;

use super::{CONSUMER_UI_SERVICE_URL, UsageError, UsageResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrokResetToken {
    pub token_id: String,
    pub validity_end: i64,
}

pub async fn redeem_available_reset(access_token: &str) -> UsageResult<()> {
    let tokens = get_remaining_resets(access_token).await?;
    let token = select_reset_token(tokens, Utc::now().timestamp())
        .ok_or_else(|| UsageError::Other("Grok 当前没有可用的重置额度".into()))?;

    let request = encode_grpc_web_frame(&encode_redeem_reset_request(&token.token_id));
    let response = send_consumer_ui_request(access_token, "RedeemReset", request).await?;
    decode_redeem_reset_response(&response)?;
    Ok(())
}

/// Return the number of reset credits that can actually be redeemed now.
///
/// The provider response can contain expired tokens, so the UI must not use
/// the raw response length as the available count.
pub async fn remaining_reset_credits(access_token: &str) -> UsageResult<u32> {
    let now = Utc::now().timestamp();
    let tokens = get_remaining_resets(access_token).await?;
    Ok(tokens
        .into_iter()
        .filter(|token| !token.token_id.is_empty() && token.validity_end > now)
        .count() as u32)
}

pub fn select_reset_token(tokens: Vec<GrokResetToken>, now: i64) -> Option<GrokResetToken> {
    tokens
        .into_iter()
        .filter(|token| !token.token_id.is_empty() && token.validity_end > now)
        .min_by_key(|token| token.validity_end)
}

async fn get_remaining_resets(access_token: &str) -> UsageResult<Vec<GrokResetToken>> {
    let response = send_consumer_ui_request(
        access_token,
        "GetRemainingResets",
        encode_grpc_web_frame(&[]),
    )
    .await?;
    decode_remaining_resets_response(&response)
}

async fn send_consumer_ui_request(
    access_token: &str,
    method: &str,
    body: Vec<u8>,
) -> UsageResult<Vec<Vec<u8>>> {
    let client = crate::fetchers::http_client()?;
    let url = format!("{CONSUMER_UI_SERVICE_URL}/{method}");
    let response = client
        .post(url)
        .bearer_auth(access_token.trim())
        .header(reqwest::header::ACCEPT, "application/grpc-web+proto")
        .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
        .header("X-Grpc-Web", "1")
        .header("X-User-Agent", "connect-es/2.1.1")
        .header(reqwest::header::ORIGIN, "https://grok.com")
        .header(reqwest::header::REFERER, "https://grok.com/")
        .body(body)
        .send()
        .await
        .map_err(|error| UsageError::transport("Grok quota reset", error))?;

    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| UsageError::transport("Grok quota reset response", error))?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::http_status(
            "Grok quota reset",
            status.as_u16(),
            &String::from_utf8_lossy(&body),
        ));
    }

    decode_grpc_web_frames(&body)
}

pub fn encode_grpc_web_frame(message: &[u8]) -> Vec<u8> {
    let length = message.len() as u32;
    let mut frame = Vec::with_capacity(5 + message.len());
    frame.push(0);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(message);
    frame
}

pub fn encode_redeem_reset_request(token_id: &str) -> Vec<u8> {
    let mut message = Vec::with_capacity(2 + token_id.len());
    message.push(0x0a); // field 1, length-delimited string
    encode_varint(token_id.len() as u64, &mut message);
    message.extend_from_slice(token_id.as_bytes());
    message
}

pub fn encode_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

pub fn decode_grpc_web_frames(body: &[u8]) -> UsageResult<Vec<Vec<u8>>> {
    let mut offset = 0;
    let mut messages = Vec::new();
    while offset < body.len() {
        if body.len() - offset < 5 {
            return Err(UsageError::Fetcher(
                "Grok quota reset 返回了不完整的 gRPC-Web 帧".into(),
            ));
        }
        let flags = body[offset];
        let length = u32::from_be_bytes([
            body[offset + 1],
            body[offset + 2],
            body[offset + 3],
            body[offset + 4],
        ]) as usize;
        offset += 5;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| UsageError::Fetcher("Grok quota reset 返回帧长度溢出".into()))?;
        if end > body.len() {
            return Err(UsageError::Fetcher(
                "Grok quota reset 返回了超出边界的 gRPC-Web 帧".into(),
            ));
        }
        let payload = &body[offset..end];
        if flags & 0x80 != 0 {
            validate_grpc_web_trailers(payload)?;
        } else {
            messages.push(payload.to_vec());
        }
        offset = end;
    }
    Ok(messages)
}

fn validate_grpc_web_trailers(payload: &[u8]) -> UsageResult<()> {
    let trailers = String::from_utf8_lossy(payload);
    let status = trailers
        .lines()
        .find_map(|line| line.strip_prefix("grpc-status:"))
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(0);
    if status == 0 {
        return Ok(());
    }
    let message = trailers
        .lines()
        .find_map(|line| line.strip_prefix("grpc-message:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("provider rejected the request");
    if status == 16 {
        Err(UsageError::AuthRequired)
    } else {
        Err(UsageError::Fetcher(format!(
            "Grok quota reset gRPC 状态 {status}: {message}"
        )))
    }
}

pub fn decode_remaining_resets_response(messages: &[Vec<u8>]) -> UsageResult<Vec<GrokResetToken>> {
    decode_reset_tokens(messages)
}

fn decode_redeem_reset_response(messages: &[Vec<u8>]) -> UsageResult<Vec<GrokResetToken>> {
    decode_reset_tokens(messages)
}

fn decode_reset_tokens(messages: &[Vec<u8>]) -> UsageResult<Vec<GrokResetToken>> {
    let mut tokens = Vec::new();
    for message in messages {
        for (field, wire_type, payload) in protobuf_fields(message)? {
            if field == 1 && wire_type == 2 {
                tokens.push(decode_reset_token(&payload)?);
            }
        }
    }
    Ok(tokens)
}

fn decode_reset_token(message: &[u8]) -> UsageResult<GrokResetToken> {
    let mut token_id = String::new();
    let mut validity_end = None;
    for (field, wire_type, payload) in protobuf_fields(message)? {
        match (field, wire_type) {
            (1, 2) => {
                token_id = std::str::from_utf8(&payload)
                    .map_err(|_| UsageError::Fetcher("Grok 重置令牌 ID 不是有效文本".into()))?
                    .to_string();
            }
            (20, 2) => validity_end = Some(decode_timestamp_seconds(&payload)?),
            _ => {}
        }
    }
    Ok(GrokResetToken {
        token_id,
        validity_end: validity_end.unwrap_or_default(),
    })
}

fn decode_timestamp_seconds(message: &[u8]) -> UsageResult<i64> {
    for (field, wire_type, payload) in protobuf_fields(message)? {
        if field == 1 && wire_type == 0 {
            let mut value = 0;
            for (index, byte) in payload.iter().copied().enumerate() {
                value |= u64::from(byte & 0x7f) << (index * 7);
                if byte & 0x80 == 0 {
                    return Ok(value as i64);
                }
            }
        }
    }
    Err(UsageError::Fetcher("Grok 重置令牌缺少有效期".into()))
}

/// Return field payloads for the wire types used by the consumer billing
/// messages. Varints are returned as their original bytes because timestamps
/// need to preserve the signed int64 representation until decoded.
fn protobuf_fields(message: &[u8]) -> UsageResult<Vec<(u32, u8, Vec<u8>)>> {
    let mut fields = Vec::new();
    let mut offset = 0;
    while offset < message.len() {
        let key = read_varint(message, &mut offset)?;
        let field = (key >> 3) as u32;
        let wire_type = (key & 0x07) as u8;
        if field == 0 {
            return Err(UsageError::Fetcher(
                "Grok gRPC 返回非法 protobuf 字段".into(),
            ));
        }
        let payload = match wire_type {
            0 => {
                let start = offset;
                read_varint(message, &mut offset)?;
                message[start..offset].to_vec()
            }
            2 => {
                let length = read_varint(message, &mut offset)? as usize;
                let end = offset
                    .checked_add(length)
                    .ok_or_else(|| UsageError::Fetcher("Grok protobuf 字段长度溢出".into()))?;
                if end > message.len() {
                    return Err(UsageError::Fetcher("Grok protobuf 字段超出消息边界".into()));
                }
                let payload = message[offset..end].to_vec();
                offset = end;
                payload
            }
            1 => {
                let end = offset
                    .checked_add(8)
                    .ok_or_else(|| UsageError::Fetcher("Grok protobuf fixed64 字段溢出".into()))?;
                if end > message.len() {
                    return Err(UsageError::Fetcher(
                        "Grok protobuf fixed64 字段超出消息边界".into(),
                    ));
                }
                let payload = message[offset..end].to_vec();
                offset = end;
                payload
            }
            5 => {
                let end = offset
                    .checked_add(4)
                    .ok_or_else(|| UsageError::Fetcher("Grok protobuf fixed32 字段溢出".into()))?;
                if end > message.len() {
                    return Err(UsageError::Fetcher(
                        "Grok protobuf fixed32 字段超出消息边界".into(),
                    ));
                }
                let payload = message[offset..end].to_vec();
                offset = end;
                payload
            }
            _ => {
                return Err(UsageError::Fetcher(
                    "Grok gRPC 返回了不支持的 protobuf wire type".into(),
                ));
            }
        };
        fields.push((field, wire_type, payload));
    }
    Ok(fields)
}

fn read_varint(message: &[u8], offset: &mut usize) -> UsageResult<u64> {
    let mut value = 0u64;
    for index in 0..10 {
        let byte = *message
            .get(*offset)
            .ok_or_else(|| UsageError::Fetcher("Grok protobuf varint 不完整".into()))?;
        *offset += 1;
        if index == 9 && byte > 1 {
            return Err(UsageError::Fetcher("Grok protobuf varint 溢出".into()));
        }
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(UsageError::Fetcher("Grok protobuf varint 溢出".into()))
}
