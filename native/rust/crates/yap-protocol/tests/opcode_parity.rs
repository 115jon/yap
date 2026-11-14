//! Keeps the Rust opcode and close-code enums in step with
//! `packages/protocol/src/opcodes.ts`, which is read at test time.

use std::collections::BTreeMap;
use std::path::PathBuf;

use yap_protocol::{GatewayCloseCode, GatewayOp, HEARTBEAT_FRAME};

fn opcodes_ts() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/src/opcodes.ts");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Drops `/* ... */` and `// ...` comments.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        } else if let Some(after) = rest.strip_prefix("//") {
            rest = after.split_once('\n').map_or("", |(_, tail)| tail);
            out.push('\n');
        } else {
            let mut chars = rest.chars();
            out.extend(chars.next());
            rest = chars.as_str();
        }
    }
    out
}

/// The `Name = N` pairs of `export enum <enum_name> { ... }`.
fn ts_enum(source: &str, enum_name: &str) -> BTreeMap<String, i64> {
    let header = format!("export enum {enum_name} {{");
    let start = source
        .find(&header)
        .unwrap_or_else(|| panic!("enum {enum_name} not found in opcodes.ts"))
        + header.len();
    let body = &source[start..];
    let body = &body[..body.find('}').expect("enum is closed")];
    body.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (name, value) = entry
                .split_once('=')
                .unwrap_or_else(|| panic!("unexpected enum entry {entry:?}"));
            let value = value
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("non-numeric value in {entry:?}"));
            (name.trim().to_owned(), value)
        })
        .collect()
}

fn assert_matches(ts: &BTreeMap<String, i64>, rust: BTreeMap<String, i64>, label: &str) {
    assert!(!ts.is_empty(), "{label}: parsed no TypeScript variants");
    for (name, value) in ts {
        assert_eq!(
            rust.get(name),
            Some(value),
            "{label}::{name} = {value} is missing or different in Rust"
        );
    }
    for (name, value) in &rust {
        assert_eq!(
            ts.get(name),
            Some(value),
            "{label}::{name} = {value} exists only in Rust"
        );
    }
}

#[test]
fn gateway_op_should_match_typescript_enum() {
    let ts = ts_enum(&strip_comments(&opcodes_ts()), "GatewayOp");
    let rust = GatewayOp::ALL
        .iter()
        .map(|op| (op.name().to_owned(), i64::from(op.code())))
        .collect();
    assert_matches(&ts, rust, "GatewayOp");
}

#[test]
fn gateway_close_code_should_match_typescript_enum() {
    let ts = ts_enum(&strip_comments(&opcodes_ts()), "GatewayCloseCode");
    let rust = GatewayCloseCode::ALL
        .iter()
        .map(|code| (code.name().to_owned(), i64::from(code.code())))
        .collect();
    assert_matches(&ts, rust, "GatewayCloseCode");
}

#[test]
fn heartbeat_frame_should_match_typescript_constant() {
    let source = opcodes_ts();
    assert!(
        source.contains(&format!("HEARTBEAT_FRAME = '{HEARTBEAT_FRAME}'")),
        "HEARTBEAT_FRAME differs from opcodes.ts"
    );
}

#[test]
fn try_from_should_round_trip_every_variant() {
    for op in GatewayOp::ALL {
        assert_eq!(GatewayOp::try_from(i64::from(op.code())), Ok(*op));
    }
    for code in GatewayCloseCode::ALL {
        assert_eq!(
            GatewayCloseCode::try_from(i64::from(code.code())),
            Ok(*code)
        );
    }
}

#[test]
fn try_from_should_reject_unknown_numbers() {
    assert!(GatewayOp::try_from(14).is_err());
    assert!(GatewayOp::try_from(-1).is_err());
    assert!(GatewayCloseCode::try_from(1000).is_err());
}
