# scorekit 项目画像与方向

> **ScoreKit is an Agent-oriented Music Compiler, not an AI music generator.**
> 它把高级音乐语义稳定地编译为可执行资产；创作智能永远属于上游 Agent。

## 项目概述

scorekit 是一个 **Agent 驱动的游戏配乐编译器**：输入一份文本 DSL（YAML）描述的乐曲结构，输出游戏可直接使用的音频资产（无缝 loop、分轨 stem、场景过渡段）。它服务两类使用者：写 DSL 的 AI Agent，以及把产物放进引擎的独立游戏开发者。

它是一个 Rust CLI 薄编排层，不自研音频算法，只负责把稳定的中间格式可靠地编译下去：

```text
scene.yaml ──(唯一事实来源，git 直接版本控制)
    │  scorekit validate / schema
    ▼
乐曲 IR ──► scorekit midi ──► scene.mid（字节级确定）
    │
    ▼  scorekit render          ┌─ 渲染后端边界（可替换）─┐
FluidSynth + SF2 ──► scene.wav  │ 后续: sfizz+SFZ、其他   │
    │                           └─────────────────────────┘
    ▼  scorekit export（调用 FFmpeg）
scene.ogg + loop 元数据 + stems/*.ogg（样本对齐）
```

外部依赖：FluidSynth（渲染）、FFmpeg（转码/后处理）、SoundFont/SFZ 音源文件。命名已定为 scorekit，不再讨论。

## 项目画像（目标状态）

做好之后，它是一台**确定性的编曲编译器**：

- **确定性压倒一切。** 同一 DSL + 同一音源 + 同一版本工具，产出可复现：MIDI 字节级一致，音频在断言容差内一致。品质冲突时，宁可牺牲便利与性能，也不牺牲可复现性——这是 Agent 回归测试的地基。
- **一切皆文本。** DSL 是唯一事实来源，行导向、diff 友好。"Git for Music"（diff/merge/branch/review/rollback）由 git 原生完成，不是 scorekit 的功能。
- **产物是游戏资产，不是"一首歌"。** loop 必须样本精确（时长 = 小节数×拍×采样率的整数样本）、首尾无爆音；stems 必须等长且样本对齐；场景段（explore/combat/victory）共享主题素材。
- **对 Agent 友好。** 每个命令一次调用完成；失败以非零退出码 + 指向 DSL 行号的机读错误报告；schema 可通过命令导出，Agent 仅凭 schema 与报错即可写出合法 DSL。
- **薄。** 渲染与后处理永远委托外部工具；scorekit 的价值在 DSL 的稳定与编译的可靠，不在代码量。

## 当前能力清单

- **场景 DSL 与校验**：YAML 场景（tempo/key/拍号/bars/loop/tracks），未知字段拒绝、语义校验带字段路径、解析错误带行号；`scorekit validate`、`scorekit schema`（JSON Schema 导出）。证据：`src/schema.rs`、`tests/cli.rs`。
- **确定性 MIDI 编译**：`scorekit midi` 输出字节级稳定的 SMF（format 1、PPQ 480），模式：sustain/arpeggio/bass/drums，golden 基准锁定。证据：`src/composer.rs`、`src/midi.rs`、`tests/golden/forest.mid`。
- **渲染与导出**：`scorekit render`（FluidSynth+SF2，SF2 魔数预检，捕获 fluidsynth "exit 0 但报错"的静默失败）、`scorekit export`（FFmpeg → OGG，libvorbis→vorbis 回退）、`scorekit build` 全链路；所有写文件走临时文件+原子重命名，失败不留半成品。证据：`src/tools.rs`、`tests/cli.rs`。
- **机读错误**：`--json` 输出结构化错误（code/message/location/field/exit_code）；退出码约定 1 io / 2 输入非法 / 3 依赖缺失 / 4 外部工具失败。证据：`src/error.rs`。
- **音乐语法校验**：`scorekit lint scene.yaml --grammar grief.yaml`——外部语法档案声明确定性审美约束（tempo/留白/声部数/乐句长/终止式/和声白名单），深度规则从编译后 IR 实测，违规带实测值；`scorekit schema --grammar` 导出档案 Schema。证据：`src/grammar.rs`、`examples/grammars/grief.yaml`。
- **CI**：GitHub Actions（fmt+clippy+全量测试，含真实 fluidsynth/ffmpeg 渲染）。证据：`.github/workflows/ci.yml`。
- **测试资产**：`scripts/fetch_assets.sh` 下载 TimGM6mb.sf2 到 `assets/`（gitignored，不入库）。

## 非目标（铁律）

- **不自研 DSP。** 不写合成器、混响、压缩、重采样；一律委托 FluidSynth/FFmpeg 等外部工具。违反即"研究信号处理直到退休"。
- **不做 GUI / DAW / 时间轴编辑器。** scorekit 是编译器，不是 GarageBand。
- **不自研版本控制。** 不实现 `score commit/merge/branch`。版本控制交给 git；scorekit 至多提供 git 之上的只读语义 diff 展示。
- **不内嵌作曲智能。** 不调用 LLM、不做生成模型；作曲创意属于上游 Agent，scorekit 只编译结构。由此推论：**只编译有确定语义的字段**——游戏世界状态（hp/danger/emotion）、创作意图（`avoid: too_bright`）、抽象动机（`shape: question`）都属于 Agent 的提示词空间，不得进入 schema；schema 里不允许存在"不起作用的字段"。
- **不做游戏内实时音频运行时。** 实时混音、动态 stem 淡入淡出的运行时（含 Zig 设想）是另一个项目。
- **核心不绑定商业音源。** Kontakt/BBC 等只能作为渲染后端边界外的适配器接入；DSL schema 不得出现仅商业音源可实现的字段。

## 方向与意图（路线图）

> 阶段表达目标能力，不规定内部实现顺序；每阶段完成的判据见"完成的样子"与验收矩阵。

### M0 — 走通骨架（状态：已完成）

单音轨从 DSL 到可播放文件的全链路：`validate → midi → render(SF2) → export(ogg)`。服务画像中"确定性"与"对 Agent 友好"——golden test 与机读错误从第一天建立。实际交付超出底线：多音轨、`build` 全链路命令、CI。

### M1 — 游戏资产核心（状态：已完成）

多音轨、无缝 loop、样本对齐 stems、轨道强度（intensity）。这是与"能出声的 demo"拉开差距的阶段，服务"产物是游戏资产"画像。实录：FluidSynth 按毫秒时钟调度 MIDI，渲染实际周期 ≈ L−28 样本且有抖动，样本级周期性不可达；采用 2-pass 取 `[L, 2L)` + 尾部线性交叉淡化封口（终帧与 `raw[L-1]` 位级相等），环回点即原始渲染的相邻样本对——无缝可被位级断言验证。stems 用同一线性封口逐轨切割，逐样本求和与全混音 RMS 偏差 ≈0.2%。`build` 另产出 `<name>.meta.json`（loop_samples/total_samples/stems 清单）供游戏引擎与 Agent 消费。

### M2 — 结构化乐曲（状态：已完成）

段落结构（intro/loop A/loop B/combat/victory/failure）、可复用 pattern/motif、段落间过渡。服务"场景共享主题"的游戏叙事需求。实录：一个场景文件可声明 `motifs`（音阶级数旋律动机，`pattern: melody` 的轨道引用）与 `sections`（每段独立 bars/loop/tempo 覆盖/mute/强度缩放，共享 tracks、motifs、调性）；`build` 对 suite 逐段产出 `<stem>-<section>.<ext>` 资产与单一 suite manifest（`<stem>.meta.json`）；`midi --section` 可单独编译某段。过渡即短小非 loop 段落，无需专门机制。示例：`examples/scenes/forest_suite.yaml`。

### M3 — 后端可替换性验证（状态：已完成，音质升级保留为方向）

以第二个渲染后端证明渲染边界成立。实录：第二后端选择 TiMidity++ 而非 sfizz——sfizz 在 homebrew/apt 均无现成包（源码构建对 CI 不可行），TiMidity++ 双平台可安装且复用同一 SF2。`--renderer {fluidsynth|timidity}` 下同一 DSL 产出相同的样本精确长度、不同的音色字节；封口手术、stems、export 全部渲染器无关。过程中发现并防御了 TiMidity 的新失败模式：坏 SF2 时 exit 0 静默回退内置音色（stderr `***` 标记扫描）、或输出 header-only 零帧 WAV（渲染器无关的零帧兜底检查）。SFZ/采样级音质档（sfizz + 免费管弦库）保留为"方向与意图"，商业音源适配器持续 HOLD。

### M4 — Agent 体验完善（状态：已完成）

JSON Schema 导出、只读语义 diff（`scorekit diff`，git porcelain）、批量渲染与机读报告。服务"Agent 仅凭 schema 与报错即可工作"。实录：`schema` 命令 M0 已交付；`diff` 对比音乐语义而非文本（键序/格式/注释差异 diff 为空），porcelain 行输出 `~/+/- <path> <old> -> <new>`，`--json` 输出同构 JSON 数组，只读、退出码恒 0（差异不是错误）；`batch` 将多个场景渲染进一个目录，单场景失败不中断其余构建，逐场景结果（含错误 code/exit_code）写入 `report.json`，进程退出码反映首个失败，输出名冲突在构建前拒绝。

### M5 — 演奏表现层与和声声明（状态：已完成）

消灭"AI 味"的编译器答案。AI 味的来源不是旋律，而是每个音 velocity/timing 恒定；解药不是生成模型，而是**确定性的结构变换**：

- `performance` 块：种子化 humanize（velocity/timing 抖动，同种子同字节）、力度弧线（`dynamics`，loop 场景必须首尾同级以保无缝）、swing、legato。全部默认关闭——不写 `performance` 的场景字节级不变，golden 不动。
- `harmony` 字段：罗马数字和声进行（如 `[i, VI, III, VII]`），取代内部硬编码的两套进行。和声本就是内部单一事实源（bass/pad/arpeggio 全部派生自 `chord_for_bar`），本阶段只是把选择权交给 DSL——改和弦，所有轨道一起变。

实录：`performance` 施加于 compose 尾部、loop 复制之前——两遍 pass 位级一致，M1 的 `[L, 2L)` 封口数学不受影响（E2E 断言 humanize+swing 下 loop 仍样本级精确）；humanize 用内联 LCG（Knuth MMIX 常数）而非 rand 依赖，跨平台永久位级一致；dynamics 弧线 start→peak→start 构造性 loop 安全；不写 `performance`/`harmony` 的场景走原路径，forest golden 字节不动。

边界裁决实录（2026-07 架构评审）：五层提案（Story/Composition/Arrangement/Performance/Runtime）经审计**收窄为本阶段**。Story→Scene 翻译、Intent 字段、抽象 motif（`shape: question`）判为 Agent 提示词空间，塞进编译器即违反"不内嵌作曲智能"铁律；role→instrument 间接层判为无消费者的过载设计；事件式 timeline 已被 sections+mute+intensity 语义覆盖；Music IR 已由 `ScoreIr`+SMF（行业通用 IR，REAPER/MuseScore/Kontakt 原生消费）满足。

### M6 — 音乐语法引擎（Music Grammar lint）（状态：已完成）

把"审美"变成可校验断言。`scorekit lint scene.yaml --grammar grief.yaml`——语法档案是外部 YAML 数据文件（`scorekit schema --grammar` 导出其 JSON Schema），声明一组确定性约束；编译器只校验、不生成一个音符，与"不内嵌作曲智能"铁律相容。

- 规则集（全部可选，至少声明一条）：`tempo_min/max`、`pads_max`、`melodic_voices_max`（同时发声旋律声部峰值）、`melody_rest_ratio_min`（**逐旋律轨**留白占比——对话式织体的 union 几乎无空隙，声部自身的呼吸才是留白）、`phrase_min_beats`（音符区间按 <2 拍间隙归并为乐句）、`resolution: complete|incomplete`（末音是否落主音）、`harmony_allowed`、`require_performance`。
- 表面规则直接查 Scene；深度规则从 **compose 后的 ScoreIr 实测**（pattern 展开 + performance 变换后的真实结果，而非 YAML 表面）。suite 场景逐 section 检查。
- 违规输出带实测值（`tempo_max @ scene: measured 92, want <= 60`），`--json` 输出 violations 数组，exit 2——Agent 拿到的是可执行的修正指令，不是审美形容词。
- 出厂参照对：`examples/grammars/grief.yaml`（孤独语法：tempo≤60、pad≤1、旋律声部≤2、留白≥35%、乐句≥5 拍、不完全终止、哀歌和声白名单、必须有 performance）+ `examples/scenes/dunes.yaml`（《东邪西毒》创作实战定稿，活证据：宪法是可满足的）。

价值兑现：Agent 获得**审美回归测试**——风格体系沉淀为数据文件，跨模型换代存续；创作反馈回路从"人耳听"变为"机器断言 + 人耳终审"。来源：2026-07 电影配乐三版创作实战复盘（用户提出"音乐语法引擎"定位）。

## 方向与意图（挂起，待证据）
- **演奏空间与滑音字段**（per-track `pan`、`reverb` send、尾音 portamento）：确定性渲染语义（MIDI CC10/CC91/pitch-bend），合格 schema 字段。需求来自真实创作（电影配乐三连版实战："近→远→宽→收回"的空间叙事今天无法表达）。
- **声明式 runtime 清单**：meta.json 已是引擎数据契约；将其扩展为可声明"状态→段落/分轨映射、淡入淡出时长"（编译器只校验引用、不执行）合法且有价值——但**挂起直到出现真实引擎集成**驱动字段设计，避免为无消费者的契约猜测 schema。执行版运行时（实时混音）仍是铁律禁区。
- **SFZ/采样级音质档**（自 M3 延续）：sfizz + 免费管弦库作为第三渲染后端，服务音质升级；商业音源适配器持续 HOLD。
- **Story 层约定文档**：Agent 如何把游戏世界状态翻译成 scene DSL，以文档+示例沉淀（`docs/` 或 examples 注释），不产生代码。
- **Story/character 字段二审维持 REJECT**（2026-07 创作实战复核）：`character: { regret: 0.9 }` 与 Story 层同病——无确定编译语义。实战证据：同一 DSL 下三版电影配乐（v1 氛围→v3 叙事）的"人物感"提升全部来自 Agent 创作 brief 的质量，DSL 零改动。人物主题的正确载体是 motif 复用（同一动机跨场景出现即人物签名，已支持）与 brief 约定，不是 schema 字段。

## 完成的样子

> 当以下可观察结果全部出现时，对应阶段才算达成；手段（测试框架、目录结构）由执行者自选。

- 一条命令链把 `scene.yaml` 变成 `scene.ogg`，在干净环境（CI）中可重复跑通。
- 同一输入连续编译两次，MIDI 字节级相同；音频时长、样本数、RMS 在既定容差内相同（golden test 挡住回归）。
- 生成的 loop 在游戏引擎中循环播放听不出接缝；用样本数断言可自动验证长度精确。
- stems 数量与 DSL 轨道一致、逐一等长；叠加混合与 full mix 的差异在容差内。
- 给 Agent 一份 schema 与一条报错信息，它能在不读源码的情况下修正非法 DSL——错误信息含行号与字段路径。
- 缺少外部依赖（fluidsynth/ffmpeg/音源文件）时，命令以清晰的机读错误退出，不产出半成品文件。

## 验收矩阵（业务能力覆盖矩阵）

> 覆盖底线（硬性规定，不得降级）：
>
> 1. 每个一级功能至少有一条 Happy Path E2E。
> 2. 每个高风险功能至少覆盖一条失败路径。
> 3. 每个涉及权限的功能至少验证两种角色。
> 4. 每个会修改系统状态的操作至少验证一次失败后的恢复或回滚。
> 5. 每次新增一级业务功能，必须同步新增对应的 E2E 并更新本矩阵。

权限说明：scorekit 为本地单用户 CLI，无角色/权限体系，权限列整体"不适用"。状态变更指写出文件；其恢复要求为：失败时不得留下损坏的半成品产物（如临时文件 + 原子重命名），并以非零码退出。

| 一级功能 | 风险级别 | Happy Path E2E | 失败路径 | 权限角色覆盖 | 失败恢复/回滚 | 证据（测试路径/用例） |
| --- | --- | --- | --- | --- | --- | --- |
| DSL 校验与 schema 导出（validate/schema） | 低 | ✅ | ✅ | 不适用（本地 CLI） | 不适用（只读） | `tests/cli.rs::validate_happy_path` / `validate_rejects_unknown_field_with_location` / `validate_rejects_semantic_error_with_field_path_json` / `schema_emits_json_schema` |
| MIDI 生成（midi） | 中 | ✅（golden 字节比对 + 双跑确定性） | ✅ | 不适用（本地 CLI） | ✅（失败无半成品） | `tests/cli.rs::midi_matches_golden_bytes` / `midi_is_deterministic_across_runs` / `midi_invalid_scene_leaves_no_partial_file` |
| 音频渲染（render，FluidSynth+SF2） | 高（外部进程+写文件） | ✅（断言采样率与时长） | ✅（损坏 SF2 / 非 SF2 文件 / fluidsynth 缺失） | 不适用（本地 CLI） | ✅（失败目录无残留） | `tests/cli.rs::render_happy_path_produces_exact_rate_wav` / `render_corrupt_soundfont_fails_without_partial_output` / `render_text_file_as_soundfont_is_input_error` / `render_missing_fluidsynth_is_dependency_error` |
| 导出与 loop 元数据（export/build，FFmpeg） | 高（外部进程+写文件） | ✅（loop 精确 L 样本 + 位级无缝封口 + meta.json + 非 loop 定长） | ✅ | 不适用（本地 CLI） | ✅（失败目录无残留） | `tests/cli.rs::export_happy_path_produces_ogg` / `export_missing_input_is_input_error` / `export_seek_take_cuts_bit_exactly` / `build_full_chain_scene_to_ogg` / `build_loop_wav_is_sample_exact_and_sealed` / `build_nonloop_wav_has_exact_padded_length` |
| 分轨渲染（stems） | 中 | ✅（4 轨等长 + 求和≈全混音 RMS<2%） | ✅（损坏 SF2） | 不适用（本地 CLI） | ✅（staging 目录原子交换，失败无 stems 残留） | `tests/cli.rs::build_stems_are_aligned_and_sum_to_mix` / `build_corrupt_soundfont_leaves_no_partial_output_or_stems` |
| 结构化乐曲与过渡（sections/motif） | 中 | ✅（suite 逐段资产 + 精确段长 + manifest + tempo 覆盖 + mute） | ✅（未知 motif 引用 / 重复段名 / 全静音段 / 未知 --section） | 不适用（本地 CLI） | ✅（失败目录无残留，复用 build 原子机制） | `tests/cli.rs::build_suite_emits_per_section_assets_with_exact_lengths` / `midi_section_selector_compiles_that_section_deterministically` / `midi_unknown_section_is_input_error` / `validate_rejects_unknown_motif_reference` / `validate_rejects_duplicate_section_names_and_mute_all` / `example_suite_validates` |
| 第二渲染后端（--renderer，M3） | 中 | ✅（timidity 全链路：同一 DSL 相同精确长度、不同音色、非静音） | ✅（坏 SF2 header-only WAV 零帧兜底 exit 4；缺失 SF2 exit 2） | 不适用（本地 CLI） | ✅（失败即删除半成品，目录无残留） | `tests/cli.rs::build_timidity_backend_same_length_different_timbre` / `render_timidity_corrupt_soundfont_fails_without_partial_output` / `render_timidity_missing_soundfont_is_input_error` |
| 语义 diff（diff，M4） | 低（只读） | ✅（语义变化 porcelain + `--json` 数组；格式/键序差异 diff 为空） | ✅（非法场景 exit 2） | 不适用（本地 CLI） | 不适用（只读，无状态变更） | `tests/cli.rs::diff_reports_semantic_changes_and_ignores_formatting` / `diff_invalid_scene_is_input_error` |
| 批量渲染与机读报告（batch，M4） | 高（外部进程+批量写文件） | ✅（多场景全建成 + 逐场景精确长度 + report.json 统计） | ✅（单场景失败不中断、report 记录 error、exit 反映首个失败；输出名冲突构建前拒绝） | 不适用（本地 CLI） | ✅（失败场景无半成品产物，成功场景产物完整保留） | `tests/cli.rs::batch_builds_all_scenes_and_writes_report` / `batch_partial_failure_reports_and_exits_nonzero` / `batch_duplicate_scene_stems_is_input_error` |
| 演奏表现层（performance，M5） | 中 | ✅（同种子字节级复现、异种子不同；humanize+swing 下 loop 仍样本级精确） | ✅（swing 超界 exit 2 + 字段路径） | 不适用（本地 CLI） | 不适用（纯计算，复用 midi/build 原子机制） | `tests/cli.rs::performance_same_seed_is_byte_identical_different_seed_differs` / `performance_build_keeps_loop_sample_exact` / `validate_rejects_bad_swing_and_bad_numeral` |
| 和声进行声明（harmony，M5） | 低 | ✅（自定义进行改变音符且总长不变） | ✅（非法罗马数字 exit 2 + `harmony[i]` 路径） | 不适用（本地 CLI） | 不适用（纯计算） | `tests/cli.rs::harmony_changes_notes_at_same_length` / `validate_rejects_bad_swing_and_bad_numeral` |
| 音乐语法校验（lint/schema --grammar，M6） | 低（只读） | ✅（出厂 dunes×grief 参照对全过；`schema --grammar` 导出档案 Schema） | ✅（违规带实测值 exit 2 + `--json` violations 数组；深度规则从编译后 IR 实测；空规则档案 exit 2） | 不适用（本地 CLI） | 不适用（只读，无状态变更） | `tests/cli.rs::lint_shipped_scene_conforms_to_shipped_grammar` / `lint_reports_violations_with_measured_values` / `lint_measures_rest_ratio_from_compiled_ir` / `lint_rejects_grammar_without_rules` / `schema_grammar_flag_emits_grammar_schema` |

矩阵当前无已知缺口；新增一级功能时按覆盖底线第 5 条同步登记。
