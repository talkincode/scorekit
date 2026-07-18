# Command Reference

All commands accept the global `--json` flag. Successful diagnostic commands write JSON to stdout; errors write one JSON object to stderr.

| Command | Purpose |
| --- | --- |
| `doctor` | Probe the platform, architecture, FFmpeg, and render backends |
| `validate <scene>` | Validate scene syntax and semantics |
| `schema` | Print the scene JSON Schema |
| `schema --grammar` | Print the grammar-profile schema |
| `schema --profile` | Print the renderer-profile schema |
| `lint <scene> --grammar <file>` | Check compiled music against measurable style rules |
| `midi <scene> -o <file>` | Compile deterministic Standard MIDI |
| `render <midi> -o <wav>` | Render one MIDI file through a selected backend |
| `export <audio> -o <file>` | Convert or trim audio through FFmpeg |
| `build <scene> -o <file>` | Run the complete asset pipeline |
| `profile check <profile>` | Render probes through every mapped SFZ patch |
| `diff <old> <new>` | Compare scene semantics |
| `batch <scenes...> --out-dir <dir>` | Build several scenes and write a JSON report |
| `mcp` | Serve MCP (Model Context Protocol) over stdio; each tool wraps one CLI command |

Exit codes are stable: `0` success, `1` I/O failure, `2` invalid input, `3` missing dependency, and `4` external tool failure.

Run `scorekit <command> --help` for the complete flag list shipped by the installed binary.
