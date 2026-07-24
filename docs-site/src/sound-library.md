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
   publisher/version (or commit) identity, a license record, and an archive
   checksum. Unversioned downloads are candidate material, not coverage.
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
5. **Only clean licenses.** CC0, CC-BY, GPL-with-sampling-exception, and
   similar. No `NC`/`ND` variants, no "converted from a commercial
   SoundFont" material, no per-file-unclear collections.

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
    renderers/<name>.yaml           # scorekit renderer profiles (instrument -> .sfz)
    textures/<name>.yaml            # scorekit texture profiles (source name -> audio file)
  sf2/                              # SF2 soundfonts (GM tier)
  catalog/                          # generated inventory + stored certification reports
  incoming/                         # scratch area for downloads under evaluation
```

Two invariants keep the corpus auditable:

- `manifests/sources.tsv` is the append-only acquisition ledger — one line
  per archive with its official source URL.
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

This library ships with a defective SFZ (see next section) — repair it
before mapping.

### Textures

The reference texture profile draws ambience/sound-design sources from
libraries already in the corpus (VCSL ocean drum, wind chimes, bowed brake
drum, wine glasses; VSCO 2 CE "Miscellania" ambiences) — no additional
downloads. Texture profiles use the same portable-name-to-local-path model
as renderer profiles; see [SFZ Renderer Profiles](profiles.md).

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
instrument names to `.sfz` paths (see [SFZ Renderer
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
nondeterminism verdicts — see [SFZ Renderer Profiles](profiles.md).

The corpus currently certifies **four renderer profiles — 235 mappings
over 173 unique patches, 0 failures**:

- `scoredata-open` — broad reference: 101 mappings / 85 patches, covering
  56 of the 60 DSL instruments (the remaining gaps — `fretless_bass`,
  `music_box`, `slap_bass`, `whistle` — have no license-clean open source
  yet and are deliberately left unmapped rather than faked with
  substitutes; the GM SF2 tier still resolves them).
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

The chamber/symphonic pair doubles as the documented solo-vs-section
variant pair: same score, audibly different orchestration identity.

## Minimal rebuild walkthrough

```bash
ROOT=/path/to/my-sound-corpus
mkdir -p $ROOT/{libraries,archives,manifests/{libraries,patches},profiles/renderers,catalog/reports,incoming}

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

# Point scorekit at the corpus:
export SCOREKIT_SOUND_LIBRARY_DIR=$ROOT
scorekit build scene.yaml --renderer sfizz --profile $ROOT/profiles/renderers/my-profile.yaml -o out.ogg
```

A rebuilt corpus will not be byte-identical to the reference one (different
download dates, archive re-compressions), but after certification it gives
the same guarantee that matters: every mapped patch renders, is audible,
and is deterministic — and your own stored report becomes your baseline.
