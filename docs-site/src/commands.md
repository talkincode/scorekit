# Command Reference

All commands accept the global `--json` flag. Successful diagnostic commands write JSON to stdout; errors write one JSON object to stderr.

| Command | Purpose |
| --- | --- |
| `doctor` | Probe the platform, architecture, FFmpeg, and render backends |
| `validate <scene>` | Validate scene syntax and semantics |
| `schema` | Print the scene JSON Schema |
| `schema --grammar` | Print the grammar-profile schema |
| `schema --profile` | Print the leaf renderer-profile schema |
| `schema --orchestration` | Print the orchestration-profile schema |
| `schema --texture-profile` | Print the texture-source profile schema |
| `schema --resolver` | Print the instrument-resolver config schema |
| `lint <scene> --grammar <file>` | Check compiled music against measurable style rules |
| `midi <scene> -o <file>` | Compile deterministic Standard MIDI |
| `makecode <scene> -o <file>` | Compile MakeCode `music.createSong` TypeScript (plus a `.meta.json` manifest) |
| `render <midi> -o <wav>` | Render one MIDI file through a selected backend |
| `export <audio> -o <file>` | Convert or trim audio through FFmpeg |
| `build <scene> -o <file>` | Run the complete asset pipeline |
| `profile check <profile>` | Render probes through every mapped SFZ patch in a leaf renderer profile |
| `orchestration check <file>` | Validate palette bindings and every referenced leaf profile/SFZ file |
| `inspect-instruments <scene>` | Resolve every track's instrument and report substitutions and gaps |
| `diff <old> <new>` | Compare scene semantics |
| `batch <scenes...> --out-dir <dir>` | Build several scenes and write a JSON report |
| `mcp` | Serve MCP (Model Context Protocol) over stdio; each tool wraps one CLI command |

Exit codes are stable: `0` success, `1` I/O failure, `2` invalid input, `3` missing dependency, and `4` external tool failure.

Run `scorekit <command> --help` for the complete flag list shipped by the installed binary.

Numeric audio-command options reject non-finite or out-of-range values before
resolving tools or writing files: `--sample-rate` is 8000..=384000 Hz,
`--gain` is 0.0..=8.0, `--quality` is 0..=10, `--tail` is 0.0..=3600.0
seconds, and `--crossfade-ms` is 0..=60000.

Scenes with `textures` require `build`/`batch --texture-profile <file>`. This
flag is independent of the musical renderer: it works alongside either an
SF2 `--soundfont` or an sfizz `--orchestration` profile.

Standalone `midi` can encode melodic instruments with exact GM programs plus
channel-10 `drums`/`tabla`. It rejects profile-only melodic identities before
writing the output: the command has no renderer-profile input, and omitting a
program change would make a generic GM player select piano. Use
`build --renderer sfizz --orchestration <file>` to render those identities.

`makecode` compiles a scene to Microsoft MakeCode's synth-song format (song
encoding v0, played by MakeCode Arcade and micro:bit V2): a paste-ready `.ts`
snippet with one ``let <name> = music.createSong(hex`…`)`` statement per song
— one song per section for a suite — plus a `.meta.json` manifest reporting
the tick grid and each track's mapped MakeCode chip or drum voice. Synthesis
happens entirely in the MakeCode runtime. Chip and drum amplitudes are scaled
from track `intensity` against a ~1.2 full-scale mix budget so stacked voices
stay near the MakeCode editor's own 220–384 / 1024 range; tiny PWM speakers
clip if every voice is written at full scale. Features the format cannot
express fail with structured exit-2 errors (pitch bends/`glide`, clip CC
automation, `textures`, timing finer than 255 ticks per beat such as
`humanize`); `pan`/`reverb` are dropped with a WARN line and a manifest
record. Keep bass in a chip-safe register — sub-bass square waves distort on
Arcade and micro:bit speakers.

## Instrument resolution

`build` and `batch` resolve every track's instrument against what the
selected backend actually provides before anything is rendered. SF2 backends
resolve the 60-instrument core plus exact named extension programs
(shakuhachi, sitar, shamisen); non-GM world identities fail rather than
defaulting to piano. With
`--renderer sfizz` availability is each track's effective palette's leaf
renderer profile (routed through `--orchestration <file>`, the only sfizz
routing input for `build`/`batch`/`inspect-instruments`), and unmapped
instruments go through a scored fallback policy confined to that one
palette — resolution never crosses palettes, even when another palette in
the same orchestration happens to map the same instrument:

- `--fallback-mode conservative` (default) substitutes within the same
  instrument family only, minimum score 0.70. Missing instruments never
  silently become strings — substituting *into* strings always requires an
  explicit `allowed_families: [strings]` opt-in in the resolver config.
- `--fallback-mode strict` performs no substitution: an unmapped instrument
  fails the build (exit 2, code `resolution`) and the error names the best
  candidate it refused to use.
- `--fallback-mode flexible` also reaches related families and synth
  stand-ins.
- World instruments are exact-source-only under every mode: no fallback may
  enter, leave, or occur within that family.
- `--resolver <file>` supplies a config (see `scorekit schema --resolver`)
  with `default_mode`, `minimum_score`, `allow_cross_family`, `allow_synth`,
  `allowed_families`, and `excluded_families`.

Every substitution prints one `WARN instrument fallback:` line with its score
and reasons; `meta.json` embeds the full `instrument_resolution` report,
including each track's `id`, declared/effective `palette`, resolved leaf
profile, requested/effective articulation, and resolved SFZ path.
Unresolved instruments abort before staging, leaving no partial artifacts.

`inspect-instruments <scene> [--orchestration <file>] [--resolver <file>]
[--fallback-mode <mode>] [--verbose]` prints the same resolution standalone:
per-track status (`exact`/`alias`/`fallback`/`missing`/`rejected`), scores,
reasons, palette/profile/SFZ routing, and the missing-instrument list. It
exits 2 when instruments are unresolved, and `--verbose` lists every scored
candidate. Alias spellings (e.g. `french_horn` for `horn`) are pure surface
syntax and never change MIDI bytes.
