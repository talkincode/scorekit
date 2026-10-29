# Worked example: narrative film score

Read this example when the user describes a character arc, dramatic scene, or
emotional story instead of providing scene YAML.

Two worked scenes ship, in deliberately unrelated palettes. Read both before
choosing anything:

| Scene | Palette | What it proves |
| --- | --- | --- |
| [exile-in-the-dunes.yaml](exile-in-the-dunes.yaml) | solo violin, harp, slow strings, cello · D minor · 56 BPM · 4/4 | one narrative → one orchestration, and the cost of that choice |
| [lantern-keepers.yaml](lantern-keepers.yaml) | music box, marimba, celesta, picked bass · G major · 84 BPM · 3/4 | the same motif discipline in a palette that is not orchestra |

The palette is a **decision inside the brief**, never a property of "film
score". A grieving wide shot is where `strings-cinema`
([../palettes.md](../palettes.md)) is the right answer — it is just not the
default answer.

## Copy-paste Agent prompt

Note the empty `配器 / palette` slot: it is filled after the inertia rule,
not before the brief is written.

```text
请使用 scorekit skill，为下面的电影场景创作并实际渲染一段配乐：

片名：（自行填入，或留空由你命名）
时长：100–115 秒
人物：（谁，想要什么，失去了什么）

叙事弧线：
1. 开头：……
2. 转折：……
3. 结尾：……

音乐要求：
- 写一个可记住的 4–6 音人物动机；必须回来、变化、成长。
- 配器：先按 palettes.md 的惯性规则做决定——写下我的惯性答案、列出 2–3 个
  候选调色板、依据 setting/era/technology/culture/character 选一个并说明理由，
  再写 tracks。弦乐只有在论证后才是答案。
- 留出明显空白，高潮后要突然收缩，结尾不要完全解决。
- 使用确定性的 humanize seed。
- 先写一段 Narrative brief，说明人物、目标、记忆、转折和结尾。
- Narrative 只存在于 brief，不要发明 scorekit schema 字段。
- 查询实时 schema，生成 YAML，执行 validate；用默认 MuseScore General
  SoundFont 构建 WAV，同时生成 stems。
- 若项目里有反同质化 constitution，运行
    scorekit lint scene.yaml --grammar grammars/anti-homogenization.yaml \
      --corpus <已有作品目录>
  并在报告里引用它测出的 overlap 与 axes 行。
- 最后报告文件路径、实际时长、人物动机、配器选择和验证结果。
```

This prompt is specific enough to produce a musical viewpoint and an arc, and
it names objective gates, so the Agent must produce and validate artifacts
instead of stopping at prose — while leaving the palette as a reasoned choice.

## Translate narrative before writing YAML

Write a compact brief first (both shipped scenes were written this way):

| Narrative beat | Musical decision |
| --- | --- |
| Hope still exists | State the five-note identity in a clear middle register over sparse harmony. |
| Home has disappeared | Raise the register, shorten the cell, increase harmonic pressure, then cut to silence. |
| He walks anyway | Return fragments lower and slower; end away from the tonic. |
| The world around him | **Palette decision goes here** — never "strings because it is emotional". |

Do not add `character`, `hope`, `memory`, or `narrative` keys to the scene.
Those ideas guide decisions; the DSL records only decisions with deterministic
compile semantics.

## Producing the artifact

Copy **one** of the two shipped scenes into the user's working directory as a
starting point, and only after the palette decision agrees with it:

```bash
scorekit doctor
scorekit schema > /tmp/scorekit-scene-schema.json
scorekit --json validate out/scene.yaml
scorekit build out/scene.yaml --stems -o out/scene.wav --space hall --master -14
```

`exile-in-the-dunes.yaml` is 24 bars at 56 BPM (about 102.9 s before the
non-loop tail) and develops its `exile_arc` cell through statement, collapse,
silence and an unresolved ending. `lantern-keepers.yaml` is 16 bars of 3/4 at
84 BPM, loops seamlessly, and carries the same craft in struck timbres.

`--space hall` gives the piece a room (a scene's `reverb` alone cannot) and
`--master -14` delivers the level; both are recorded in `meta.json`
(`space`, `master`). Without them the same render measures −23 LUFS with a
−82 dB tail — quiet and dry, which is what "thin" sounds like.

The default FluidSynth path resolves
`$SCOREKIT_SOUND_LIBRARY_DIR/sf2/MuseScore_General.sf2`. For SFZ libraries,
replace the build command with
`--renderer sfizz --orchestration <orchestration.yaml>`.

## Report the result

A useful final response is concrete:

```text
Rendered out/scene.wav with aligned stems and metadata.
Palette decision: inertia answer was strings + harp; candidates were
music-box, folk-acoustic and strings-cinema; picked music-box because the
brief is an object being carried, not a wide shot.
The lantern motif is the recurring 4–3–2–rest–5 cell: stated plainly, lifted an
octave, cut short, then inverted as it is handed over. Music box carries the
keeper; marimba and celesta stay thin; a plucked bass keeps the walking. The
last phrase ends on scale degree 2, so the loop wants to fall back into bar 1.
Validation passed; anti-homogenization measured overlap 0.00 and 6/8 axes
changed against the previous pieces; the scene is 16 bars at 84 BPM.
```

Completion means the YAML validates, audio and metadata exist, stems align,
the palette choice is traceable to the brief, and the explanation connects
narrative beats to audible musical decisions.
