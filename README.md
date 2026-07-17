# scorekit

Agent 驱动的游戏配乐编译器：文本 DSL（YAML）→ MIDI → 可替换渲染后端（FluidSynth/TiMidity++ + SF2）→ FFmpeg 后处理 → 游戏可用音频资产（无缝 loop、分轨 stems、场景过渡）。

> 状态：M0–M3 已完成（全链路 + 无缝 loop + stems + suite 段落/motif + 双渲染后端）。项目画像、非目标（铁律）与路线图见 [docs/roadmap.md](docs/roadmap.md)。

```text
scene.yaml ─► validate ─► midi ─► render ─► export ─► scene.ogg + stems/
```

## 快速开始

依赖：Rust、FluidSynth（`brew install fluid-synth` / `apt install fluidsynth`）、FFmpeg；可选 TiMidity++（第二渲染后端，`--renderer timidity`）。

```bash
./scripts/fetch_assets.sh              # 下载测试用 GM SoundFont 到 assets/
cargo build --release

# 校验场景 → 一条命令直出游戏资产（无缝 loop + 分轨 + 元数据）
./target/release/scorekit validate examples/scenes/forest.yaml
./target/release/scorekit build examples/scenes/forest.yaml \
    --soundfont assets/TimGM6mb.sf2 -o forest.ogg --stems
# 产出：forest.ogg（精确 loop 长度、环回无缝）
#      forest.stems/01-strings.ogg … 04-drums.ogg（与全混音样本对齐，可动态分层）
#      forest.meta.json（loop_samples/total_samples/stems 清单，供引擎与 Agent 消费）

# Suite：一个场景文件 → intro/explore/combat/victory 多个资产，共享主题动机
./target/release/scorekit build examples/scenes/forest_suite.yaml \
    --soundfont assets/TimGM6mb.sf2 -o forest.ogg
# 产出：forest-intro.ogg forest-explore.ogg(loop) forest-combat.ogg(loop, 132bpm)
#      forest-victory.ogg + forest.meta.json（suite manifest）
./target/release/scorekit midi examples/scenes/forest_suite.yaml \
    --section combat -o combat.mid   # 单独编译某一段

# 分步执行（--renderer timidity 可切换第二后端，产物结构与长度不变）
./target/release/scorekit midi examples/scenes/forest.yaml -o forest.mid
./target/release/scorekit render forest.mid --soundfont assets/TimGM6mb.sf2 -o forest.wav
./target/release/scorekit export forest.wav -o forest.ogg

# Agent 集成
./target/release/scorekit schema       # DSL 的 JSON Schema
./target/release/scorekit --json validate scene.yaml   # 机读错误（stderr JSON）
```

退出码：`0` 成功 · `1` IO · `2` 输入非法 · `3` 依赖缺失 · `4` 外部工具失败。

场景 DSL 示例见 [examples/scenes/forest.yaml](examples/scenes/forest.yaml)（单场景）与 [examples/scenes/forest_suite.yaml](examples/scenes/forest_suite.yaml)（含 motifs/sections 的组曲）。

无缝 loop 的原理：loop 场景渲染两遍取第二遍 `[L, 2L)`（开头自带上一遍的混响尾音），`L` 由量化后的 MIDI tempo 精确推导；因 FluidSynth 按毫秒调度、真实周期存在漂移，尾部再做短交叉淡化封口，使终帧与环回目标位级衔接（`--crossfade-ms` 可调，默认 50ms）。

设计立场：

- 确定性压倒一切——同一输入必须产出可复现结果，这是 Agent 回归测试的地基。
- 一切皆文本——DSL 由 git 原生 diff/merge/回滚，不自研版本控制。
- 薄编排——不自研 DSP，不做 GUI/DAW，不内嵌作曲智能。

## 质量与验收

所有一级功能受 [docs/roadmap.md 验收矩阵](docs/roadmap.md#验收矩阵业务能力覆盖矩阵) 约束：每个一级功能必须有 Happy Path E2E，高风险功能必须覆盖失败路径，写文件操作必须验证失败后不留半成品；新增一级功能必须同步更新矩阵。详细硬性规定见 [AGENTS.md](AGENTS.md)。

## License

[MIT](LICENSE)
