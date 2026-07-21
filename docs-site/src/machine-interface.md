# Machine Interface

The scorekit CLI **is** the SDK. Every command completes in a single invocation, reports errors as structured JSON, and exports its schemas on demand. Third parties who want to build servers, MCP tools, CI bots, or editor plugins on top of scorekit should wrap the CLI as a subprocess — there is no separate library to link, and none is needed: audio rendering already spawns external processes (FluidSynth, FFmpeg, sfizz), so a linked library would save nothing while losing process isolation.

## Stability contract

The machine interface follows semantic versioning. Within a major version, the following are stable and safe to program against:

- Command names and documented flags in the [Command Reference](commands.md).
- The scene DSL itself, specified normatively in [Scene Protocol](scene-protocol.md) — field semantics, additive-only evolution, and byte-identical compilation for existing scenes.
- Exit codes: `0` success · `1` I/O failure · `2` invalid input · `3` missing dependency · `4` external tool failure.
- The `--json` error object shape on stderr (below).
- The JSON Schemas exported by `scorekit schema`, `schema --grammar`, and `schema --profile` (fields are added, not removed or repurposed).
- The `meta.json` / `report.json` artifact fields.

Determinism boundary: the same scene + same sound source + same tool versions produces byte-identical MIDI, and audio identical within documented tolerances. Reproducibility across *different* FluidSynth/FFmpeg versions is explicitly **not** promised — pin your toolchain (e.g. in a container image) if you need cross-machine identical audio.

## Structured errors (`--json`)

With the global `--json` flag, every failure prints exactly one JSON object to **stderr** and exits non-zero:

```json
{
  "code": "validation",
  "message": "invalid value at `tracks[0].role`: unknown role `bass2`",
  "location": null,
  "field": "tracks[0].role",
  "exit_code": 2
}
```

- `code` — stable machine tag: `io`, `parse`, `validation`, `lint`, `profile_check`, `doctor`, `missing_dependency`, `tool_failure`.
- `location` — `{ "line": N, "column": N }` for parse errors, else `null`.
- `field` — the offending DSL field path for validation errors, else `null`.
- `lint` errors carry a `violations` array; `profile_check` and `doctor` carry a full `report` object instead of `location`/`field`.

Successful diagnostic commands (`doctor`, `profile check`, `diff`, `batch` reports) write JSON to **stdout**.

## Wrapping the CLI

Everything an integration needs is a subprocess call plus JSON parsing. Python:

```python
import json, subprocess

def scorekit(*args):
    p = subprocess.run(["scorekit", "--json", *args],
                       capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(json.loads(p.stderr.splitlines()[-1]))
    return p.stdout

scorekit("validate", "scene.yaml")
scorekit("build", "scene.yaml", "-o", "out/scene.ogg", "--stems")
meta = json.load(open("out/scene.meta.json"))
```

Node.js:

```js
const { execFileSync } = require("node:child_process");

function scorekit(...args) {
  try {
    return execFileSync("scorekit", ["--json", ...args], { encoding: "utf8" });
  } catch (e) {
    throw JSON.parse(e.stderr.trim().split("\n").pop());
  }
}

const schema = JSON.parse(scorekit("schema")); // feed to an LLM / form generator
scorekit("validate", "scene.yaml");
```

This is the complete recipe for a *custom* MCP server or any other integration: expose one tool per command, shell out, and pass the structured error through to the model. The error `field`/`location` data is designed so an Agent can repair a scene from the message alone.

## Built-in MCP server (`scorekit mcp`)

For the common case you don't need to write the wrapper at all:

```bash
scorekit mcp        # MCP over stdio (newline-delimited JSON-RPC 2.0)
```

`scorekit mcp` exposes `doctor`, `validate`, `schema`, `lint`, `build`, and `diff` as MCP tools. It is a pure protocol adapter: each tool call re-invokes the scorekit binary with `--json`, and the structured stdout/stderr is passed through verbatim as the tool result (`isError: true` carries the exact error object shown above). No HTTP, no auth, no resident state — determinism is untouched, and MCP clients get the same contract as subprocess callers.

Example client registration (Claude Desktop / any MCP client):

```json
{
  "mcpServers": {
    "scorekit": { "command": "scorekit", "args": ["mcp"] }
  }
}
```

## Deployment

For cloud or CI use, the repository root ships a `Dockerfile` that pins scorekit, FluidSynth, FFmpeg, and the SHA-256-verified default SoundFont. Multi-arch images (linux/amd64, linux/arm64) are published on every release to Docker Hub ([`talkincode/scorekit`](https://hub.docker.com/r/talkincode/scorekit)) and GHCR (`ghcr.io/talkincode/scorekit`):

```bash
docker pull talkincode/scorekit          # or ghcr.io/talkincode/scorekit
docker run --rm -v "$PWD:/work" -w /work talkincode/scorekit build scene.yaml -o scene.ogg --stems
docker run --rm -i talkincode/scorekit mcp   # stdio MCP server in the pinned toolchain
docker build -t scorekit .               # or build the image locally
```

Pinning is what carries the determinism guarantee across machines. If you need an HTTP API, put your own thin gateway in front of the image — that layer belongs to the deployer, not the compiler.

## What scorekit will not ship

- **No embedded HTTP/API server.** scorekit stays a thin, single-invocation compiler; a resident service belongs to whoever deploys it and is a 20-line gateway in front of this interface. (`scorekit mcp` does not contradict this: stdio, one client, no state.)
- **No Rust library crate for now.** Splitting a public `scorekit-core` crate is on hold until a real third-party consumer demonstrates a need that a subprocess cannot serve (for example, direct access to the Score IR). See the roadmap's "Direction & intent (on hold, pending evidence)" section.
