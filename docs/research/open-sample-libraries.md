# scorekit 开放音色库补充调研

> 核验日期：2026-07-18。本文只做候选研究，不表示已经通过 `sfizz_render` 实机验收。

## 结论

当前组合（VSCO 2 CE + VCSL + Virtual Playing Orchestra 3 WAV）已经覆盖管弦乐主体、钢琴、竖琴和定音鼓。最值得继续补的是：真实原声鼓、电吉他、电贝司、真正的电钢琴，以及合成器 lead/pad/bass。不要再优先堆另一套综合管弦乐库。

建议按以下顺序验证并接入：

1. **P0：FreePats CC0 合成器单音色 + Virtuosity Drums**，先补齐当前 profile 中以管弦乐替代的 synth/drums。
2. **P0：Black And Green Guitars + Black And Blue Basses**，补成套节奏组。
3. **P1：Greg Sullivan's E-Pianos**，价值很高，但先做 ARIA opcode 兼容验收。
4. **P1：MuldjordKit / FreePats Classical Guitar**，作为小体积、低门槛备选。
5. **P2：Legato Vocal Tutorial**，只能作为人声技术样本，不能宣称完整 choir。

## 许可分类口径

- **OSI 开源许可**主要是软件许可口径。SFZ 映射代码可以使用 GPL/MIT 等 OSI 许可，但录音样本通常是文化作品，不能仅凭仓库“open source”字样推断样本可再分发。
- **CC0 / CC BY 开放采样**允许复制、修改和再分发；CC BY 必须保留署名和许可证信息。它们适合 scorekit 的外部音源/profile 模式，但不是 OSI 软件许可证。
- **仅免费使用**不等于可再分发。CC BY-NC、厂商 EULA 或“可免费作曲但不可打包样本”都不应成为默认可下载依赖。
- scorekit 最稳妥的交付方式仍是只提交 profile、下载说明、固定版本和校验值，不把数百 MB/GB 样本直接放进仓库。CC0 也不改变这一工程选择。

## sfizz 共同风险

scorekit 当前构建脚本固定 sfizz 1.2.3。sfizz 官方 1.2.3 README 同时链接 SFZ v1、v2 和 ARIA 支持矩阵；当前官方状态页显示的覆盖率是 SFZv1 96%、SFZv2 44%、ARIA 45%，因此“文件扩展名是 `.sfz`”不等于能无损运行。尤其要警惕 ARIA 扩展、深层 `#include`、keyswitch/CC 状态、release trigger、脚踏板噪声、复杂 mic mixer 和随机轮转。

来源：[sfizz 1.2.3 README](https://github.com/sfztools/sfizz/blob/1.2.3/README.md) · [sfizz opcode 支持矩阵](https://sfz.tools/sfizz/development/status/opcodes/)

每个候选进入 profile 前至少验证：加载无 unsupported-opcode 警告、常用音域非静音、最低/最高 velocity、重复音 round-robin、note-off/release、64 号踏板（若有）、44.1/48 kHz 离线渲染，以及相同输入连续两次渲染满足项目已有的音频确定性容差（能做到时再比较 PCM 哈希）。复杂 keyswitch patch 不应直接作为 scorekit 的 `sustain` 映射，优先选单奏法 patch。

## 候选短名单

### P0-1 FreePats CC0 合成器单音色组

- **许可**：各页面明确标为 CC0 1.0；属于开放采样，不是 OSI 软件许可，可修改和再分发且无需署名。
- **格式与内容**：官方直接提供 SFZ+FLAC、SFZ+WAV 和 SF2。可直接补 `square_lead`、`synth_bass`、`synth_strings`、`warm_pad`、`choir_pad`、`bowed_pad`、`sweep_pad`；单包约 1–18 MiB，远小于完整综合库。
- **维护/下载**：页面版本跨度 2019–2024；Lately Bass 更新到 2024-04-09，下载仍由 FreePats 官方站直接提供。它更像稳定资产目录，不是高频开发项目。
- **互补价值**：这是对现有管弦乐替代映射最直接、最小的修复。当前 VSCO2 profile 把 lead/pad 映射为长笛、弱弦或弱音铜管，而这些文件就是对应的合成器音色。
- **sfizz 风险：低到中**。独立、小型 SFZ 比大型 GUI/ARIA bank 更容易预检；但仍未在 scorekit 固定的 1.2.3 上实测。
- **官方来源**：[Synth Lead（含 Square）](https://freepats.zenvoid.org/Synthesizer/synth-lead.html) · [Synth Bass](https://freepats.zenvoid.org/Synthesizer/synth-bass.html) · [Synth Strings](https://freepats.zenvoid.org/Synthesizer/synth-strings.html) · [Synth Pad](https://freepats.zenvoid.org/Synthesizer/synth-pad.html) · [FreePats 许可说明](https://freepats.zenvoid.org/licenses.html)

### P0-2 Virtuosity Drums

- **许可**：仓库 LICENSE 为 CC0 1.0，样本与映射可开放再分发。
- **格式与内容**：原生 SFZ；现场/爵士取向的原声鼓，六个可混音麦克风位置。kick、snare、toms、hi-hat、ride/crash 有多种击法，另含 cowbell、bongo、conga、timbale、agogo、cabasa、shaker、guiro、triangle 等辅助打击乐。
- **维护/下载**：官方 release `v0.925` 发布于 2026-04-21，提供完整 ZIP；本次核验时仓库非归档，属于短名单里下载状态最健康的一项。
- **互补价值**：VPO 官方示例明确注明 drum set 不包含在库中；VSCO2 的 GM-style percussion 也不是完整、深采样的现代鼓组。它能真正承担 `drums`，而不是再做替代。
- **sfizz 风险：中**。SFZ 树包含多个 mic、keymap、roll/hi-hat choke 和大量 include；先用 `01-basic-kit.sfz`，不要从 full/multi-mic patch 起步。
- **官方来源**：[项目与奏法清单](https://github.com/sfzinstruments/virtuosity_drums) · [CC0 LICENSE](https://github.com/sfzinstruments/virtuosity_drums/blob/master/LICENSE) · [v0.925 下载](https://github.com/sfzinstruments/virtuosity_drums/releases/tag/v0.925)

### P0-3 Black And Green Guitars

- **许可**：仓库 LICENSE 为 CC0 1.0；README 另明确允许商业和非商业使用。
- **格式与内容**：原生 SFZ，采样自 Gretsch Anniversary 与 Hofner Club 两把电吉他；提供 twang、staccato、hammer-on、behind-the-bridge、feedback/noise 及组合 patch。
- **维护/下载**：稳定版 `v1.000` 发布于 2022-07-21。README 明确提醒 GitHub “Code → ZIP” 会破坏 Sforzando bank XML 签名，应从 Releases 下载；对 scorekit 来说 release ZIP 也是唯一应记录的入口。
- **互补价值**：可覆盖 `electric_guitar`，staccato 可作为 `muted_guitar` 的近似；比当前用管弦乐音色代替吉他更符合游戏节奏组。
- **sfizz 风险：中**。库面向 Plogue Sforzando，使用多层 include、控制曲线和 keyswitch；优先验证 `04-green_twang.sfz` / `05-green_staccato.sfz` 这类单奏法 patch，避免 keyswitch bank。
- **官方来源**：[项目与 README](https://github.com/sfzinstruments/karoryfer.black-and-green-guitars) · [CC0 LICENSE](https://github.com/sfzinstruments/karoryfer.black-and-green-guitars/blob/main/LICENSE) · [v1.000 下载](https://github.com/sfzinstruments/karoryfer.black-and-green-guitars/releases/tag/v1.000)

### P0-4 Black And Blue Basses

- **许可**：仓库 `license` 为 CC0 1.0。
- **格式与内容**：原生 SFZ；包含 Darkblack 与 Babyblue 两套电贝司素材，现成 patch 覆盖普通 pluck、warm、ghost、staccato、behind-the-bridge 等，适合映射 `bass`、`picked_bass`，经听感验收后再决定能否近似 `fretless_bass`。
- **维护/下载**：`v1.002` 发布于 2024-12-04，官方 release 有完整 ZIP；本次核验时仓库非归档。
- **互补价值**：当前 hybrid profile 仍以低音提琴替代 bass，以低音弦拨奏替代 synth bass；该库能首先解决真实电贝司缺口。
- **sfizz 风险：中**。patch 使用 controls/maps include、滤波和可能的双轨逻辑；先选 `05-darkblack_pluck.sfz` 这类单一 patch，不用 all/keyswitch 版。
- **官方来源**：[项目](https://github.com/sfzinstruments/karoryfer.black-and-blue-basses) · [CC0 LICENSE](https://github.com/sfzinstruments/karoryfer.black-and-blue-basses/blob/main/license) · [v1.002 下载](https://github.com/sfzinstruments/karoryfer.black-and-blue-basses/releases/tag/v1.002)

### P1-1 Greg Sullivan's E-Pianos

- **许可**：仓库 README 和 LICENSE 明确为 CC BY 3.0；允许再分发和商业使用，但必须署名、保留许可证并标注修改。
- **格式与内容**：FLAC + SFZ v2/ARIA 映射，包含 Yamaha CP80 Electric Grand、Hohner Pianet T、Wurlitzer EP200。它们是实际电钢琴，明显优于当前用 VCSL TX81Z FM Piano 近似 `epiano`。
- **维护/下载**：仓库未归档，但无正式 release；内容最后一批提交在 2020 年，需从仓库获取并自行固定 commit/checksum。
- **互补价值**：目标非常准确，一次补三种电钢琴色彩；不与 VCSL Steinway B 重复。
- **sfizz 风险：高**。官方 README 明说使用 “SFZ v2 with ARIA extensions”，而 sfizz 的 SFZv2/ARIA 覆盖并不完整。必须逐个 patch 听测 release、pedal、控制器与静音音域，通过后才能列入默认 profile。
- **官方来源**：[项目、格式与许可说明](https://github.com/sfzinstruments/GregSullivan.E-Pianos) · [CC BY 3.0 LICENSE](https://github.com/sfzinstruments/GregSullivan.E-Pianos/blob/master/LICENSE)

### P1-2 FreePats MuldjordKit

- **许可**：CC BY 4.0，允许修改、商业使用和再分发，但需要署名并说明修改。
- **格式与内容**：官方提供 SFZ+FLAC（157 MiB）、SFZ+WAV（223 MiB）、Hydrogen 与 SF2；两只 kick、三只 hanging tom、一只 floor tom、snare、hi-hat、多只 crash/ride/china，含 velocity layers 与随机样本。
- **维护/下载**：FreePats 版本为 2020-10-18，官方直链仍可下载；稳定但较久未更新。
- **互补价值**：是 Virtuosity Drums 的更小、更简单备选，适合先建立 `drums` 的可重复渲染基线；若音色满足需求，不必一开始承担六麦大型库的复杂度。
- **sfizz 风险：低到中**。FreePats 版已经简化成 stereo kit，但随机层意味着必须专门核验离线渲染是否由输入 MIDI/固定引擎状态决定，而不是运行时随机种子。
- **官方来源**：[FreePats Acoustic Drum Kit 页面与下载](https://freepats.zenvoid.org/Percussion/acoustic-drum-kit.html) · [FreePats 许可说明](https://freepats.zenvoid.org/licenses.html)

### P1-3 FreePats Spanish Classical Guitar

- **许可**：CC0 1.0。
- **格式与内容**：尼龙弦古典吉他，提供 SFZ+FLAC（4.5 MiB）、SFZ+WAV（6.9 MiB）和 SF2。
- **维护/下载**：版本 2019-06-18，官方直链仍可下载。页面坦承原始录音条件不理想，做过降噪和滤波。
- **互补价值**：用极小体积补 `guitar` 的原声基线，适合测试/轻量发行；不能替代高质量钢弦吉他，也不应夸大为完整 guitar family。
- **sfizz 风险：低**。小型单乐器、官方直接给 SFZ；主要风险是音质而非 opcode 复杂度。
- **官方来源**：[项目、许可与下载](https://freepats.zenvoid.org/Guitar/acoustic-guitar.html) · [FreePats 许可说明](https://freepats.zenvoid.org/licenses.html)

### P2-1 Legato Vocal Tutorial

- **许可**：仓库 LICENSE 为 CC0 1.0。
- **格式与内容**：SFZ + 样本，来自 Karoryfer Hadzi-Fia 的单个 `a` 元音；有 polyphonic、basic monophonic、no-unison legato、complete legato 等教学 patch。
- **维护/下载**：`v0.100` 发布于 2025-09-09，官方 release ZIP 可用；仓库说明仍可能继续增加示例。
- **互补价值**：可以验证 `voice`/`choir_pad` 的真实人声方向和 legato 技术，但它不是多元音、男女声部齐全的 choir。VPO 本身已有 Vocals 目录，应先盘点现有 WAV 再决定是否采用。
- **sfizz 风险：中到高**。教程的价值恰恰在复杂 legato/CC/include；scorekit 当前 articulation 只选文件，不会主动发送这些控制状态。只能先取最简单的 polyphonic patch 做实验。
- **官方来源**：[项目说明](https://github.com/sfzinstruments/legato_vocal_tutorial) · [CC0 LICENSE](https://github.com/sfzinstruments/legato_vocal_tutorial/blob/main/LICENSE) · [v0.100 下载](https://github.com/sfzinstruments/legato_vocal_tutorial/releases/tag/v0.100)

## 暂不纳入短名单

| 项目 | 分类 | 暂不采用的原因 | 官方证据 |
| --- | --- | --- | --- |
| FreePats General MIDI set | GPLv3+特殊例外；GPL 是 OSI 许可，但这里用于声音集合 | 2022-10-26 版只含 45 个 melodic entries、43 个 percussion entries；官方还说明各样本可能带其他 GPL-compatible 许可。可研究，但比按 CC0/CC BY 单项选材更难做清晰物料清单 | [项目与下载](https://freepats.zenvoid.org/SoundSets/general-midi.html) · [GPL exception 原文](https://freepats.zenvoid.org/licenses.html#GPL_exception) |
| Discord SFZ General MIDI Bank | 计划只收 CC0/CC BY 等开放许可，但当前逐乐器许可 | README 仍标为 Work in progress；没有正式 release，且承诺中的统一 license list 尚未提供。不能把“未来会开放”当成当前可审计交付物 | [官方仓库 README](https://github.com/sfzinstruments/Discord-SFZ-GM-Bank) |
| jRhodes3d | 样本 CC BY-NC 4.0；控制文件 CC0；创作输出可自由使用 | 免费作曲不等于样本可商用再分发。官方 LICENSE 明确：把样本用于商业产品需另行联系授权，不适合作为 scorekit 默认可再分发依赖 | [官方仓库](https://github.com/sfzinstruments/jlearman.jRhodes3d) · [许可边界](https://github.com/sfzinstruments/jlearman.jRhodes3d/blob/master/LICENSE) |
| Philharmonia Orchestra sound samples | 免费用于个人/商业作品，但不是开放采样许可 | 官方允许把声音用于创作，却明确禁止原样或作为 sampler instrument 出售/提供；也没有官方 SFZ 包。典型的“免费使用不等于可再分发” | [官方样本页与使用条件](https://philharmonia.co.uk/resources/sound-samples/) |
| Salamander Grand Piano v3 | CC BY 3.0 开放采样 | 许可合格，但与 VCSL Steinway B 重叠，补缺价值低；官方 README 明示大量使用 SFZ v2 + ARIA extensions，且警告非 ARIA sampler 可能出问题 | [官方仓库、兼容说明](https://github.com/sfzinstruments/SalamanderGrandPiano) · [CC BY 3.0 LICENSE](https://github.com/sfzinstruments/SalamanderGrandPiano/blob/master/LICENSE) |
| Virtual Playing Orchestra 3 本体 | 混合许可，不是整库 CC0 | 当前已在库中。官方说明再打包/再分发需逐来源处理，且主张衍生库仅个人使用或免费发布；它适合作为现有外部依赖，不适合作为“统一开放许可”的再分发底座 | [官方项目、格式、sfizz 支持与许可](https://virtualplaying.com/virtual-playing-orchestra/) |

## 接入验收顺序

短名单不是直接写 profile 的授权。真正接入时按下列顺序，一项失败就不进入默认示例：

1. 固定官方 release/commit、记录下载 URL、文件大小、SHA-256 和许可证副本。
2. 对目标单奏法 `.sfz` 运行 opcode 扫描，再用 scorekit 固定的 sfizz 1.2.3 实际渲染。
3. 使用同一段覆盖音域、velocity、重复音、note-off 和 pedal 的 MIDI 连续渲染两次，按项目现有音频容差比较（可 bit-exact 时再固定 PCM 哈希）；随机层造成不可接受漂移时直接淘汰或制作确定性简化映射。
4. 只把通过测试的具体 patch 写入 profile；不要因为同一库里一个 patch 成功就宣称整库兼容。
5. CC BY 项目随 profile 增加 `NOTICE`/attribution；CC0 也保留来源、版本和许可证以便供应链审计。
