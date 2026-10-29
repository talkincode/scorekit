# Arrangement audit — scene review protocol

Audit an arrangement the way `lint` audits style: measured values against
wants, never vibes. Two layers — deterministic gates (tool-measured,
blocking) and a craft rubric (agent-measured, advisory) — and two modes:

- **Self-check (default):** run automatically after every completed
  compose or build, *before* reporting completion. A failed gate blocks
  completion; craft findings ship only with a concrete fix or a one-line
  creative justification.
- **Standalone:** the user hands you a scene (plus optional grammar,
  orchestration, texture profile, or a built `meta.json`) and asks for an
  audit, review, or critique. Run the same protocol, deliver the report,
  and change nothing unless asked.

Both modes read artifacts; neither invents schema fields or edits the
scene as a side effect.

## Layer 1 — deterministic gates (blocking)

Run every gate that applies; each is a real command with a
machine-readable verdict. Skip a gate only when its input doesn't exist
(no grammar in the project, no textures in the scene, not yet built) and
mark it `n/a` in the report.

| # | Gate | Command | Pass condition |
| --- | --- | --- | --- |
| G1 | Schema | `scorekit --json validate scene.yaml` | exit 0 |
| G2 | Style | `scorekit --json lint scene.yaml --grammar <g> [--corpus <dir> ...]` | exit 0 for every grammar profile the project declares (corpus rules need `--corpus`) |
| G3 | Instruments | `scorekit --json inspect-instruments scene.yaml [--orchestration <o>]` | no `missing`/`rejected`; every `fallback` reviewed (below) |
| G4 | Textures | `scorekit --json texture inspect <tp> --source <name>` per declared source, then `scorekit --json texture check <tp>` | every `textures[].source` exists, declares the scene's `mode` in `playback.modes`, and certifies audible |
| G5 | Build evidence | read `meta.json` next to the audio | `instrument_resolution` clean or reviewed; `loop_samples`/`total_samples` sane; stems listed when `--stems`; `story` echoed |

**Fallback review (G3/G5):** a `fallback` status is a diagnostic, never a
pass. Fix the mapping, re-orchestrate to an instrument the palette maps,
or record in the report *why* the substitute is musically acceptable for
this scene. Never widen `--fallback-mode`, lower resolver gates, or bind
an unrelated patch just to silence the WARN. World identities (`erhu`,
`pipa`, `guzheng`, `dizi`, `oud`, `ney`, `duduk`, `tabla`) never pass on a
lookalike — real mapping or visible re-orchestration.

## Layer 2 — craft rubric (advisory, measured)

Measure each dimension from the scene YAML (and `meta.json` when built).
Report `measured` vs `want` exactly like lint violations. Defaults below
are this skill's craft baselines; an explicit user brief or a project
grammar overrides them — note the override in the report.

| Dim | Measure | Want (default) |
| --- | --- | --- |
| A · motif economy | distinct motifs; which are variations of one cell | 1 core identity, ≤2 motifs total; variations relate by octave, rhythm, or answering instrument |
| B · voice discipline | peak simultaneous non-rest melody voices (overlap the melody tracks' beat grids) | ≤2 |
| C · breathing | per melody track: rest beats ÷ total beats | ≥0.3 |
| D · emotional curve | intensity × register × active-voice count over the timeline, as rough percentages | has corners (rise, peak, cut or contrast, return) — not a plateau |
| E · role coverage | tracks per role: foundation (`bass` or low pitched clip), harmony (`sustain`/`arpeggio`), lead (`melody` or foreground pitched clip), pulse (`drums`/`tabla` or percussion clip) | no unintended gap; no two tracks duplicating a role without registral or rhythmic contrast |
| F · register spacing | octave bands occupied by concurrent tracks, plus the lowest/highest MIDI note that actually sounds | bass alone at the bottom; ≤2 voices per band; melody clear of the pad's center; the low end and the air are occupied (`lowest_note_at_most` / `highest_note_at_least`) |
| G · loop/ending seal | loop: `dynamics` start == end and last chord pulls to the first; one-shot: final cadence matches the brief (`resolution` complete/incomplete) | matches the declared `loop` and the stated intent |
| H · determinism hygiene | `performance.humanize` present ⇒ `seed` set | seeded |
| I · story alignment | if `story:` present: each major musical decision (key, tempo, motif shape, curve, orchestration) traceable to it | no decision contradicts the brief |
| J · source honesty | unreviewed fallbacks from G3/G5; texture sources picked by guess | 0 |
| K · style independence | vs your recent deliverables (this session/project): instrument-set overlap, and variation axes changed (palette family, key+mode, tempo class, meter, lead timbre, pulse, harmony color, density — see [palettes.md](palettes.md)) | overlap <50% and ≥3 axes changed, unless the user pinned a series style; strings/orchestral palette carries a stated brief-based justification |
| L · delivery (room, level, width) | read `meta.json`: `space`, `master.achieved_lufs`, `master.achieved_true_peak_dbfs`, `limited_by_peak`; and the side/mid ratio of the delivered file | space is deliberate (not the dry default for a cue that wants depth); loudness in the brief's range (−16…−12 LUFS for game/film, unless the brief says otherwise); true peak ≤ −1 dBFS; width not collapsed <−15 dB side/mid |

**K is no longer a judgement call.** The eight axes and the overlap are
computed by the tool: declare the corpus rules in the project's constitution
(`corpus_instrument_overlap_max`, `corpus_axes_changed_min`) and run

```bash
scorekit lint scene.yaml --grammar grammars/anti-homogenization.yaml \
  --corpus out/ --corpus examples/scenes
```

A passing run prints `nearest <scene> — instrument overlap 0.42 (shared …),
5/8 axes changed (…)`; quote that line as the K evidence instead of a
sentence. A failing run is a G2 violation with `measured` vs `want`, which
makes a BLOCKED verdict — not a note-to-self. `--corpus` with no scenes in
it, or corpus rules without `--corpus`, is an input error: an unmeasurable K
must never be reported as passing.

Measuring L: `build` records the room preset and the full mastering plan in
`meta.json` (`space`, `master.{target_lufs, measured_lufs, achieved_lufs,
gain_db, achieved_true_peak_dbfs, limited_by_peak}`). Report the achieved
numbers, not the flags you passed. A delivered asset at −23 LUFS with a −9
dBFS peak is not "ready for the engine" — it is 13 dB of unused headroom, and
the finding is `fix: --master -14` (or a post FFmpeg pass if the crest factor
also has to come down, which `build` deliberately does not do because
compression is not distributive over stems).

Measuring K: list the instrument sets side by side and count; run
`scorekit diff` between the current and previous scene when both exist —
a diff that only touches motif content while palette, tempo, key, and
track list stay put is the homogenization signature. In standalone mode
with no prior pieces available, mark K `n/a (no corpus)` — or, when the
user supplies several scenes at once, measure K pairwise across them.
Prefer the corpus lint above whenever a corpus exists; `scorekit diff`
answers a different question (what changed inside one piece).

Dimension shortcuts by pattern: `sustain`/`arpeggio`/`bass`/`drums` fill
the whole scene, so B–D are shaped *only* by melody tracks' rests and by
`sections[].mute` — check those first when the curve reads flat. `clip` is not
a shortcut: inspect its exact event spans, automation, and section clip
overrides because it may be sparse or role-switching within every bar.

## Report format

One compact block per audit, always in this shape:

```text
Arrangement audit — scene.yaml (mode: self-check | standalone)
Gates: G1 PASS · G2 PASS (grief; corpus: overlap 0.42, 5/8 axes) · G3 PASS (1 fallback: oboe→english_horn, reviewed) · G4 n/a · G5 PASS
Craft:
  B voice discipline: measured peak 3, want ≤2 — fix: rest the harp under bars 13–16
  D curve: 20→40→70→70→70 — plateau after the peak; cut to silence or drop a role at bar 17
  (A, C, E–J in range)
Verdict: BLOCKED | SHIP WITH NOTES | CLEAN
```

- **BLOCKED** — any deterministic gate failed. Fix and re-run; never
  report completion over a blocked audit.
- **SHIP WITH NOTES** — gates pass, craft findings remain. Each finding
  carries a concrete fix *or* a one-line justification (e.g. "3 voices at
  the climax is the scene's single tutti moment, by design").
- **CLEAN** — gates pass, rubric in range.

In self-check mode, append the verdict line to the completion report. In
standalone mode, the report *is* the deliverable — include the prioritized
fix list and offer (don't apply) the edits.

## Graduate recurring findings into grammar

When the same craft finding recurs across a project's scenes, freeze it as
a measurable rule in a grammar profile — `tempo_min`/`tempo_max`,
`pads_max`, `melodic_voices_max`, `melody_rest_ratio_min`,
`phrase_min_beats`, `resolution`, `harmony_allowed`,
`require_performance`, `percussion_events_per_bar_min`,
`percussion_onsets`, `automation_activity`,
`corpus_instrument_overlap_max`, `corpus_axes_changed_min`, and named
`section_rules` — and
lint every new scene. The dimension then
becomes a G2 gate that survives model changes. Dimensions grammar cannot
yet measure (curve shape, register spacing, story alignment) stay in the
rubric; report them with measured values so a future rule has evidence.
