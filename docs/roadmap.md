# scorekit Project Profile & Direction

> **ScoreKit is an Agent-oriented Music Compiler, not an AI music generator.**
> It reliably compiles high-level musical semantics into executable assets; the creative intelligence always belongs to the upstream Agent.

## Project overview

scorekit is an **Agent-driven game score compiler**: it takes a text DSL (YAML) describing a piece's structure as input, and outputs audio assets ready to drop straight into a game (seamless loops, split stems, scene transitions). It serves two kinds of users: the AI Agent that writes the DSL, and the indie game developer who drops the output into an engine.

It is a thin Rust CLI orchestration layer — it doesn't build its own audio algorithms, it just reliably compiles down through a stable intermediate format:

```text
scene.yaml ──(single source of truth, versioned directly by git)
    │  scorekit validate / schema
    ▼
Score IR ──► scorekit midi ──► scene.mid (byte-exact deterministic)
    │
    ▼  scorekit render          ┌─ render backend boundary (swappable) ─┐
FluidSynth + SF2 ──► scene.wav  │ future: sfizz+SFZ, others             │
    │                           └─────────────────────────────────────┘
    ▼  scorekit export (invokes FFmpeg)
scene.ogg + loop metadata + stems/*.ogg (sample-aligned)
```

External dependencies: FluidSynth (rendering), FFmpeg (transcoding/post-processing), SoundFont/SFZ sound source files. The name is settled as scorekit — not up for further discussion.

## Project profile (target state)

Once finished, it is a **deterministic composition compiler**:

- **Determinism above all.** The same DSL + same sound source + same tool versions produces a reproducible result: byte-identical MIDI, audio identical within assertion tolerance. When quality trade-offs conflict, sacrifice convenience and performance before reproducibility — this is the foundation for Agent regression testing.
- **Everything is text.** The DSL is the single source of truth, line-oriented and diff-friendly. "Git for Music" (diff/merge/branch/review/rollback) is handled natively by git — it is not scorekit's job.
- **The output is a game asset, not "a song".** Loops must be sample-exact (duration = an integer number of samples for bars × beats × sample rate) with no clicks at the seam; stems must be equal length and sample-aligned; scene sections (explore/combat/victory) share thematic material.
- **Agent-friendly.** Every command completes in a single invocation; failures exit with a non-zero code plus a machine-readable error report pointing to a DSL line number; the schema can be exported via a command, so an Agent can write valid DSL from the schema and error messages alone.
- **Thin.** Rendering and post-processing are always delegated to external tools; scorekit's value is in the stability of the DSL and the reliability of the compile, not in lines of code.

## Current capability list

- **Scene DSL and validation**: YAML scenes (tempo/key/time signature/bars/loop/tracks), unknown fields rejected, semantic validation with field paths, parse errors with line numbers; `scorekit validate`, `scorekit schema` (JSON Schema export). Evidence: `src/schema.rs`, `tests/cli.rs`.
- **Deterministic MIDI compilation**: `scorekit midi` outputs a byte-stable SMF (format 1, PPQ 480), patterns: sustain/arpeggio/bass/drums, locked against golden baselines. Evidence: `src/composer.rs`, `src/midi.rs`, `tests/golden/forest.mid`.
- **Rendering and export**: `scorekit render` (FluidSynth+SF2, SF2 magic-number precheck, catches fluidsynth's silent "exit 0 but error" failures), `scorekit export` (FFmpeg → OGG, libvorbis→vorbis fallback), `scorekit build` full pipeline; every file write goes through temp-file + atomic rename, so a failure leaves no partial artifact. Evidence: `src/tools.rs`, `tests/cli.rs`.
- **Machine-readable errors**: `--json` outputs structured errors (code/message/location/field/exit_code); exit-code convention is 1 io / 2 invalid input / 3 missing dependency / 4 external tool failed. Evidence: `src/error.rs`.
- **Music grammar validation**: `scorekit lint scene.yaml --grammar grief.yaml` — an external grammar profile declares deterministic aesthetic constraints (tempo/rests/voice count/phrase length/cadence/harmony whitelist); deep rules are measured on the compiled IR, and violations report measured values; `scorekit schema --grammar` exports the profile's schema. Evidence: `src/grammar.rs`, `examples/grammars/grief.yaml`.
- **CI**: GitHub Actions (fmt+clippy+full test suite, including real fluidsynth/ffmpeg rendering). Evidence: `.github/workflows/ci.yml`.
- **Test assets**: `scripts/fetch_assets.sh` downloads TimGM6mb.sf2 into `assets/` (gitignored, not checked in).

## Non-goals (iron rules)

- **No in-house DSP.** No writing a synthesizer, reverb, compressor, or resampler; always delegated to external tools like FluidSynth/FFmpeg. Violating this means "researching signal processing until retirement."
- **No GUI / DAW / timeline editor.** scorekit is a compiler, not GarageBand.
- **No in-house version control.** No implementing `score commit/merge/branch`. Version control is git's job; scorekit may at most offer a read-only semantic diff view layered on top of git.
- **No embedded compositional intelligence.** No calling an LLM, no generative models; compositional creativity belongs to the upstream Agent, scorekit only compiles structure. Corollary: **only compile fields with deterministic semantics** — game world state (hp/danger/emotion), creative intent (`avoid: too_bright`), and abstract motifs (`shape: question`) all belong in the Agent's prompt space, not the schema; the schema must not contain "fields that don't do anything."
- **No in-game real-time audio runtime.** A runtime for real-time mixing and dynamic stem fade-in/out (including the Zig concept) is a separate project.
- **The core is not tied to commercial sound sources.** Kontakt/BBC etc. can only be integrated as adapters outside the render-backend boundary; the DSL schema must not contain fields achievable only with commercial sound sources.

## Direction & intent (roadmap)

> Milestones express target capabilities, not a mandated internal implementation order; each milestone's completion criteria are defined in "What done looks like" and the acceptance matrix.

### M0 — Walking skeleton (status: complete)

The full pipeline from DSL to a playable file for a single track: `validate → midi → render(SF2) → export(ogg)`. This serves the "determinism" and "Agent-friendly" parts of the profile — golden tests and machine-readable errors were established from day one. What actually shipped exceeded the baseline: multiple tracks, the `build` full-pipeline command, CI.

### M1 — Game asset core (status: complete)

Multiple tracks, seamless loops, sample-aligned stems, track intensity. This is the milestone that separates it from a "demo that makes sound," serving the "output is a game asset" profile. Findings: FluidSynth schedules MIDI on a millisecond clock, so the actual rendered period is ≈ L−28 samples with jitter — sample-exact periodicity is unreachable directly. The fix is a 2-pass approach taking `[L, 2L)` plus a linear crossfade seal at the tail (the final frame is bit-exact with `raw[L-1]`), so the loop point is an adjacent sample pair from the original render — seamlessness that can be verified with a bit-exact assertion. Stems are cut per-track with the same linear seal; the per-sample sum deviates from the full mix RMS by ≈0.2%. `build` also produces `<name>.meta.json` (loop_samples/total_samples/stems listing) for the game engine and Agent to consume.

### M2 — Structured composition (status: complete)

Section structure (intro/loop A/loop B/combat/victory/failure), reusable patterns/motifs, transitions between sections. Serves the game-narrative need for "scenes sharing a theme." Findings: a single scene file can declare `motifs` (scale-degree melodic motifs, referenced by tracks with `pattern: melody`) and `sections` (each section independently overrides bars/loop/tempo/mute/intensity scaling, while sharing tracks, motifs, and key); `build` on a suite produces per-section assets named `<stem>-<section>.<ext>` plus a single suite manifest (`<stem>.meta.json`); `midi --section` can compile just one section. A transition is simply a short non-loop section — no dedicated mechanism needed. Example: `examples/scenes/forest_suite.yaml`.

### M3 — Render-backend swappability proven (status: complete, audio-quality upgrade kept as future direction)

Prove the render boundary holds by adding a second render backend. Findings: TiMidity++ was chosen over sfizz as the second backend — sfizz has no ready-made package on homebrew or apt (building from source isn't feasible in CI), while TiMidity++ installs on both platforms and reuses the same SF2. `--renderer {fluidsynth|timidity}` produces the same sample-exact length with different timbre bytes for the same DSL; the seal, stems, and export are all renderer-agnostic. Along the way, new TiMidity failure modes were discovered and guarded against: with a bad SF2 it exits 0 and silently falls back to built-in timbres (detected by scanning stderr for the `***` marker), or outputs a header-only zero-frame WAV (guarded by a renderer-agnostic zero-frame fallback check). A sample-level audio-quality profile (SFZ, sfizz + free orchestral libraries) remains a future "direction and intent"; commercial sound-source adapters stay on HOLD.

### M4 — Agent experience polish (status: complete)

JSON Schema export, read-only semantic diff (`scorekit diff`, git-porcelain style), batch rendering with a machine-readable report. Serves "an Agent can work from just the schema and error messages." Findings: the `schema` command shipped back in M0; `diff` compares musical semantics rather than text (key-order/formatting/comment differences produce an empty diff), porcelain output is `~/+/- <path> <old> -> <new>`, `--json` emits an isomorphic JSON array, it's read-only and always exits 0 (a diff isn't an error); `batch` renders multiple scenes into one directory, a single scene's failure doesn't stop the rest, per-scene results (including error code/exit_code) are written to `report.json`, the process exit code reflects the first failure, and output-name collisions are rejected before the build starts.

### M5 — Performance layer and harmony declaration (status: complete)

The compiler's answer to eliminating the "AI feel." That feel doesn't come from the melody — it comes from every note's velocity/timing being constant. The cure isn't a generative model, it's **deterministic structural transformation**:

- The `performance` block: seeded humanize (velocity/timing jitter, same seed → same bytes), a dynamics arc (`dynamics`, loop scenes must have the same level at start and end to stay seamless), swing, legato. All off by default — a scene that doesn't write `performance` is byte-identical, golden tests unaffected.
- The `harmony` field: a roman-numeral chord progression (e.g. `[i, VI, III, VII]`), replacing two hardcoded internal progressions. Harmony was already a single internal source of truth (bass/pad/arpeggio all derive from `chord_for_bar`) — this stage just hands the choice to the DSL: change the chords and every track changes together.

Findings: `performance` is applied at the tail of compose, before the loop copy — the two passes stay bit-identical, and M1's `[L, 2L)` seal math is unaffected (E2E asserts the loop is still sample-exact with humanize+swing on). Humanize uses an inline LCG (Knuth MMIX constants) instead of a `rand` dependency, so it's permanently bit-identical across platforms; the dynamics arc's start→peak→start construction is loop-safe by design. Scenes that don't write `performance`/`harmony` take the original path, and the forest golden file's bytes are unchanged.

Boundary ruling (2026-07 architecture review): a five-layer proposal (Story/Composition/Arrangement/Performance/Runtime) was audited and **narrowed down to this stage**. Story→Scene translation, an Intent field, and abstract motifs (`shape: question`) were judged to belong in the Agent's prompt space — putting them in the compiler would violate the "no embedded compositional intelligence" iron rule; a role→instrument indirection layer was judged over-engineered with no consumer; event-style timelines are already covered by sections+mute+intensity semantics; the Music IR requirement is already satisfied by `ScoreIr`+SMF (an industry-standard IR natively consumed by REAPER/MuseScore/Kontakt).

### M6 — Music grammar engine (Music Grammar lint) (status: complete)

Turning "aesthetics" into checkable assertions. `scorekit lint scene.yaml --grammar grief.yaml` — a grammar profile is an external YAML data file (`scorekit schema --grammar` exports its JSON Schema) declaring a set of deterministic constraints; the compiler only validates and generates zero notes, compatible with the "no embedded compositional intelligence" iron rule.

- Rule set (all optional, at least one must be declared): `tempo_min/max`, `pads_max`, `melodic_voices_max` (peak number of simultaneously sounding melody voices), `melody_rest_ratio_min` (rest ratio **per melody track** — the union of a dialogue-style texture has almost no gaps; a voice's own breathing room is what counts as rest), `phrase_min_beats` (note spans merged into phrases at gaps of <2 beats), `resolution: complete|incomplete` (whether the final note lands on the tonic), `harmony_allowed`, `require_performance`.
- Surface rules query the Scene directly; deep rules are **measured on the compiled ScoreIr** (the actual result after pattern expansion and performance transforms, not the YAML surface). Suite scenes are checked section by section.
- Violations report measured values (`tempo_max @ scene: measured 92, want <= 60`), `--json` outputs a violations array, exit 2 — the Agent gets an actionable fix instruction, not an aesthetic adjective.
- Shipped reference pair: `examples/grammars/grief.yaml` (the grammar of loneliness: tempo≤60, pads≤1, melody voices≤2, rest ratio≥35%, phrases≥5 beats, incomplete cadence, elegy harmony whitelist, performance required) + `examples/scenes/dunes.yaml` (the final cut from a real film-scoring exercise ("East Meets West"), living proof that the constitution is satisfiable).

Value delivered: the Agent gains an **aesthetic regression test** — a style system captured as a data file that survives model changeovers; the creative feedback loop moves from "listen with human ears" to "machine assertion + final human review." Source: a 2026-07 retrospective on three rounds of real film-scoring work (the user proposed the "music grammar engine" framing).

## Direction & intent (on hold, pending evidence)
- **Spatial performance and glide fields** (per-track `pan`, `reverb` send, tail portamento): deterministic render semantics (MIDI CC10/CC91/pitch-bend), a schema field that qualifies. The need comes from real composing work (a three-part film-score exercise: "near→far→wide→pull back" spatial narrative can't be expressed today).
- **Declarative runtime manifest**: `meta.json` is already the engine's data contract; extending it to declare "state→section/stem mapping, fade durations" (the compiler only validates references, never executes them) is legitimate and valuable — but **on hold until a real engine integration** exists to drive the field design, to avoid guessing at a schema for a contract with no consumer. An executing runtime (real-time mixing) remains off-limits under the iron rules.
- **SFZ / sample-level audio-quality profile** (continued from M3): sfizz + a free orchestral library as a third render backend, serving an audio-quality upgrade; commercial sound-source adapters remain on HOLD.
- **Story-layer convention docs**: how an Agent translates game-world state into the scene DSL, captured as docs + examples (`docs/` or example comments), producing no code.
- **The Story/character field stays REJECTED on second review** (2026-07 composing-practice re-review): `character: { regret: 0.9 }` has the same problem as the Story layer — no deterministic compile semantics. Practical evidence: across three versions of the same film score under one DSL (v1 mood → v3 narrative), all the improvement in "character feel" came from the quality of the Agent's creative brief, with zero DSL changes. The correct vehicle for a character's theme is motif reuse (the same motif recurring across scenes is already a character signature, and is already supported) and brief conventions — not a schema field.

## What done looks like

> A stage is only considered achieved once all of the following observable outcomes appear; the means (test framework, directory layout) are up to whoever implements it.

- A single command chain turns `scene.yaml` into `scene.ogg`, repeatably, in a clean environment (CI).
- Compiling the same input twice in a row produces byte-identical MIDI; audio duration, sample count, and RMS match within established tolerances (golden tests catch regressions).
- A generated loop plays back seamlessly in a game engine with no audible seam; the exact length can be verified automatically via a sample-count assertion.
- The number of stems matches the DSL's tracks, each one the same length; the difference between the summed stems and the full mix is within tolerance.
- Given a schema and one error message, an Agent can fix invalid DSL without reading the source — error messages include line numbers and field paths.
- When an external dependency is missing (fluidsynth/ffmpeg/sound source file), the command exits with a clear machine-readable error and produces no partial artifact.

## Acceptance matrix (business capability coverage matrix)

> Coverage baseline (hard rules, must not be downgraded):
>
> 1. Every tier-1 feature must have at least one happy-path E2E test.
> 2. Every high-risk feature must cover at least one failure path.
> 3. Every feature involving permissions must verify at least two roles.
> 4. Every operation that mutates system state must verify recovery/rollback after at least one failure.
> 5. Every time a new tier-1 business feature is added, the corresponding E2E test must be added and this matrix updated at the same time.

Permissions note: scorekit is a local single-user CLI with no role/permission system, so the permissions column is "not applicable" throughout. "State mutation" means writing a file; its recovery requirement is: a failure must not leave a corrupted partial artifact behind (e.g. via temp file + atomic rename), and must exit with a non-zero code.

| Tier-1 feature | Risk level | Happy path E2E | Failure path | Role/permission coverage | Failure recovery/rollback | Evidence (test path/case) |
| --- | --- | --- | --- | --- | --- | --- |
| DSL validation and schema export (validate/schema) | Low | ✅ | ✅ | N/A (local CLI) | N/A (read-only) | `tests/cli.rs::validate_happy_path` / `validate_rejects_unknown_field_with_location` / `validate_rejects_semantic_error_with_field_path_json` / `schema_emits_json_schema` |
| MIDI generation (midi) | Medium | ✅ (golden byte comparison + double-run determinism) | ✅ | N/A (local CLI) | ✅ (failure leaves no partial artifact) | `tests/cli.rs::midi_matches_golden_bytes` / `midi_is_deterministic_across_runs` / `midi_invalid_scene_leaves_no_partial_file` |
| Audio rendering (render, FluidSynth+SF2) | High (external process + file write) | ✅ (asserts sample rate and duration) | ✅ (corrupt SF2 / non-SF2 file / missing fluidsynth) | N/A (local CLI) | ✅ (no leftovers in the directory on failure) | `tests/cli.rs::render_happy_path_produces_exact_rate_wav` / `render_corrupt_soundfont_fails_without_partial_output` / `render_text_file_as_soundfont_is_input_error` / `render_missing_fluidsynth_is_dependency_error` |
| Export and loop metadata (export/build, FFmpeg) | High (external process + file write) | ✅ (loop is exactly L samples + bit-exact seamless seal + meta.json + fixed length for non-loop) | ✅ | N/A (local CLI) | ✅ (no leftovers in the directory on failure) | `tests/cli.rs::export_happy_path_produces_ogg` / `export_missing_input_is_input_error` / `export_seek_take_cuts_bit_exactly` / `build_full_chain_scene_to_ogg` / `build_loop_wav_is_sample_exact_and_sealed` / `build_nonloop_wav_has_exact_padded_length` |
| Stem rendering (stems) | Medium | ✅ (4 tracks equal length + sum ≈ full-mix RMS < 2%) | ✅ (corrupt SF2) | N/A (local CLI) | ✅ (staging directory atomic swap, no leftover stems on failure) | `tests/cli.rs::build_stems_are_aligned_and_sum_to_mix` / `build_corrupt_soundfont_leaves_no_partial_output_or_stems` |
| Structured composition and transitions (sections/motif) | Medium | ✅ (suite per-section assets + exact section lengths + manifest + tempo override + mute) | ✅ (unknown motif reference / duplicate section names / all-muted section / unknown --section) | N/A (local CLI) | ✅ (no leftovers on failure, reuses build's atomic mechanism) | `tests/cli.rs::build_suite_emits_per_section_assets_with_exact_lengths` / `midi_section_selector_compiles_that_section_deterministically` / `midi_unknown_section_is_input_error` / `validate_rejects_unknown_motif_reference` / `validate_rejects_duplicate_section_names_and_mute_all` / `example_suite_validates` |
| Second render backend (--renderer, M3) | Medium | ✅ (full timidity pipeline: same exact length, different timbre, non-silent, for the same DSL) | ✅ (bad SF2 → header-only zero-frame WAV fallback → exit 4; missing SF2 → exit 2) | N/A (local CLI) | ✅ (failure deletes the partial artifact, no leftovers in the directory) | `tests/cli.rs::build_timidity_backend_same_length_different_timbre` / `render_timidity_corrupt_soundfont_fails_without_partial_output` / `render_timidity_missing_soundfont_is_input_error` |
| Semantic diff (diff, M4) | Low (read-only) | ✅ (semantic changes as porcelain + `--json` array; formatting/key-order differences produce an empty diff) | ✅ (invalid scene → exit 2) | N/A (local CLI) | N/A (read-only, no state mutation) | `tests/cli.rs::diff_reports_semantic_changes_and_ignores_formatting` / `diff_invalid_scene_is_input_error` |
| Batch rendering with machine-readable report (batch, M4) | High (external process + batch file writes) | ✅ (all scenes built + per-scene exact length + report.json stats) | ✅ (one scene's failure doesn't stop the rest, report records the error, exit code reflects the first failure; output-name collisions rejected before building) | N/A (local CLI) | ✅ (failed scenes leave no partial artifact, successful scenes' output stays intact) | `tests/cli.rs::batch_builds_all_scenes_and_writes_report` / `batch_partial_failure_reports_and_exits_nonzero` / `batch_duplicate_scene_stems_is_input_error` |
| Performance layer (performance, M5) | Medium | ✅ (same seed is byte-identical, different seed differs; loop stays sample-exact with humanize+swing) | ✅ (swing out of range → exit 2 + field path) | N/A (local CLI) | N/A (pure computation, reuses midi/build's atomic mechanism) | `tests/cli.rs::performance_same_seed_is_byte_identical_different_seed_differs` / `performance_build_keeps_loop_sample_exact` / `validate_rejects_bad_swing_and_bad_numeral` |
| Harmony progression declaration (harmony, M5) | Low | ✅ (a custom progression changes the notes but not the total length) | ✅ (invalid roman numeral → exit 2 + `harmony[i]` path) | N/A (local CLI) | N/A (pure computation) | `tests/cli.rs::harmony_changes_notes_at_same_length` / `validate_rejects_bad_swing_and_bad_numeral` |
| Music grammar validation (lint/schema --grammar, M6) | Low (read-only) | ✅ (the shipped dunes×grief reference pair passes fully; `schema --grammar` exports the profile schema) | ✅ (violations report measured values → exit 2 + `--json` violations array; deep rules measured on the compiled IR; an empty rules profile → exit 2) | N/A (local CLI) | N/A (read-only, no state mutation) | `tests/cli.rs::lint_shipped_scene_conforms_to_shipped_grammar` / `lint_reports_violations_with_measured_values` / `lint_measures_rest_ratio_from_compiled_ir` / `lint_rejects_grammar_without_rules` / `schema_grammar_flag_emits_grammar_schema` |

The matrix currently has no known gaps; when adding a new tier-1 feature, register it per coverage-baseline rule 5.
