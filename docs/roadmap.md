# scorekit 项目画像与方向

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

- **仓库骨架**：仅 MIT LICENSE 与 Rust 向 .gitignore（`/.gitignore` 含 Cargo/rustfmt 条目，Rust 选型由此确立）。**尚无任何代码、文档与测试。**
- **本机环境（待核验，非仓库能力）**：ffmpeg 已安装（`/opt/homebrew/bin/ffmpeg`）；fluidsynth 未安装；无 SF2 音源。

## 非目标（铁律）

- **不自研 DSP。** 不写合成器、混响、压缩、重采样；一律委托 FluidSynth/FFmpeg 等外部工具。违反即"研究信号处理直到退休"。
- **不做 GUI / DAW / 时间轴编辑器。** scorekit 是编译器，不是 GarageBand。
- **不自研版本控制。** 不实现 `score commit/merge/branch`。版本控制交给 git；scorekit 至多提供 git 之上的只读语义 diff 展示。
- **不内嵌作曲智能。** 不调用 LLM、不做生成模型；作曲创意属于上游 Agent，scorekit 只编译结构。
- **不做游戏内实时音频运行时。** 实时混音、动态 stem 淡入淡出的运行时（含 Zig 设想）是另一个项目。
- **核心不绑定商业音源。** Kontakt/BBC 等只能作为渲染后端边界外的适配器接入；DSL schema 不得出现仅商业音源可实现的字段。

## 方向与意图（路线图）

> 阶段表达目标能力，不规定内部实现顺序；每阶段完成的判据见"完成的样子"与验收矩阵。

### M0 — 走通骨架（状态：未开始）

单音轨从 DSL 到可播放文件的全链路：`validate → midi → render(SF2) → export(ogg)`。服务画像中"确定性"与"对 Agent 友好"——golden test 与机读错误从第一天建立。

### M1 — 游戏资产核心（状态：未开始）

多音轨、无缝 loop、样本对齐 stems、轨道强度（intensity）。这是与"能出声的 demo"拉开差距的阶段，服务"产物是游戏资产"画像。

### M2 — 结构化乐曲（状态：未开始）

段落结构（intro/loop A/loop B/combat/victory/failure）、可复用 pattern/motif、段落间过渡。服务"场景共享主题"的游戏叙事需求。

### M3 — 音质升级与后端可替换性验证（状态：未开始）

以第二个渲染后端（优先 sfizz + 免费管弦 SFZ 库）证明渲染边界成立，把音质从 SoundFont 档抬到独立游戏发行档。商业音源适配器持续 HOLD，直至开源路线证实不足。

### M4 — Agent 体验完善（状态：未开始）

JSON Schema 导出、只读语义 diff（`scorekit diff`，git porcelain）、批量渲染与机读报告。服务"Agent 仅凭 schema 与报错即可工作"。

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
| DSL 校验与 schema 导出（validate/schema） | 低 | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | 不适用（只读） | ❌ 缺口 |
| MIDI 生成（midi） | 中 | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | ❌ 缺口 | ❌ 缺口 |
| 音频渲染（render，FluidSynth+SF2） | 高（外部进程+写文件） | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | ❌ 缺口 | ❌ 缺口 |
| 导出与 loop 元数据（export，FFmpeg） | 高（外部进程+写文件） | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | ❌ 缺口 | ❌ 缺口 |
| 分轨渲染（stems） | 中 | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | ❌ 缺口 | ❌ 缺口 |
| 结构化乐曲与过渡（sections/motif） | 中 | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | ❌ 缺口 | ❌ 缺口 |
| 第二渲染后端（sfizz+SFZ，M3） | 中 | ❌ 缺口 | ❌ 缺口 | 不适用（本地 CLI） | ❌ 缺口 | ❌ 缺口 |

各缺口最低期望：

- **validate/schema**：合法样例通过、非法样例报出行号与字段路径（失败路径即核心价值）。
- **midi**：golden 字节比对一条；非法 IR 拒绝生成。失败恢复：不写出部分 MIDI 文件。
- **render**：SF2 渲染出断言时长/样本数的 WAV；fluidsynth 缺失或崩溃时非零退出且无半成品文件。
- **export**：OGG 产物 + loop 样本数断言；ffmpeg 失败时同上。
- **stems**：轨道数、等长、混合等价断言；单轨渲染失败时整体失败而非输出残缺 stem 集。
- **sections/motif**：段落时长与结构断言；引用不存在的 motif/section 时校验期报错。
- **第二后端**：同一 DSL 在两个后端均可渲染成功（音色不同、结构断言一致）。
