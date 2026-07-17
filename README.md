# scorekit

Agent 驱动的游戏配乐编译器：文本 DSL（YAML）→ MIDI → 外部渲染（FluidSynth/SF2 起步）→ FFmpeg 后处理 → 游戏可用音频资产（无缝 loop、分轨 stems、场景过渡）。

> 状态：M0 已完成（全链路可用）。项目画像、非目标（铁律）与路线图见 [docs/roadmap.md](docs/roadmap.md)。

```text
scene.yaml ─► validate ─► midi ─► render ─► export ─► scene.ogg + stems/
```

## 快速开始

依赖：Rust、FluidSynth（`brew install fluid-synth` / `apt install fluidsynth`）、FFmpeg。

```bash
./scripts/fetch_assets.sh              # 下载测试用 GM SoundFont 到 assets/
cargo build --release

# 校验场景 → 一条命令直出 OGG
./target/release/scorekit validate examples/scenes/forest.yaml
./target/release/scorekit build examples/scenes/forest.yaml \
    --soundfont assets/TimGM6mb.sf2 -o forest.ogg

# 分步执行
./target/release/scorekit midi examples/scenes/forest.yaml -o forest.mid
./target/release/scorekit render forest.mid --soundfont assets/TimGM6mb.sf2 -o forest.wav
./target/release/scorekit export forest.wav -o forest.ogg

# Agent 集成
./target/release/scorekit schema       # DSL 的 JSON Schema
./target/release/scorekit --json validate scene.yaml   # 机读错误（stderr JSON）
```

退出码：`0` 成功 · `1` IO · `2` 输入非法 · `3` 依赖缺失 · `4` 外部工具失败。

场景 DSL 示例见 [examples/scenes/forest.yaml](examples/scenes/forest.yaml)。

设计立场：

- 确定性压倒一切——同一输入必须产出可复现结果，这是 Agent 回归测试的地基。
- 一切皆文本——DSL 由 git 原生 diff/merge/回滚，不自研版本控制。
- 薄编排——不自研 DSP，不做 GUI/DAW，不内嵌作曲智能。

## 质量与验收

所有一级功能受 [docs/roadmap.md 验收矩阵](docs/roadmap.md#验收矩阵业务能力覆盖矩阵) 约束：每个一级功能必须有 Happy Path E2E，高风险功能必须覆盖失败路径，写文件操作必须验证失败后不留半成品；新增一级功能必须同步更新矩阵。详细硬性规定见 [AGENTS.md](AGENTS.md)。

## License

[MIT](LICENSE)
