# scorekit Agent 规范

项目画像、路线图与完整验收矩阵见 [docs/roadmap.md](docs/roadmap.md)。动手前先读它：项目概述讲清了 scorekit 是什么，非目标（铁律）讲清了绝不做什么。

## 核心边界（MUST）

- MUST NOT 自研 DSP（合成/混响/压缩），渲染与后处理一律委托外部工具（FluidSynth、FFmpeg 等）。
- MUST NOT 实现版本控制命令（commit/merge/branch）；版本控制交给 git，DSL 必须保持 diff 友好的稳定文本格式。
- MUST 保证确定性：同一 DSL + 同一音源 + 同一版本工具，MIDI 输出字节级一致。
- DSL schema MUST NOT 引入仅商业音源可实现的字段。

## 验收矩阵（硬性规定）

以下五条为 MUST 级规则，矩阵本体维护在 [docs/roadmap.md](docs/roadmap.md) 的"验收矩阵"章节（单一事实来源，此处不复制内容）：

1. 每个一级功能至少有一条 Happy Path E2E。
2. 每个高风险功能至少覆盖一条失败路径。
3. 每个涉及权限的功能至少验证两种角色（本项目当前无权限体系，整体"不适用"，若未来引入则本条生效）。
4. 每个会修改系统状态（写文件）的操作至少验证一次失败后的恢复：失败不得留下损坏的半成品产物。
5. 新增一级业务功能时，MUST 同步新增对应 E2E 并更新 docs/roadmap.md 的验收矩阵，否则变更不完整。
