//! `scorekit mcp` — a thin stdio MCP (Model Context Protocol) server.
//!
//! Speaks newline-delimited JSON-RPC 2.0 on stdin/stdout and exposes a
//! curated set of tools, each of which shells out to this same binary with
//! the global `--json` flag and passes the structured output through
//! untouched. Zero HTTP, zero auth, zero resident state: the CLI remains the
//! single machine interface (see docs-site/src/machine-interface.md), and the
//! MCP layer is protocol adaptation only.

use crate::error::{Error, Result};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::process::Command;

const PROTOCOL_VERSION: &str = "2024-11-05";

struct Tool {
    name: &'static str,
    description: &'static str,
    schema: Value,
}

fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "doctor",
            description: "Check platform support and external audio dependencies \
                          (FFmpeg, render backends, default SoundFont).",
            schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        Tool {
            name: "validate",
            description: "Validate a scene YAML file (syntax and semantics). \
                          Errors carry a line/column or a field path.",
            schema: json!({
                "type": "object",
                "properties": {
                    "scene": { "type": "string", "description": "Path to the scene YAML file" }
                },
                "required": ["scene"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "schema",
            description: "Print the JSON Schema of the scene DSL, grammar profile, \
                          renderer profile, texture-source profile, or resolver config.",
            schema: json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["scene", "grammar", "profile", "texture_profile", "resolver"],
                        "description": "Which schema to print (default: scene)"
                    }
                },
                "additionalProperties": false
            }),
        },
        Tool {
            name: "lint",
            description: "Check a scene against an aesthetic grammar profile; violations \
                          report measured values from the compiled score.",
            schema: json!({
                "type": "object",
                "properties": {
                    "scene": { "type": "string", "description": "Path to the scene YAML file" },
                    "grammar": { "type": "string", "description": "Path to the grammar profile YAML" }
                },
                "required": ["scene", "grammar"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "build",
            description: "Full pipeline: scene YAML -> deterministic MIDI -> rendered audio -> \
                          sample-exact loop/stems + meta.json.",
            schema: json!({
                "type": "object",
                "properties": {
                    "scene": { "type": "string", "description": "Path to the scene YAML file" },
                    "output": { "type": "string", "description": "Output audio path (.ogg or .wav)" },
                    "renderer": {
                        "type": "string",
                        "enum": ["fluidsynth", "timidity", "sfizz"],
                        "description": "Synthesizer backend (default: fluidsynth)"
                    },
                    "soundfont": { "type": "string", "description": "SF2 SoundFont path (fluidsynth/timidity)" },
                    "profile": { "type": "string", "description": "Renderer profile path (sfizz only)" },
                    "texture_profile": { "type": "string", "description": "Texture-source profile path when the scene declares textures" },
                    "stems": { "type": "boolean", "description": "Also render sample-aligned instrument and texture stems" }
                },
                "required": ["scene", "output"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "inspect_instruments",
            description: "Resolve every track's instrument against a renderer profile's \
                          availability and report exact/alias/fallback/missing status, \
                          substitution scores, reasons, and the missing-instrument list.",
            schema: json!({
                "type": "object",
                "properties": {
                    "scene": { "type": "string", "description": "Path to the scene YAML file" },
                    "profile": { "type": "string", "description": "Renderer profile defining availability (omit for the full General MIDI vocabulary)" },
                    "resolver": { "type": "string", "description": "Resolver configuration YAML" },
                    "fallback_mode": {
                        "type": "string",
                        "enum": ["strict", "conservative", "flexible"],
                        "description": "Fallback mode (default: conservative)"
                    },
                    "verbose": { "type": "boolean", "description": "Include the full scored candidate list per track" }
                },
                "required": ["scene"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "diff",
            description: "Semantic diff of two scene files (musical meaning, not text).",
            schema: json!({
                "type": "object",
                "properties": {
                    "old": { "type": "string", "description": "Path to the old scene YAML" },
                    "new": { "type": "string", "description": "Path to the new scene YAML" }
                },
                "required": ["old", "new"],
                "additionalProperties": false
            }),
        },
    ]
}

fn required_str(args: &Value, key: &str) -> std::result::Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing required string argument `{key}`"))
}

/// Translate one MCP tool call into CLI argv (without the leading `--json`).
fn tool_argv(name: &str, args: &Value) -> std::result::Result<Vec<String>, String> {
    let mut argv: Vec<String> = Vec::new();
    match name {
        "doctor" => argv.push("doctor".into()),
        "validate" => {
            argv.push("validate".into());
            argv.push(required_str(args, "scene")?);
        }
        "schema" => {
            argv.push("schema".into());
            match args.get("kind").and_then(Value::as_str) {
                None | Some("scene") => {}
                Some("grammar") => argv.push("--grammar".into()),
                Some("profile") => argv.push("--profile".into()),
                Some("texture_profile") => argv.push("--texture-profile".into()),
                Some("resolver") => argv.push("--resolver".into()),
                Some(other) => return Err(format!("unknown schema kind `{other}`")),
            }
        }
        "lint" => {
            argv.push("lint".into());
            argv.push(required_str(args, "scene")?);
            argv.push("--grammar".into());
            argv.push(required_str(args, "grammar")?);
        }
        "build" => {
            argv.push("build".into());
            argv.push(required_str(args, "scene")?);
            argv.push("-o".into());
            argv.push(required_str(args, "output")?);
            if let Some(renderer) = args.get("renderer").and_then(Value::as_str) {
                argv.push("--renderer".into());
                argv.push(renderer.to_owned());
            }
            if let Some(soundfont) = args.get("soundfont").and_then(Value::as_str) {
                argv.push("--soundfont".into());
                argv.push(soundfont.to_owned());
            }
            if let Some(profile) = args.get("profile").and_then(Value::as_str) {
                argv.push("--profile".into());
                argv.push(profile.to_owned());
            }
            if let Some(profile) = args.get("texture_profile").and_then(Value::as_str) {
                argv.push("--texture-profile".into());
                argv.push(profile.to_owned());
            }
            if args.get("stems").and_then(Value::as_bool) == Some(true) {
                argv.push("--stems".into());
            }
        }
        "diff" => {
            argv.push("diff".into());
            argv.push(required_str(args, "old")?);
            argv.push(required_str(args, "new")?);
        }
        "inspect_instruments" => {
            argv.push("inspect-instruments".into());
            argv.push(required_str(args, "scene")?);
            if let Some(profile) = args.get("profile").and_then(Value::as_str) {
                argv.push("--profile".into());
                argv.push(profile.to_owned());
            }
            if let Some(resolver) = args.get("resolver").and_then(Value::as_str) {
                argv.push("--resolver".into());
                argv.push(resolver.to_owned());
            }
            if let Some(mode) = args.get("fallback_mode").and_then(Value::as_str) {
                argv.push("--fallback-mode".into());
                argv.push(mode.to_owned());
            }
            if args.get("verbose").and_then(Value::as_bool) == Some(true) {
                argv.push("--verbose".into());
            }
        }
        other => return Err(format!("unknown tool `{other}`")),
    }
    Ok(argv)
}

/// Run one tool by re-invoking this binary with `--json` and passing the
/// structured stdout/stderr through as the MCP tool result.
fn call_tool(name: &str, args: &Value) -> std::result::Result<(String, bool), String> {
    let argv = tool_argv(name, args)?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate scorekit binary: {e}"))?;
    let out = Command::new(exe)
        .arg("--json")
        .args(&argv)
        .output()
        .map_err(|e| format!("cannot spawn scorekit: {e}"))?;
    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        Ok((
            if stdout.is_empty() {
                "ok".into()
            } else {
                stdout
            },
            false,
        ))
    } else {
        // The last stderr line is the single structured `--json` error object.
        let stderr = String::from_utf8_lossy(&out.stderr);
        let payload = stderr.lines().last().unwrap_or("").trim().to_owned();
        Ok((
            if payload.is_empty() {
                format!("scorekit exited with {}", out.status)
            } else {
                payload
            },
            true,
        ))
    }
}

fn response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    // Notifications (no id) never get a response.
    let id = id?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => response(
            id,
            json!({
                "protocolVersion": params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(PROTOCOL_VERSION),
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "scorekit",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        ),
        "ping" => response(id, json!({})),
        "tools/list" => {
            let list: Vec<Value> = tools()
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.schema,
                    })
                })
                .collect();
            response(id, json!({ "tools": list }))
        }
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args) {
                Ok((text, is_error)) => response(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": is_error,
                    }),
                ),
                Err(message) => error_response(id, -32602, &message),
            }
        }
        _ => error_response(id, -32601, &format!("method `{method}` not found")),
    })
}

/// Serve MCP over stdio until stdin closes.
pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|source| Error::Io {
            path: "<stdin>".to_owned(),
            source,
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg),
            Err(e) => Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            )),
        };
        if let Some(reply) = reply {
            let out = format!("{reply}\n");
            stdout
                .write_all(out.as_bytes())
                .and_then(|()| stdout.flush())
                .map_err(|source| Error::Io {
                    path: "<stdout>".to_owned(),
                    source,
                })?;
        }
    }
    Ok(())
}
