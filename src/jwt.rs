use std::io::{Read, Write};

use anyhow::{Context, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use comfy_table::{
    Attribute, Cell, Color, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL_CONDENSED,
};
use jiff::Timestamp;
use serde_json::{Map, Value};

use crate::AnyResult;

const MAX_TOKEN_SIZE: u64 = 1024 * 1024;

/// 解码 JWT 的 header 与 claims，并明确提示签名未经验证。
pub fn inspect<R: Read, W: Write>(reader: R, mut writer: W) -> AnyResult {
    let mut input = String::new();
    reader
        .take(MAX_TOKEN_SIZE + 1)
        .read_to_string(&mut input)
        .context("failed to read JWT")?;
    if input.len() as u64 > MAX_TOKEN_SIZE {
        bail!("JWT is too large: expected at most {MAX_TOKEN_SIZE} bytes");
    }

    let token = input.trim();
    if token.is_empty() {
        bail!("JWT is empty");
    }
    let segments = token.split('.').collect::<Vec<_>>();
    if segments.len() != 3 {
        bail!(
            "invalid JWT: expected 3 dot-separated segments, got {}",
            segments.len()
        );
    }

    let header = decode_json_object(segments[0], "header")?;
    let claims = decode_json_object(segments[1], "claims")?;

    let status_rows = vec![(
        Cell::new("Signature"),
        Cell::new("NOT VERIFIED")
            .fg(Color::Yellow)
            .add_attribute(Attribute::Bold),
    )];
    write_table(&mut writer, "JWT", status_rows)?;

    let header_rows = display_rows(
        &header,
        &[("alg", "Algorithm"), ("typ", "Type"), ("kid", "Key ID")],
    );
    write_table(&mut writer, "Header", header_rows)?;

    let claim_rows = display_claim_rows(&claims);
    write_table(&mut writer, "Claims", claim_rows)?;
    Ok(())
}

fn decode_json_object(segment: &str, name: &str) -> AnyResult<Map<String, Value>> {
    let bytes = URL_SAFE_NO_PAD
        .decode(segment)
        .with_context(|| format!("invalid JWT {name}: expected unpadded Base64URL"))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid JWT {name}: expected UTF-8 JSON"))?;
    match value {
        Value::Object(object) => Ok(object),
        _ => bail!("invalid JWT {name}: expected a JSON object"),
    }
}

fn display_rows(object: &Map<String, Value>, known: &[(&str, &str)]) -> Vec<(Cell, Cell)> {
    let mut rows = Vec::new();
    for (name, label) in known {
        if let Some(value) = object.get(*name) {
            rows.push((Cell::new(*label), Cell::new(display_value(value))));
        }
    }
    for (name, value) in object {
        if !known.iter().any(|(known_name, _)| known_name == name) {
            rows.push((Cell::new(name), Cell::new(display_value(value))));
        }
    }
    rows
}

fn display_claim_rows(claims: &Map<String, Value>) -> Vec<(Cell, Cell)> {
    const KNOWN: &[(&str, &str)] = &[
        ("iss", "Issuer"),
        ("sub", "Subject"),
        ("aud", "Audience"),
        ("iat", "Issued at"),
        ("nbf", "Not before"),
        ("exp", "Expires at"),
        ("jti", "JWT ID"),
    ];
    let mut rows = Vec::new();
    for (name, label) in KNOWN {
        if let Some(value) = claims.get(*name) {
            let value = match *name {
                "iat" | "nbf" | "exp" => display_numeric_date(value),
                _ => display_value(value),
            };
            rows.push((Cell::new(*label), Cell::new(value)));
        }
    }
    for (name, value) in claims {
        if !KNOWN.iter().any(|(known_name, _)| known_name == name) {
            rows.push((Cell::new(name), Cell::new(display_value(value))));
        }
    }
    rows
}

fn display_numeric_date(value: &Value) -> String {
    let Some(number) = value.as_number() else {
        return display_value(value);
    };
    let Some(seconds) = number.as_i64() else {
        return number.to_string();
    };
    match Timestamp::new(seconds, 0) {
        Ok(timestamp) => format!("{timestamp}  ({seconds})"),
        Err(_) => seconds.to_string(),
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(values) if values.iter().all(Value::is_string) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        value => serde_json::to_string_pretty(value).expect("serializing a JSON value cannot fail"),
    }
}

fn write_table<W: Write>(writer: &mut W, title: &str, rows: Vec<(Cell, Cell)>) -> AnyResult {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new(title).add_attribute(Attribute::Bold),
            Cell::new("Value").add_attribute(Attribute::Bold),
        ]);
    for (label, value) in rows {
        table.add_row(vec![label, value]);
    }
    writeln!(writer, "{table}")?;
    writeln!(writer)?;
    Ok(())
}
