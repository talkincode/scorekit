# Building a Sound Library

scorekit ships no samples beyond the default MuseScore General SF2. The
reference sample corpus used to develop and certify the open renderer
profile (internally called **ScoreData**) is **not distributed** — partly
because several upstream licenses permit music use but restrict
repackaging (Virtual Playing Orchestra explicitly forbids redistribution),
and partly on principle: the corpus is a private, disk-local asset; what is
public is the **recipe**. This page is that recipe. Every library in the
corpus comes from a public channel listed below, so a third party can
rebuild an equivalent corpus from scratch and certify it with
`scorekit profile check`.

## Design rules

The corpus follows the anti-homogenization program in
[docs/roadmap.md](https://github.com/talkincode/scorekit/blob/main/docs/roadmap.md)
(section "Sound library & orchestration program"). The load-bearing rules:

1. **Versioned identity.** A library enters the corpus only with a
   publisher/version (or commit) identity, a license record, and checksums.
   Downloads retain archive checksums; first-party generated libraries retain
   per-file `SHA256SUMS`, the pinned recipe, and generator provenance.
2. **Certification before use.** A profile mapping counts as coverage only
   after `scorekit profile check` passes it: rendered twice, deterministic,
   non-silent, golden `render_sha256` recorded.
3. **Gaps close with real sources, never wider fallbacks.** A missing
   instrument is either closed with a genuinely fitting library or stays a
   visible, honest gap. Binding an unrelated patch to silence a warning is
   the one move that is always wrong.
4. **Additive mappings.** New libraries add mappings; they never silently
   rebind existing instruments to a different timbre. Rebinding is an
   audible style change and must be an explicit, reviewed edit.
5. **License evidence stays literal.** CC0, CC-BY,
   GPL-with-sampling-exception, and similar are preferred. No `NC`/`ND`
   variants and no commercial-SoundFont conversions. A public-domain
   declaration with an incomplete original sample chain can be accepted only
   when the declaration, missing evidence, and decision are permanently
   disclosed; it must never be relabelled CC0.

## Directory contract

```text
<corpus root>/                      # any disk location; not a git repo
  libraries/<publisher>/<lib>/<version>/   # extracted library content
  archives/<publisher>/<lib>/<version>/    # the original downloaded archive
  manifests/
    sources.tsv                     # acquisition ledger: archive, version, license, official URL
    archive-sha256sums              # checksums of every archive (verify from this directory)
    libraries/<lib>.yaml            # one manifest per library (identity, path, formats, license)
    patches/                        # diffs for locally repaired upstream files
  profiles/
    renderers/<name>.yaml           # scorekit leaf renderer profiles (instrument -> .sfz)
    orchestrations/<name>.yaml      # scorekit orchestration profiles (palette -> renderers/<name>.yaml)
    textures/<name>.yaml            # scorekit texture profiles (source name -> audio file)
  sf2/                              # SF2 soundfonts (GM tier)
  catalog/                          # generated inventory + stored certification reports
  incoming/                         # scratch area for downloads under evaluation
```

Two invariants keep the corpus auditable:

- `manifests/sources.tsv` is the append-only acquisition ledger — one line
  per downloaded archive with its official source URL. First-party generated
  libraries instead carry their recipe and `generator.json` in the versioned
  library directory.
- `shasum -a 256 -c archive-sha256sums` (run inside `manifests/`) must
  always pass; the certified `profile check --json` report stored under
  `catalog/` doubles as a golden-render baseline for every patch.

## Acquisition channels

Everything below is publicly downloadable. Versions are the ones the
reference profile was certified against; newer upstream versions usually
work but re-certify after any change.

### Foundation (orchestra, keyboards, percussion)

| Library | Version | License | Channel |
|---|---|---|---|
| VSCO 2 Community Edition | 1.1.0 | CC0-1.0 | <https://versilian-studios.com/vsco-community/> (also `github.com/sgossner/VSCO-2-CE`) |
| Versilian Community Sample Library (VCSL) | 1.2.2-rc | CC0-1.0 | <https://github.com/sgossner/VCSL> |
| Virtual Playing Orchestra (waves) | 3.2 | VPO mixed-open: music use unrestricted, **no repackaging** | <http://virtualplaying.com> |
| Virtual Playing Orchestra SFZ scripts | 3.3 | same as above | `virtualplaying.com/vp-downloads/Virtual-Playing-Orchestra3-3-standard-scripts.zip` + `...-performance-scripts.zip` |
| MuseScore General (SF2, GM tier) | 0.2.0 | MIT (samples: public domain / CC) | <https://ftp.osuosl.org/pub/musescore/soundfont/MuseScore_General/> (fetched by `make install`) |

The VPO 3.3 SFZ scripts are overlaid onto the 3.2 wave set (merge the
`standard` and `performance` script trees into the extracted 3.2 library);
this is how the choir, solo voice, celesta, and english horn mappings are
sourced.

### Guitars, basses, drums, e-pianos (sfzinstruments / Karoryfer)

| Library | Version | License | Channel |
|---|---|---|---|
| Karoryfer Black & Green Guitars | 1.000 | CC0-1.0 | `github.com/sfzinstruments/karoryfer.black-and-green-guitars` (releases) |
| Karoryfer Black & Blue Basses | 1.002 | CC0-1.0 | `github.com/sfzinstruments/karoryfer.black-and-blue-basses` (releases) |
| Virtuosity Drums | 0.925 | CC0-1.0 | `github.com/sfzinstruments/virtuosity_drums` (releases) |
| Greg Sullivan E-Pianos | commit `8c3e581` | CC-BY-3.0 | `github.com/sfzinstruments/GregSullivan.E-Pianos` |

### FreePats (synths, pads, folk & fretted instruments)

All from <https://freepats.zenvoid.org> or `github.com/freepats` releases;
CC0-1.0 unless noted.

| Library | Version | Notes |
|---|---|---|
| Synth Square / Synth Bass Lead / Synth Bass 1 / Synth Bass 2 | 2020-05-12 / 2020-05-22 / 2019-07-23 / 2021-04-05 | |
| Lately Bass | 2024-04-09 | |
| Synth Strings 1 / Synth Strings 2 | 2020-05-28 | |
| Synth Pad Bowed / Synth Pad Choir / Sweep Pad / New Age | 2019-07-19 / 2020-05-16 / 2019-08-13 / 2019-07-30 | |
| Spanish Classical Guitar | 2019-06-18 | nylon guitar |
| FSS Steel String Guitar | 2020-05-21 | **GPL-3.0-or-later with FreePats sampling exception** (rendered music is unencumbered; see the package's `readme.txt`) |
| Button Accordion HN | 2024-03-29 | |
| MuldjordKit (acoustic drums) | 2020-10-18 | CC-BY-4.0 |

### Community one-offs

| Library | Version | License | Channel |
|---|---|---|---|
| SamsterBirdies Pan Flute | commit `60d4974` | CC0-1.0 | `github.com/SamsterBirdies/panflute` |
| GM LightCool Fretless Bass | artifact 5323 | Public-Domain-Claimed; original Freesound rights chain undisclosed | `musical-artifacts.com/artifacts/5323` |
| NeoSoundFonts BMS Pull Slap Bass | commit `d03fc7e` | CC0-1.0 | `github.com/NeoSoundFonts/SP-BMS-Slap-Pull` |

The pan flute ships with a defective SFZ (see next section) — repair it before
mapping. The two bass sources ship as SF2. Preserve each original, export with
the pinned Polyphone release, audit paired opcodes and non-silent WAV files,
retain raw and normalized SFZ checksums, then certify the normalized mapping.
Artifact 5323's metadata says only that its samples came from Freesound; it
does not identify those files or their original licenses. Its manifest and
`SOURCE-DECLARATION.md` therefore keep that limitation visible.

### World instruments

The world-instrument vocabulary is additive: the original 60-instrument
coverage target stays intact, while each of these 11 identities is admitted
only with an exact source.

| Instrument | Certified source | License / status |
|---|---|---|
| Erhu | AliExpress Erhu v1.000, tag/commit `6615047b`, corrected sustain keymap + upstream short program | CC0-1.0; release archive SHA-256 `2f54cc1a19ccd842f1c05eeb136416e680633ff4112201d14f6bf9931aad3fe0` |
| Tabla | Subodh Deolekar, [Tabla strokes dataset](https://doi.org/10.5281/zenodo.4327350), 650 original WAVs + deterministic 550-region SFZ | CC-BY-4.0; archive SHA-256 `513ea8b7cae6e5ec7038a1dd3e3054e061739c64c13603faa6c3df8ea1468fa2` |
| Shakuhachi | MuseScore General zero-based GM program 77 | MIT SoundFont; exact SF2 tier |
| Sitar | MuseScore General zero-based GM program 104 | MIT SoundFont; exact SF2 tier |
| Shamisen | MuseScore General zero-based GM program 106 | MIT SoundFont; exact SF2 tier |
| Pipa, guzheng, dizi | — | Pending an exact, primary-source licensed payload |
| Oud, ney, duduk | — | Pending an exact, primary-source licensed payload |

Acquire Erhu from the pinned
[`v1.000` release](https://github.com/sfzinstruments/aliexpress-erhu/releases/tag/v1.000);
the tag resolves to commit
`6615047b2fd06126877483e97b8bb4af9d00b080`.
`manifests/recipes/build_erhu_clean_sfz.py` maps the unchanged sustain WAVs
one octave higher to their measured MIDI pitches, removes the upstream
synthetic wobble/unison layers, restores velocity response, and sets the bend
range to two semitones. The profile uses the generated clean patch for sustain
and `03-erhu_short.sfz` for staccato. The upstream marcato program layers those
two recordings and is not relabelled as another technique.

For Tabla, retain the Zenodo archive and all 650 WAV files byte-for-byte.
`manifests/recipes/build_tabla_sfz.py` verifies the complete Zenodo MD5
inventory plus 44.1 kHz/stereo/16-bit PCM structure, then writes 11 keys
(MIDI 36–46), each using 50 sequential `seq_position` takes. It performs no
normalization, trimming, resampling, effects, or random selection. Attribution
to Subodh Deolekar and the CC BY 4.0 license stay beside the payload.

#### Private/commercial source boundary

A locally licensed source may close any remaining identity without changing
the DSL: keep its payload outside the scorekit repository, give it a versioned
manifest in the corpus, map the exact instrument in a private leaf renderer
profile, and run `scorekit profile check`. Only vendor-supplied SFZ or an
explicitly permitted WAV/SFZ export is admissible. Do not extract or convert
Kontakt/Pianobook packages whose terms forbid sampler conversion, and never
commit their samples. Until such a payload exists, the identity must remain
unmapped and fail visibly.

### First-party synthesized (scoredata-forge)

| Library | Version | License | Rebuild source |
|---|---|---|---|
| ScoreData Dubstep Growl | 1.0.0 | CC0-1.0 | `scoredata-forge/recipes/dubstep-growl.toml` |
| ScoreData Music Box | 1.0.0 | CC0-1.0 | `scoredata-forge/recipes/music-box.toml` |
| ScoreData Whistle | 1.0.0 | CC0-1.0 | `scoredata-forge/recipes/whistle.toml` |

These three ordinary WAV+SFZ libraries close structurally simple timbre gaps
and the control-responsive growl gap without putting synthesis inside
scorekit. Build and verify them in the separate `scoredata-forge` repository;
install the full versioned output directory so `recipe.toml`,
`generator.json`, `SHA256SUMS`, and the CC0 dedication remain with the
samples. The producing platform proves a byte-identical rebuild with
`forge verify`; the installed consumer boundary is independently gated by
`scorekit profile check`.

### Textures

The reference texture profile exposes 93 certified sources from libraries
already in the corpus — VCSL and VSCO 2 CE "Miscellania" — spanning 2
ambiences, 9 foley gestures, 25 impacts, 12 transitions, 15 tonal gestures,
12 industrial mechanisms, and 18 abstract sound-design sources. Every source
has a unique path and source hash; three are authored for looping and the
rest are explicit one-shots. A full scan of the other manifested libraries
found no honest additions: they contain melodic multisamples, release
triggers, or round-robin drum banks rather than standalone textures, so they
were not bulk-imported merely to inflate the count. The `organic` category
remains an explicit gap in this **open reference profile**: simulated surf from
an ocean drum is labelled as such rather than presented as a natural field
recording. Texture profiles use the same portable-name-to-local-path model as
renderer profiles, plus required discovery metadata (description, category,
tags, playback modes, use cases, provenance) so an agent can pick a source
without opening the files; see
[Orchestration and Renderer Profiles](profiles.md).

A texture source enters the corpus the same way an instrument does: acquire
→ manifest → map → certify. `scorekit texture check` is the texture-side
counterpart of `scorekit profile check` — it proves every declared source
exists, decodes, and is audible before a scene depends on it, and records a
`sha256` of the source file so a swapped recording is detectable.

#### Optional private nature profile

The local ScoreData corpus also has a deliberately isolated, non-commercial
nature layer. It acquires 775 unique upstream files and maps 774 one-shot
sources after quality gating: 720 ESC-50 WAV excerpts, 49 National Park
Service recordings, and 5 NOAA PMEL underwater recordings. The resulting
`scoredata-nature-personal` profile contains 530 `organic` and 244 `ambience`
sources; `scoredata-personal-all` combines those 774 with the 93-source open
reference set for 867/867 certified sources. One NPS stream recording remains
in the archive but is not mapped because MP3 decoding overshoots 0 dBFS.

This layer does **not** close the open-profile organic gap:

- ESC-50 is pinned at commit
  `33c8ce9eb2cf0b1c2f8bcf322eb349b6be34dbb6` (archive SHA-256
  `661183a6f53ef04f12c9bd618fed0ddc1713280d6c94a5a5431e844ba6f6a21f`).
  Select the 18 categories `cat`, `chirping_birds`, `cow`,
  `crackling_fire`, `crickets`, `crow`, `dog`, `frog`, `hen`, `insects`,
  `pig`, `rain`, `rooster`, `sea_waves`, `sheep`, `thunderstorm`,
  `water_drops`, and `wind`. Preserve the WAV bytes, `LICENSE`, README,
  complete metadata CSV, per-file Freesound IDs, and attribution. The
  dataset-wide license is CC BY-NC 3.0, so these sources must never enter
  commercial work or the default open profile.
- Crawl only the Amphibians, Birds, Geological, Hydrological, Insects,
  Mammals, Meteorological, and Reptiles sections of the
  [NPS Natural Sounds Gallery](https://www.nps.gov/subjects/sound/gallery.htm).
  Save the gallery's public-domain declaration, each detail page, direct audio
  URL, NPS credit, and source checksum. Fifty files are acquired; Singing
  Sands exposes no downloadable audio and the clipping gate excludes one
  stream file, leaving 49 mapped sources.
- Download the five WAV links exposed by the
  [NOAA PMEL Acoustics gallery](https://www.pmel.noaa.gov/acoustics/multimedia.html).
  NOAA pages can contain partner material, so these remain tagged
  `rights_review` until each asset's rights chain is independently cleared.

Keep the three libraries in separate versioned directories and retain an
attribution ledger. After generating either personal profile, certify it with:

```bash
scorekit texture check profiles/textures/scoredata-nature-personal.yaml
scorekit texture check profiles/textures/scoredata-personal-all.yaml
```

## Repairing defective upstream files

Occasionally an upstream file is broken as shipped. Two repairs exist so
far: the pan flute's SFZ was exported with every `lokey/hikey`,
`lovel/hivel`, and `loop_start/loop_end` pair reversed (every region empty,
rendering silence), and the MuldjordKit SFZ uses DrumGizmo's nonstandard
drum keymap (kick on 48, snare on 50…) instead of General MIDI (remapped
to GM keys: kick 36, snare 38, hats 42/46…). The repair convention:

1. Keep the upstream file **byte-intact**.
2. Place the repaired copy alongside it with a `.scoredata-fixN.` infix and
   a header comment stating what changed and why.
3. Store the diff under `manifests/patches/` and record a structured
   `transforms:` entry in the library manifest (upstream and normalized
   SHA-256, patch path, type, reason, reversibility).
4. Map only the repaired file.

Anyone rebuilding the corpus can re-apply the published patch or re-derive
the fix from its description; nothing about the repair lives only in git
history or someone's memory.

Not every defect is repairable. VPO's `all-brass-SEC-sustain`,
`all-brass-SOLO-sustain` and `all-strings-SOLO-sustain` ensemble files
hang sfizz 1.2.3 (rendering never finishes); bisecting shows any 4 of a
file's 7 groups render fine while any 5 hang — a cumulative voice-count
interaction with no single broken opcode to patch. Such files are recorded
as do-not-map in the library manifest's `notes:` and profiles re-orchestrate
around them visibly (map the individual section patches instead) — never by
silently substituting a different sound.

## Certification workflow

After placing libraries, write a renderer profile mapping scorekit
instrument names to `.sfz` paths (see [Orchestration and Renderer
Profiles](profiles.md)), then:

```bash
# 1. Archive integrity (inside manifests/)
shasum -a 256 -c archive-sha256sums

# 2. Certify every mapping: rendered twice, deterministic, non-silent
scorekit profile check profiles/renderers/<name>.yaml

# 3. Store the machine-readable report as the golden baseline
scorekit --json profile check profiles/renderers/<name>.yaml > catalog/reports/<name>.json
```

Each passing patch reports a `render_sha256`; diffing two stored reports
pinpoints exactly which patches changed after a library or tool upgrade. A
failing comparison is retried once in isolation with diagnostics recorded
(`load_sensitive_flake`) so a loaded machine does not produce false
nondeterminism verdicts — see [Orchestration and Renderer Profiles](profiles.md).

The corpus currently certifies **four general-purpose renderer profiles — 242
mappings over 180 unique patches, 0 failures**:

- `scoredata-open` — broad reference: 108 mappings / 92 patches, covering
  all 60 core DSL instruments plus exact Erhu (sustain/staccato) and Tabla.
  First-party CC0 music-box and whistle libraries
  cover the structurally simple timbres; the declared-public-domain fretless
  and NeoSoundFonts CC0 pull/slap libraries cover the technique-specific
  basses. The sample tier and GM SF2 tier both retain 60/60 core coverage;
  world coverage is tracked separately (2/11 sample tier, 3/11 exact GM).
- `scoredata-chamber` — one player per part: VPO SOLO strings/winds/brass,
  VSCO 2 CE upright piano and quiet organ, VCSL harpsichord and recorder
  (49 / 42). No percussion, drums, synths or ensemble patches — deliberate
  identity gaps.
- `scoredata-symphonic` — full sections: VPO SEC strings/winds/brass,
  orchestral percussion, harp, celesta and choirs, VCSL Steinway B, VSCO 2
  CE loud organ (64 / 54). No synths or electric instruments.
- `scoredata-synth` — FreePats synth basses/leads/pads/strings, Karoryfer
  electric guitar and bass, Wurlitzer EP200, MuldjordKit drums (21 / 15).
  The acoustic orchestra is intentionally absent.

The separately certified Heavy Dubstep production identity adds four
single-mapping leaf profiles — `scoredata-dubstep-growl`,
`scoredata-dubstep-sub`, `scoredata-dubstep-metal`, and
`scoredata-dubstep-drums` — with 4/4 physical patches passing. Only the growl
patch is new; the other three deliberately reuse independent certified
sources rather than collapsing the roles onto one synth. Its active
certification proves that CC1, CC74, and pitch bend all produce deterministic,
non-silent PCM changes. The paired texture profile certifies 4/4 bindings:
chain-grind bed, hybrid impact, electric zap, and a mechanical siren visibly
used as `mechanical_riser` rather than mislabeled as reverse-processed audio.

The chamber/symphonic pair doubles as the documented solo-vs-section
variant pair: same score, audibly different orchestration identity. Binding
both under one orchestration profile (e.g. `solo: scoredata-chamber`,
`ensemble: scoredata-symphonic`) lets a single scene layer a soloist over a
section without ever naming a local path — see [Orchestration and Renderer
Profiles](profiles.md).

## Minimal rebuild walkthrough

```bash
ROOT=/path/to/my-sound-corpus
mkdir -p $ROOT/{libraries,archives,manifests/{libraries,patches},profiles/{renderers,orchestrations},catalog/reports,incoming}

# For each library in the tables above:
#   1. download the pinned version/commit from its channel into incoming/
#   2. verify + record:  shasum -a 256 <archive> >> $ROOT/manifests/archive-sha256sums
#   3. append a line to $ROOT/manifests/sources.tsv  (archive, version, license, URL)
#   4. extract into $ROOT/libraries/<publisher>/<lib>/<version>/
#   5. move the archive to $ROOT/archives/<publisher>/<lib>/<version>/
#   6. write $ROOT/manifests/libraries/<lib>.yaml  (id, name, publisher,
#      version, path, formats, tags, license, license_file, archive)

# Write a renderer profile over the extracted .sfz files, then certify:
scorekit profile check $ROOT/profiles/renderers/my-profile.yaml

# Wire it into an orchestration profile (palette -> renderer profile):
cat > $ROOT/profiles/orchestrations/my-orchestration.yaml <<'EOF'
schema_version: 1
name: my-orchestration
default_palette: main
palettes:
  main:
    profile: ../renderers/my-profile.yaml
EOF
scorekit orchestration check $ROOT/profiles/orchestrations/my-orchestration.yaml

# Point scorekit at the corpus:
export SCOREKIT_SOUND_LIBRARY_DIR=$ROOT
scorekit build scene.yaml --renderer sfizz \
    --orchestration $ROOT/profiles/orchestrations/my-orchestration.yaml -o out.ogg
```

A rebuilt corpus will not be byte-identical to the reference one (different
download dates, archive re-compressions), but after certification it gives
the same guarantee that matters: every mapped patch renders, is audible,
and is deterministic — and your own stored report becomes your baseline.
