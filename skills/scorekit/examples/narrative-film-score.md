# Worked example: narrative film score

Read this example when the user describes a character arc, dramatic scene, or
emotional story instead of providing scene YAML.

## Copy-paste Agent prompt

```text
请使用 scorekit skill，为下面的电影场景创作并实际渲染一段配乐：

片名：Exile in the Dunes
时长：100–115 秒
人物：一个被放逐的人，独自在沙漠中寻找已经不存在的家。

叙事弧线：
1. 开头：他仍相信远方有希望。
2. 转折：他发现记忆中的家已经消失，希望坍塌。
3. 结尾：他没有重新获得希望，但还是继续向前走。

音乐要求：
- D minor，4/4，缓慢但不能完全停滞。
- 写一个可记住的 4–6 音人物动机；必须回来、变化、成长。
- 以孤独的小提琴为人物视角，竖琴、低音弦乐和稀薄弦乐作为世界。
- 留出明显空白，高潮后要突然收缩，结尾不要完全解决。
- 使用确定性的 humanize seed。
- 先写一段 Narrative brief，说明人物、目标、记忆、转折和结尾。
- Narrative 只存在于 brief，不要发明 scorekit schema 字段。
- 查询实时 schema，生成 YAML，执行 validate，再用默认 MuseScore General
  SoundFont 构建 WAV；同时生成 stems。
- 最后报告文件路径、实际时长、人物动机、配器选择和验证结果。
```

This prompt is specific enough to produce a musical viewpoint and an arc, but
it leaves orchestration details to the Agent. It also names objective gates, so
the Agent must produce and validate artifacts instead of stopping at prose.

## Translate narrative before writing YAML

Write a compact brief first:

| Narrative beat | Musical decision |
| --- | --- |
| Hope still exists | State the five-note exile cell in a clear middle register over sparse harmony. |
| Home has disappeared | Raise the register, shorten the cell, increase harmonic pressure, then cut to silence. |
| He walks anyway | Return fragments lower and slower; end away from the tonic. |
| Desert world | Keep harp, slow strings, and bass quieter than the solo violin. |

Do not add `character`, `hope`, `memory`, or `narrative` keys to the scene.
Those ideas guide decisions; the DSL records only decisions with deterministic
compile semantics.

## Produce the artifact

Use [exile-in-the-dunes.yaml](exile-in-the-dunes.yaml). Copy it from the skill
directory into the user's working directory before editing or building it. Its
24 bars at 56 BPM last about 102.9 seconds before the non-loop tail. The long
`exile_arc` melody develops a recurring five-note identity across the opening,
collapse, silence, and unresolved ending.

```bash
scorekit doctor
scorekit schema > /tmp/scorekit-scene-schema.json
scorekit --json validate exile-in-the-dunes.yaml
scorekit build exile-in-the-dunes.yaml \
  --stems \
  -o out/exile-in-the-dunes.wav
```

The default FluidSynth path resolves
`$SCOREKIT_SOUND_LIBRARY_DIR/sf2/MuseScore_General.sf2`. For SFZ libraries,
replace the build command with `--renderer sfizz --profile <profile.yaml>`.

## Report the result

A useful final response is concrete:

```text
Rendered out/exile-in-the-dunes.wav with aligned stems and metadata.
The exile motif is the recurring 5–4–3–rest–1 cell: first intact, then raised
and compressed at the collapse, then reduced to fragments. Violin carries the
character; harp/slow strings/cello remain the desert world. The last phrase
ends on scale degree 2, so continuing to walk is expressed without resolution.
Validation passed; the scene is 24 bars at 56 BPM (about 102.9 s plus tail).
```

Completion means the YAML validates, audio and metadata exist, stems align,
and the explanation connects narrative beats to audible musical decisions.
