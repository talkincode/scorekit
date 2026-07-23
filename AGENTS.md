# scorekit Agent Spec

The project profile, roadmap, and full acceptance matrix live in [docs/roadmap.md](docs/roadmap.md). Read it before making changes: the project overview explains what scorekit is, and the non-goals (iron rules) explain what it will never do.

## Core boundaries (MUST)

- MUST NOT build creative DSP in-house (synthesis, reverb, compression, EQ, resampling, time-stretch); rendering and post-processing are always delegated to external tools (FluidSynth, FFmpeg, etc.). Deterministic sample-exact PCM assembly — cutting, zero-padding, linear crossfade loop sealing, gain-applied summation (`src/audio.rs`) — is explicitly *not* DSP under this rule: it exists to uphold bit-exact invariants ("sum of stems == full mix", seamless-loop seal) that external filter pipelines cannot guarantee.
- MUST NOT implement version-control commands (commit/merge/branch); version control is git's job, and the DSL must stay a diff-friendly, stable text format.
- MUST guarantee determinism: the same DSL + same sound source + same tool versions produces byte-identical MIDI output.
- The DSL schema MUST NOT introduce fields that only commercial sound sources can fulfill.

## Sound library & anti-homogenization (MUST)

The standing program — goal, measured baseline, measurable targets (T1–T5), and operating loop — lives in the "Sound library & orchestration program" section of [docs/roadmap.md](docs/roadmap.md) (single source of truth). The following rules are MUST-level whenever touching sound sources, renderer/texture profiles, or the resolver:

- Substitution fallback is a last-resort diagnostic, never a coverage strategy. When a wanted instrument or articulation is missing from the active profile, close the gap with a real source (acquire → manifest → map → `scorekit profile check`) or re-orchestrate the scene visibly; MUST NOT widen fallback policy, lower resolver score gates, or bind an unrelated patch just to silence the WARN.
- Every library enters the corpus through a versioned identity with license + checksum manifests; every new mapping MUST pass `scorekit profile check` (deterministic, non-silent) before anything relies on it.
- Preserve timbre diversity: renderer profiles are curated sound identities; prefer adding independent sources over deepening dependence on a single library, and MUST NOT wholesale-rebind existing mappings to a different library as a side effect — that is an audible style change and must be a visible, reviewed decision.
- Coverage gaps are closed in the library/profile layer, never by bending the DSL schema toward one sound source (corollary of the schema-neutrality iron rule).

## Acceptance matrix (hard rules)

The following five rules are MUST-level; the matrix itself is maintained in the "Acceptance matrix" section of [docs/roadmap.md](docs/roadmap.md) (single source of truth — not duplicated here):

1. Every tier-1 feature must have at least one happy-path E2E test.
2. Every high-risk feature must cover at least one failure path.
3. Every feature involving permissions must verify at least two roles (this project currently has no permission system, so this is "not applicable" overall; it takes effect if one is introduced in the future).
4. Every operation that mutates system state (writing files) must verify recovery after at least one failure: a failure must not leave a corrupted partial artifact behind.
5. When adding a new tier-1 business feature, you MUST add the corresponding E2E test and update the acceptance matrix in docs/roadmap.md at the same time, otherwise the change is incomplete.
