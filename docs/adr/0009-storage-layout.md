# ADR-0009 数据存储位置与布局：OS 标准目录 + 版本化迁移

| | |
| --- | --- |
| 状态 | Accepted |
| 日期 | 2026-07-03 |

## 背景

`kestrel-store` 要落地：会话事件日志（JSONL）、探针生成的模型 profile、用户配置。M1 起步图省事
把会话写在工作目录内（`kestrel.example.toml` 的 `sessions_dir = "sessions"`、`workdir = "."`）。

这是 [foundations.md](../planning/foundations.md) #12，属"现在不定以后很麻烦"：硬编码在工作目录内
会导致每个工作目录各存一份、数据与代码仓库混在一起、跨项目记忆无法共享，以后搬到规范目录还要给
所有人做迁移。用户拍板：用 **OS 标准目录**。

## 决策

### 1. 默认用 OS 标准目录（遵循各平台惯例）

用 `directories` crate（`ProjectDirs`）统一解析，不手写平台分支：

| 平台 | 配置 | 数据 / 会话 | 缓存 |
| --- | --- | --- | --- |
| Windows | `%APPDATA%\Kestrel\` | `%LOCALAPPDATA%\Kestrel\` | `%LOCALAPPDATA%\Kestrel\cache\` |
| Linux | `$XDG_CONFIG_HOME/kestrel`（`~/.config/kestrel`） | `$XDG_DATA_HOME/kestrel`（`~/.local/share/kestrel`） | `$XDG_CACHE_HOME/kestrel` |
| macOS | `~/Library/Application Support/Kestrel/` | 同左 | `~/Library/Caches/Kestrel/` |

### 2. 数据根下的布局

- `sessions/<session-id>.jsonl` —— append-only 事件日志
- `profiles/<model>.toml` —— 探针生成的模型 profile（内置 profile 仍随仓库发在 `profiles/`，
  用户/探针的覆盖落到数据目录，不污染仓库）
- 配置 `kestrel.toml` 在**配置目录**

### 3. 可覆盖（尊重"数据在手边"的用户）

- 环境变量 `KESTREL_DATA_DIR` / `KESTREL_CONFIG_DIR` 与 `--data-dir` flag 显式指定，优先级最高
  （对齐 foundations.md #10 配置优先级）。
- **项目级 `.kestrel/`** 作为 **opt-in**：放在工作目录内、数据随项目走、便于 git 忽略——满足
  "数据就在项目里"的用户，但不是默认。

### 4. 版本化迁移

- 数据目录带 `layout_version` 标记；升级时跑**幂等**迁移钩子。
- 首次运行若检测到旧的 `./sessions` 且新目录为空，提示或自动迁移（可配置），不静默丢数据。

### 5. 安全

数据目录权限收紧到仅当前用户；密钥绝不入数据目录（密钥来自 env / OS store，见
[ADR-0008](0008-i18n-localization.md) 与 [AGENTS §5.1](../../AGENTS.md)）。

## 被否决方案的最强论点（诚实记录）

- **工作目录内 `./sessions`（现状）**：最直观、数据随项目走、便于 git 忽略、"看得见的数据"。
  否决为默认——多工作目录数据碎片化、易误入代码仓库、跨项目记忆无法共享。但**保留为 opt-in**
  （`.kestrel/` + 配置），不是完全放弃这类用户。
- **单一自定义目录不分平台**（如 `~/.kestrel/`）：简单，但不符合 XDG / Windows / macOS 各自惯例，
  备份 / 漫游 / 清理工具不认。用 `directories` crate 几乎零成本拿到规范路径，不值当为省一个依赖破坏惯例。

## 重开条件

- 用户强烈要"数据默认就在项目里" -> `.kestrel/` opt-in 已覆盖；若要成默认再评估。
- 需要多机同步 / 云备份 -> 目录布局已就绪，叠同步层即可。
- 出现 layout 破坏性变更 -> `layout_version` + 迁移钩子已备。
