# scorekit

**ScoreKit is an Agent-oriented Music Compiler, not an AI music generator.**

Agent 驱动的游戏配乐编译器：文本 DSL（YAML）→ MIDI → 可替换渲染后端（FluidSynth/TiMidity++ + SF2）→ FFmpeg 后处理 → 游戏可用音频资产（无缝 loop、分轨 stems、场景过渡）。它把高级音乐语义稳定地编译为可执行资产；创作智能永远属于上游 Agent。

> 状态：M0–M6 全部完成（全链路 + 无缝 loop + stems + suite + 双后端 + Agent 工作流 + 演奏表现层/和声声明 + 音乐语法校验）。项目画像、非目标（铁律）与路线图见 [docs/roadmap.md](docs/roadmap.md)。

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
./target/release/scorekit diff old.yaml new.yaml       # 语义 diff（--json 出 JSON 数组）
./target/release/scorekit batch a.yaml b.yaml --soundfont game.sf2 \
    --out-dir assets/   # 批量渲染；逐场景结果写 assets/report.json，失败不中断

# 审美回归测试：场景是否符合项目的"音乐语法"
./target/release/scorekit lint examples/scenes/dunes.yaml \
    --grammar examples/grammars/grief.yaml
./target/release/scorekit schema --grammar   # 语法档案的 JSON Schema
```

退出码：`0` 成功 · `1` IO · `2` 输入非法 · `3` 依赖缺失 · `4` 外部工具失败。

去掉"AI 味"——`harmony` 声明和声进行（所有伴奏轨一起变），`performance` 做确定性演奏渲染（同种子同字节）：

```yaml
harmony: [i, iv, VI, v]          # 罗马数字，一小节一个和弦，循环
performance:
  humanize: { timing_ms: 18, velocity: 10, seed: 7 }  # 种子化力度/时值抖动
  legato: true                   # 旋律音衔接
  swing: 0.12                    # 摇摆律动（0..0.5）
  dynamics: { start: pp, peak: mf }  # 力度弧线，首尾同级、loop 安全
```

沉淀"审美"——语法档案（grammar）把项目的风格体系写成可校验断言，`lint` 从编译后的乐谱实测（不是看 YAML 表面），违规带实测值，Agent 直接照改：

```yaml
# examples/grammars/grief.yaml — 孤独在这个项目里的样子
name: grief
rules:
  tempo_max: 60                  # 悲伤不奔跑
  pads_max: 1
  melodic_voices_max: 2          # 同时说话的声音峰值
  melody_rest_ratio_min: 0.35    # 每个声部自己的呼吸（逐轨实测）
  phrase_min_beats: 5            # 不允许碎句
  resolution: incomplete         # 结尾不落主音——问题保持敞开
  harmony_allowed: [i, iv, v, VI, VII]
  require_performance: true      # 必须像人演奏
```

```text
$ scorekit lint examples/scenes/forest.yaml --grammar examples/grammars/grief.yaml
tempo_max @ scene: measured 92, want <= 60
require_performance @ scene: measured absent, want a `performance` block
error[lint]: 4 grammar violation(s) against `grief`   # exit 2
```

场景 DSL 示例见 [examples/scenes/](examples/scenes/)：[forest.yaml](examples/scenes/forest.yaml)（单场景）、[forest_suite.yaml](examples/scenes/forest_suite.yaml)（含 motifs/sections 的组曲），以及六种风格参考——[chiptune.yaml](examples/scenes/chiptune.yaml)（8-bit 游戏）、[dance.yaml](examples/scenes/dance.yaml)（动感舞曲）、[epic.yaml](examples/scenes/epic.yaml)（轻史诗）、[ballad.yaml](examples/scenes/ballad.yaml)（3/4 抒情）、[elegy.yaml](examples/scenes/elegy.yaml)（小提琴挽歌）、[dunes.yaml](examples/scenes/dunes.yaml)（电影配乐：单动机对话式织体，符合 [grief 语法](examples/grammars/grief.yaml)）。一次全部渲染：

```bash
./target/release/scorekit batch examples/scenes/*.yaml \
  --soundfont assets/TimGM6mb.sf2 --out-dir out/
```

无缝 loop 的原理：loop 场景渲染两遍取第二遍 `[L, 2L)`（开头自带上一遍的混响尾音），`L` 由量化后的 MIDI tempo 精确推导；因 FluidSynth 按毫秒调度、真实周期存在漂移，尾部再做短交叉淡化封口，使终帧与环回目标位级衔接（`--crossfade-ms` 可调，默认 50ms）。

设计立场：

- 确定性压倒一切——同一输入必须产出可复现结果，这是 Agent 回归测试的地基。
- 一切皆文本——DSL 由 git 原生 diff/merge/回滚，不自研版本控制。
- 薄编排——不自研 DSP，不做 GUI/DAW，不内嵌作曲智能。

## Agent 技能（第三方安装）

[skills/scorekit/](skills/scorekit/) 是可安装的 Agent 技能（`SKILL.md` 格式）：让任何支持技能的编码 Agent 学会完整的 scorekit 创作工作流——写 scene DSL、validate/lint 闭环、build 出游戏资产，附实战作曲经验与完整 DSL 参考（[reference.md](skills/scorekit/reference.md)）。

```bash
npx skills add talkincode/scorekit           # skills CLI 生态
# 或手动复制到你的 Agent 技能目录，例如：
cp -r skills/scorekit ~/.claude/skills/      # Claude Code
cp -r skills/scorekit ~/.agents/skills/      # 通用 agents 目录
```

## 质量与验收

所有一级功能受 [docs/roadmap.md 验收矩阵](docs/roadmap.md#验收矩阵业务能力覆盖矩阵) 约束：每个一级功能必须有 Happy Path E2E，高风险功能必须覆盖失败路径，写文件操作必须验证失败后不留半成品；新增一级功能必须同步更新矩阵。详细硬性规定见 [AGENTS.md](AGENTS.md)。

## License

[MIT](LICENSE)
