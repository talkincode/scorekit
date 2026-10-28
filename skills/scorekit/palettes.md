# Palettes — anti-homogenization protocol

Agents drift toward one sound: orchestral strings, D minor, slow 4/4,
"cinematic". Every scene then sounds like the last one. This file is the
counterweight: a catalog of distinct sound identities plus a decision
protocol that makes style choice **explicit and auditable** (audit
dimension K) instead of habitual.

## The inertia rule

Before writing `tracks:`, always do this in your plan/brief — it costs
four lines:

1. **Name the inertia answer.** "My default here would be strings + harp,
   D minor, 60 BPM." Naming it breaks its silent pull.
2. **List 2–3 candidate palettes** from the catalog that could serve the
   brief. Almost any palette can carry any emotion — grief works on a
   music box, hope works on a saw lead. "It's emotional" never by itself
   justifies orchestra.
3. **Pick one, citing the brief** (setting, era, technology level,
   culture, character), not the mood alone.
4. **Declare the variation axes** changed vs your recent deliverables
   (see below). Skip only when the user asked for a matching/series style.

Then **measure it instead of asserting it**. The axes and the instrument
overlap are computed by the tool, not by your own judgement:

```bash
scorekit lint scene.yaml \
  --grammar grammars/anti-homogenization.yaml \
  --corpus out/ --corpus examples/scenes
# fails (the strings quartet below, measured against the shipped scenes):
# corpus_instrument_overlap_max @ corpus `examples/scenes/elegy.yaml`:
#   measured 1.00 (shared cello, harp, slow_strings, violin), want <= 0.60
# corpus_axes_changed_min @ corpus `examples/scenes/elegy.yaml`:
#   measured 0 of 8 axes changed (kept: palette_family, key_mode, …), want >= 3
# passes (a re-paletted piece):
# ok: ... (2 rule(s) checked; corpus 11 scene(s), 9 skipped)
# nearest out/heavy-drop/circuit-breaker-120s.yaml — instrument overlap 0.25
#   (shared picked_bass), 6/8 axes changed (palette_family, key_mode, …)
```

A corpus that contains the piece you are replacing is the intended use; a
corpus that contains *other* pieces is the useful one. A revision of the same
piece will (correctly) be reported as a sibling — that is not a new identity,
and calling it one is the bug this whole file exists to prevent.

Orchestral palettes stay fully legal — they just have to *win* the
decision, not inherit it.

## Variation axes

When producing multiple pieces (one session, one project, or a series),
change **at least 3 axes** between consecutive deliverables unless the
user pinned the style:

| Axis | Values to rotate through | How the tool measures it |
| --- | --- | --- |
| Palette family | see catalog below — the biggest lever | family most tracks belong to (tie → `mixed`) |
| Key + mode | not always minor; try major, and different tonics (G, A, Eb…) | parsed tonic + major/minor |
| Tempo class | slow <70 · mid 70–110 · driving 110–140 · fast >140 | scene tempo |
| Meter | 4/4 · 3/4 · 6/8 (compound lilt) · 5/4, 7/8 (odd, via numerator 5/7) | parsed time signature |
| Lead timbre | bowed · plucked · blown · struck · synthesized · vocal | timbre class of the loudest melody track |
| Pulse | none · soft (low intensity drums) · groove (drums/tabla forward) | presence + intensity of percussion tracks |
| Harmony color | default cycle · modal (VII, v in minor) · static drone (one chord) · major-key borrowings | authored progression vs the built-in default |
| Density | 2–3 tracks chamber · 4–5 standard · 6+ layered | track count |

`scorekit diff old.yaml new.yaml` on your last two scenes is a quick
objective check: if it shows only motif tweaks, you haven't varied. The
corpus lint above is the same check scaled to a whole project, with the
numbers attached.

### Series are legal, hiding is not

A deliberate series (five Disco identities, a combat set) is allowed to keep
an axis. What is not allowed is a *series* nobody declared: if the same
palette comes back for an unrelated brief, either change it or say in one
line which brief-driven reason keeps it. Raise the thresholds in that
project's own constitution (or narrow `--corpus`) rather than silencing the
WARN.

## Palette catalog

All instrument names below are valid DSL names (GM-backed unless marked
*profile*). Sketches are starting points, not fixed kits — swap freely
within the identity.

| Palette | Identity | Core tracks (instrument → pattern) | Color notes |
| --- | --- | --- | --- |
| `chiptune` | 8-bit console, playful/tense | square_lead→melody · saw_lead→arpeggio · synth_bass→bass · drums→drums | fast arps, tight rests; no reverb; GM square/saw survive SF2 well |
| `music-box` | intimate, fragile, nostalgic | music_box→melody · celesta/marimba→arpeggio · celesta→sustain · picked_bass→bass | high register, sparse, huge rests; worked: `examples/lantern-keepers.yaml` |
| `jazz-noir` | smoky, late-night, wry | vibraphone or epiano→melody · muted_guitar→arpeggio · fretless_bass→bass · drums→drums (low) | `swing: 0.25+`, mid tempo, ii–v colors; GM vibes/epiano are the two safe leads |
| `folk-acoustic` | rural, warm, handmade | recorder or whistle→melody · steel_guitar→arpeggio · accordion→sustain · contrabass→bass | major/mixolydian, 3/4 or 6/8 welcome; recorder is the GM wind that does not squeak |
| `synth-ambient` | weightless, interior, sci-fi | warm_pad→sustain · halo_pad→sustain · epiano→melody · synth_bass→bass | slow harmonic rhythm or single-chord drone; GM pads are the least dated synths |
| `east-asian` | pentatonic, air and space | shakuhachi→melody · harp (as koto)→arpeggio · pizzicato→bass · music_box→melody (answer) | pentatonic degrees 1/3/4/5/7; *profile*: erhu, pipa, guzheng, dizi |
| `percussion-forward` | ritual, urgency, earth | marimba or xylophone→melody · timpani→bass · drums→drums · piano (low)→sustain | rhythm is the lead; melody minimal; *profile*: swap drums for tabla→tabla |
| `sacred-organ` | vast interior, awe, dread | organ→sustain · choir→melody · tubular_bells→melody (accents) · contrabass→bass | modal harmony, slow, let bells ring; GM organ is steady, GM choir is a vowel pad |
| `baroque-chamber` | precise, courtly, clockwork | harpsichord→melody · harpsichord→arpeggio · cello→bass | 2–3 voices max — chamber, not symphony; the one place a single bowed timbre is the point |
| `electro-drive` | motion, neon, pursuit | saw_lead→melody · synth_bass→bass · drums→drums · sweep_pad→sustain | 120+ BPM, 16th-note arps |
| `nu-disco` | polished Nu-Disco drive | synth_bass→clip · muted_guitar→clip · epiano→sustain · synth_brass→clip · drums→clip | 112–122 BPM, four-on-floor, syncopated bass, bright restrained hook |
| `disco-70s` | orchestral dancefloor | bass→clip · muted_guitar→clip · strings/brass→sustain · drums + auxiliary drums→clip | tambourine/cowbell lift, live-kit feel, no EDM supersaw |
| `disco-funk` | dry pocket and rhythmic bite | clavinet→clip · slap_bass→clip · muted_guitar→clip · brass→clip · drums + conga→clip | interlocking rests matter more than layer count; worked: `examples/night-market-strut.yaml` |
| `disco-italo` | 80s machine romance | synth_bass→clip · synth_brass→clip · saw_lead→melody · pad→sustain · drums→clip | rigid machine pulse, octave bass, arps, dramatic minor/major color |
| `disco-house` | loop-driven club propulsion | epiano/organ→clip · synth_bass→clip · synth_brass→clip with linear CC74 · drums→clip | 118–126 BPM, offbeat hats, filtered chord motion; keep source paths in profiles |
| `desert-modal` | arid, ancient routes | pan_flute or ocarina→melody · harp→arpeggio · contrabass→bass · drums→drums (sparse) | minor with modal color: lean on VI/VII/v, avoid raised-leading-tone pull; *profile*: oud, ney, duduk, tabla→tabla |
| `lo-fi-bedroom` | private, unpolished, tender | epiano→melody · muted_guitar→arpeggio · fretless_bass→bass · drums→drums (very low intensity) | mid-slow, `humanize` timing high (30+) |
| `heroic-brass` | triumph, arrival, banners | trumpet→melody · horn→sustain · tuba→bass · timpani→drums-adjacent accents (pattern: bass) · brass→sustain | the *earned* orchestral pick — brass-led, not string-led; GM trumpets are exposed above C5, keep the lead mid-register |
| `strings-cinema` | grief, memory, wide shots | see the counterexample below | classic film palette — must cite why nothing else serves the brief |

Two tiers for world identities: `shakuhachi`, `sitar`, `shamisen` are
exact GM; `erhu`, `pipa`, `guzheng`, `dizi`, `oud`, `ney`, `duduk`,
`tabla` need a real renderer-profile mapping (`--renderer sfizz
--orchestration`) — check `scorekit inspect-instruments` before promising
them, and never fake one with a lookalike.

### Which palettes survive the default SF2 path

The default backend (FluidSynth + MuseScore General) is a *baseline*, not a
verdict on a palette. Keys, mallets, guitars, basses, pads, choir, drums,
`square_lead`, `saw_lead`, `pan_flute` and `recorder` are safe there and need
no library. Exposed GM brass/winds high in the staff, GM organ reeds and
`synth_strings` are where it sounds like a placeholder — that is an argument
for `--renderer sfizz --orchestration` with a real source (see the ScoreData
profile set), **not** an argument for going back to strings. Judge the
palette on a proper render before you judge it.

## Same brief, different palettes (worked contrast)

"A character walks through ruins, remembering" does **not** force
strings:

- `music-box`: memory as a toy left behind — music_box states the motif
  alone, harp answers an octave down, silence does the grieving.
- `synth-ambient`: memory as fog — the motif surfaces in epiano over a
  static warm_pad drone, never completes.
- `desert-modal`: memory as distance — pan_flute calls, tabla keeps
  walking, the harmony never leaves i.

Same motif discipline, same emotional curve, three unrecognizably
different scores. That is the standard dimension K holds you to.

## Counterexample: the palette that keeps winning by default

This is the block that used to sit in SKILL.md's cheat sheet as *the*
example. It is kept here on purpose, labelled, so the shape is available and
the pull is visible:

```yaml
tracks:
  - { id: lead,       instrument: violin,       pattern: melody,   motif: lament, intensity: 0.65 }
  - { id: motion,     instrument: harp,         pattern: arpeggio, intensity: 0.3 }
  - { id: harmony,    instrument: slow_strings, pattern: sustain,  intensity: 0.35 }
  - { id: foundation, instrument: cello,        pattern: bass,     intensity: 0.35 }
```

D minor, slow, 4/4, measured against the shipped examples this scores
`instrument overlap 1.00, 0 axes changed` against `elegy.yaml`. Nothing is
wrong with the notes; what is wrong is reaching for them without a decision.
Use it when the brief is a wide, grieving shot and nothing else serves it —
and say so.

## Groove skeletons (the blank-page cost is the other half of the pull)

Melody/sustain/arpeggio/bass are cheap to write and they *are* a chorale, so
under time pressure the strings path wins by default. Rhythmic identities need
authored `clips`; these skeletons remove the blank page. Copy one, keep the
grid, change the pitches:

```yaml
# 4 bars of 4/4, `length_beats: 16`, `mode: loop` — repeat per track.
# Every clip length must divide the scene/section length, and mode `loop`
# must return to its starting state, otherwise `validate` rejects it.
clips:
  pump_bass:                      # nu-disco / house: octave-root pump
    kind: pitched
    length_beats: 16
    mode: loop
    events:
      b01: { at: 0.0,  duration: 0.25, pitch: A1, velocity: 110 }
      b02: { at: 0.5,  duration: 0.25, pitch: A2, velocity: 96 }
      b03: { at: 1.0,  duration: 0.25, pitch: A1, velocity: 108 }
      b04: { at: 1.5,  duration: 0.25, pitch: A2, velocity: 94 }
      # …keep the 8th-note grid for 16 beats, move the root with the harmony
  pocket_guitar:                  # disco-funk / 70s: muted 16th chop
    kind: pitched
    length_beats: 16
    mode: loop
    events:
      g01: { at: 0.25, duration: 0.125, pitch: C4, velocity: 88 }
      g02: { at: 0.75, duration: 0.125, pitch: E4, velocity: 76 }
      g03: { at: 1.25, duration: 0.125, pitch: G4, velocity: 84 }
      g04: { at: 1.75, duration: 0.125, pitch: E4, velocity: 74 }
  stab_horns:                     # brass stabs: rests are the arrangement
    kind: pitched
    length_beats: 16
    mode: loop
    events:
      h01: { at: 2.0, duration: 0.5, pitch: E4, velocity: 104 }
      h02: { at: 3.0, duration: 0.5, pitch: D4, velocity: 92 }
  kit:                            # percussion clip: GM voices, channel 10
    kind: percussion
    length_beats: 16
    mode: loop
    events:                       # percussion events carry no duration
      k01: { at: 0.0,  voice: kick,       velocity: 116 }
      k02: { at: 0.75, voice: snare,      velocity: 104 }
      k03: { at: 1.5,  voice: closed_hat, velocity: 82 }
```

The full, valid, renderable versions are
`examples/night-market-strut.yaml` (clip groove, GM only, no strings) and
`examples/scenes/disco_funk.yaml` / `disco_70s.yaml` / `nu_disco.yaml`
(five independent Disco identities with grammars).

Two determinism notes that bite here: clip timing ignores
`swing`/`legato`/`humanize`, so mixing a swung `drums` pattern with straight
clips flams — either humanize with `timing_ms: 0` (velocity only) or commit to
authored clips for everything, and keep `harmony` to a static drone when the
clips carry fixed pitches.
