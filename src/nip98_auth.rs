use super::utils::time_delta::TimeDelta;
use base64::prelude::*;
use nostr::bitcoin::hashes::sha256::Hash as Sha256Hash;
use nostr::bitcoin::hashes::Hash;
use nostr::util::hex;
use serde_json::Value;

pub fn nip98_verify_auth_header(
    auth_header: &str,
    url: &str,
    method: &str,
    body: &Option<Vec<u8>>,
) -> Result<nostr::Event, &'static str> {
    if auth_header.is_empty() {
        return Err("Nostr authorization header missing");
    }

    let auth_header_parts: Vec<&str> = auth_header.split_whitespace().collect();
    if auth_header_parts.len() != 2 {
        return Err("Nostr authorization header does not have 2 parts");
    }

    if auth_header_parts[0] != "Nostr" {
        return Err("Nostr authorization header does not start with `Nostr`");
    }

    let base64_encoded_note = auth_header_parts[1];
    if base64_encoded_note.is_empty() {
        return Err("Nostr authorization header does not have a base64 encoded note");
    }

    let decoded_note_json = BASE64_STANDARD
        .decode(base64_encoded_note.as_bytes())
        .map_err(|_| "Failed to decode base64 encoded note from Nostr authorization header")?;

    let note_value: Value = serde_json::from_slice(&decoded_note_json)
        .map_err(|_| "Could not parse JSON note from authorization header")?;

    let note: nostr::Event =
        nostr::Event::from_value(note_value).map_err(|_| "Could not parse Nostr note from JSON")?;

    if note.kind != nostr::Kind::HttpAuth {
        return Err("Nostr note kind in authorization header is incorrect");
    }

    let authorized_url = note
        .get_tag_content(nostr::TagKind::SingleLetter(
            nostr::SingleLetterTag::lowercase(nostr::Alphabet::U),
        ))
        .ok_or_else(|| "Missing 'u' tag from Nostr authorization header")?;

    let authorized_method = note
        .get_tag_content(nostr::TagKind::Method)
        .ok_or_else(|| "Missing 'method' tag from Nostr authorization header")?;

    if authorized_url != url || authorized_method != method {
        log::warn!(
            "Auth mismatch: method: {}<>{}, url: {}<>{}",
            authorized_method,
            method,
            authorized_url,
            url
        );
        return Err("Auth note url and/or method does not match request");
    }

    let current_time: nostr::Timestamp = nostr::Timestamp::now();
    let note_created_at: nostr::Timestamp = note.created_at();
    let time_delta = TimeDelta::subtracting(current_time, note_created_at);
    if (time_delta.negative && time_delta.delta_abs_seconds > 30)
        || (!time_delta.negative && time_delta.delta_abs_seconds > 60)
    {
        log::warn!(
            "Auth timestamp out of range: Time delta: {} seconds",
            time_delta
        );
        return Err("Auth timestamp is out of range");
    }

    if let Some(body_data) = body {
        let authorized_content_hash_bytes: Vec<u8> = hex::decode(
            note.get_tag_content(nostr::TagKind::Payload)
                .ok_or("Missing 'payload' tag from Nostr authorization header")?,
        )
        .map_err(|_| "Failed to decode hex encoded payload from Nostr authorization header")?;

        let authorized_content_hash: Sha256Hash =
            Sha256Hash::from_slice(&authorized_content_hash_bytes)
                .map_err(|_| "Failed to convert hex encoded payload to Sha256Hash")?;

        let body_hash = Sha256Hash::hash(body_data);
        if authorized_content_hash != body_hash {
            return Err("Auth note payload hash does not match request body hash");
        }
    } else {
        let authorized_content_hash_string = note.get_tag_content(nostr::TagKind::Payload);
        if authorized_content_hash_string.is_some() {
            return Err("Auth note has payload tag but request has no body");
        }
    }

    // Verify both the Event ID and the cryptographic signature
    if note.verify().is_err() {
        return Err("Auth note id or signature is invalid");
    }

    Ok(note)
}
