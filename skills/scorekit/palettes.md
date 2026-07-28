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

Orchestral palettes stay fully legal — they just have to *win* the
decision, not inherit it.

## Variation axes

When producing multiple pieces (one session, one project, or a series),
change **at least 3 axes** between consecutive deliverables unless the
user pinned the style:

| Axis | Values to rotate through |
| --- | --- |
| Palette family | see catalog below — the biggest lever |
| Key + mode | not always minor; try major, and different tonics (G, A, Eb…) |
| Tempo class | slow <70 · mid 70–110 · driving 110–140 · fast >140 |
| Meter | 4/4 · 3/4 · 6/8 (compound lilt) · 5/4, 7/8 (odd, via numerator 5/7) |
| Lead timbre | bowed · plucked · blown · struck · synthesized · vocal |
| Pulse | none · soft (low intensity drums) · groove (drums/tabla forward) |
| Harmony color | default cycle · modal (VII, v in minor) · static drone (one chord) · major-key borrowings |
| Density | 2–3 tracks chamber · 4–5 standard · 6+ layered |

`scorekit diff old.yaml new.yaml` on your last two scenes is a quick
objective check: if it shows only motif tweaks, you haven't varied.

## Palette catalog

All instrument names below are valid DSL names (GM-backed unless marked
*profile*). Sketches are starting points, not fixed kits — swap freely
within the identity.

| Palette | Identity | Core tracks (instrument → pattern) | Color notes |
| --- | --- | --- | --- |
| `chiptune` | 8-bit console, playful/tense | square_lead→melody · saw_lead→arpeggio · synth_bass→bass · drums→drums | fast arps, tight rests; no reverb |
| `music-box` | intimate, fragile, nostalgic | music_box→melody · celesta→arpeggio · harp→sustain | high register, sparse, huge rests |
| `jazz-noir` | smoky, late-night, wry | vibraphone or epiano→melody · muted_guitar→arpeggio · fretless_bass→bass · drums→drums (low) | `swing: 0.25+`, mid tempo, ii–v colors |
| `folk-acoustic` | rural, warm, handmade | recorder or whistle→melody · steel_guitar→arpeggio · accordion→sustain · contrabass→bass | major/mixolydian, 3/4 or 6/8 welcome |
| `synth-ambient` | weightless, interior, sci-fi | warm_pad→sustain · halo_pad→sustain · epiano→melody · synth_bass→bass | slow harmonic rhythm or single-chord drone |
| `east-asian` | pentatonic, air and space | shakuhachi→melody · harp (as koto)→arpeggio · pizzicato→bass · music_box→melody (answer) | pentatonic degrees 1/3/4/5/7; *profile*: erhu, pipa, guzheng, dizi |
| `percussion-forward` | ritual, urgency, earth | marimba or xylophone→melody · timpani→bass · drums→drums · piano (low)→sustain | rhythm is the lead; melody minimal; *profile*: swap drums for tabla→tabla |
| `sacred-organ` | vast interior, awe, dread | organ→sustain · choir→melody · tubular_bells→melody (accents) · contrabass→bass | modal harmony, slow, let bells ring |
| `baroque-chamber` | precise, courtly, clockwork | violin→melody · harpsichord→arpeggio · cello→bass | 2–3 voices max — chamber, not symphony |
| `electro-drive` | motion, neon, pursuit | saw_lead→melody · synth_bass→bass · drums→drums · sweep_pad→sustain | 120+ BPM, 16th-note arps |
| `nu-disco` | polished Nu-Disco drive | synth_bass→clip · muted_guitar→clip · epiano→sustain · synth_brass→clip · drums→clip | 112–122 BPM, four-on-floor, syncopated bass, bright restrained hook |
| `disco-70s` | orchestral dancefloor | bass→clip · muted_guitar→clip · strings/brass→sustain · drums + auxiliary drums→clip | tambourine/cowbell lift, live-kit feel, no EDM supersaw |
| `disco-funk` | dry pocket and rhythmic bite | clavinet→clip · slap_bass→clip · muted_guitar→clip · brass→clip · drums + conga→clip | interlocking rests matter more than layer count |
| `disco-italo` | 80s machine romance | synth_bass→clip · synth_brass→clip · saw_lead→melody · pad→sustain · drums→clip | rigid machine pulse, octave bass, arps, dramatic minor/major color |
| `disco-house` | loop-driven club propulsion | epiano/organ→clip · synth_bass→clip · synth_brass→clip with linear CC74 · drums→clip | 118–126 BPM, offbeat hats, filtered chord motion; keep source paths in profiles |
| `desert-modal` | arid, ancient routes | pan_flute or ocarina→melody · harp→arpeggio · contrabass→bass · drums→drums (sparse) | minor with modal color: lean on VI/VII/v, avoid raised-leading-tone pull; *profile*: oud, ney, duduk, tabla→tabla |
| `lo-fi-bedroom` | private, unpolished, tender | epiano→melody · muted_guitar→arpeggio · fretless_bass→bass · drums→drums (very low intensity) | mid-slow, `humanize` timing high (30+) |
| `heroic-brass` | triumph, arrival, banners | trumpet→melody · horn→sustain · tuba→bass · timpani→drums-adjacent accents (pattern: bass) · brass→sustain | the *earned* orchestral pick — brass-led, not string-led |
| `strings-cinema` | grief, memory, wide shots | violin→melody · slow_strings→sustain · cello→bass · harp→arpeggio | classic film palette — must cite why nothing else serves the brief |

Two tiers for world identities: `shakuhachi`, `sitar`, `shamisen` are
exact GM; `erhu`, `pipa`, `guzheng`, `dizi`, `oud`, `ney`, `duduk`,
`tabla` need a real renderer-profile mapping (`--renderer sfizz
--orchestration`) — check `scorekit inspect-instruments` before promising
them, and never fake one with a lookalike.

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
