# 本地 LLM 后端（llama.cpp / LM Studio）Agent 工程调研

调研时间：2026-07。API 字段名与 flag 均核对自官方文档或一手来源。

## 1. llama.cpp server（llama-server）的 agent 相关能力

来源：[tools/server/README.md](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)、[docs/function-calling.md](https://github.com/ggml-org/llama.cpp/blob/master/docs/function-calling.md)、[grammars/README.md](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md)

### 1.1 OpenAI 兼容接口

- `POST /v1/chat/completions`、`/v1/completions`、`/v1/embeddings`。
- chat/completions 支持 `tools` + `tool_choice`（auto / any / 指定）、`parallel_tool_calls: true`、流式 tool call。

### 1.2 原生工具调用（必须 --jinja）

- **--jinja 是开关**：不开则回退旧模板路径，工具定义根本不会正确注入 prompt——前端接 llama.cpp 最常见的翻车点。
- **每模型专用解析器**：Llama 3.x、Qwen 2.5、Hermes 2/3（`<tool_call>` XML）、Mistral Nemo、Functionary、Command R7B、DeepSeek R1（WIP）等；Qwen3-Coder 有自定义 XML 解析器需求（[issue #15012](https://github.com/ggml-org/llama.cpp/issues/15012)）。
- **Generic 回退模式**：模板不识别时日志显示 `Chat format: Generic`，用通用提示词包装，额外耗 token、可靠性下降。
- 可用 `--chat-template-file` 覆盖模板；`GET /props` 可查当前 chat_template。
- **实现机制 lazy grammar**：自由生成直到命中 trigger_tokens/patterns（如 `<tool_call>`），之后才启用 GBNF 约束（[discussion #12110](https://github.com/ggml-org/llama.cpp/discussions/12110)）。用户自带 grammar 与内置 function calling grammar 互斥。
- **重要告警**：KV cache 激进量化（如 `-ctk q4_0`）会**显著劣化工具调用质量**；社区建议 KV 用 Q8 以上（[InsiderLLM guide](https://insiderllm.com/guides/function-calling-local-llms/)）。
- 2026 新增：llama-server 自带内置 agent 工具（`--tools` / `--agent`），upstream 自己也在往 agent 方向走。

### 1.3 约束解码（GBNF / JSON Schema）

- 请求体字段：`grammar`（GBNF 文本）、`json_schema`，或 OpenAI 风格 `response_format: {type: "json_schema", ...}`。
- JSON Schema 到 GBNF 自动转换；**schema 只约束采样、不注入 prompt**（"The model has no visibility into the schema"），想让模型"知道"schema 仍需在 prompt 里描述。
- `additionalProperties` 默认 false（生成更快的 grammar 且减少幻觉字段）。
- 性能坑：`x? x? x?...` 式重复写法导致极慢采样，应写 `x{0,N}`。

### 1.4 Prompt 缓存与 slot 管理（agent 循环延迟的核心）

- `cache_prompt`（请求字段，现已默认开启）：复用上一请求的 KV cache，只重算差异后缀。
- **slot 分配按前缀相似度**：默认 `-sps` = 0.5，前缀匹配 >= 50% 的 slot 被复用（[discussion #13606](https://github.com/ggml-org/llama.cpp/discussions/13606)）。
- `--cache-reuse N`：允许通过 KV shifting 复用非严格前缀的缓存块。
- `--cache-ram N`（MiB）：逐出的 KV 存主机内存做二级缓存；`-ctxcp N` 每 slot 保存上下文检查点（[discussion #20574](https://github.com/ggml-org/llama.cpp/discussions/20574)）。
- **slot 持久化端点**：`GET /slots`、`POST /slots/{id}?action=save|restore|erase`（KV 落盘/恢复）——可用于 agent 会话切换而不丢 prefill。
- **为什么稳定前缀重要**：本地 prefill 吞吐远低于云端；前缀不稳（system prompt 时间戳、工具列表顺序变动、历史改写）则整个 KV 失效，每轮重付全量 prefill。实测量级：128k token 重处理约 60s，缓存命中后约 200ms（[CraftRigs 实测](https://craftrigs.com/guides/llama-cpp-server-prefix-cache-setup-verify/)：TTFT 降幅可达 93%）。学术佐证：[Don't Break the Cache (arXiv:2601.06007)](https://arxiv.org/abs/2601.06007)——动态工具结果/工具定义是主要 cache-breaker，选择性缓存带来 41-80% 成本、13-31% TTFT 改善。
- 其他：`-np N` 并行 slot（注意 slot 均分 --ctx-size）；`--context-shift`；投机解码 `--spec-draft-model` 等（2026 版已内置无草稿模型的 n-gram 投机）。

## 2. LM Studio

来源：[LM Studio Developer Docs](https://lmstudio.ai/docs/developer)

- **三套 API**：OpenAI 兼容、原生 REST（`/api/v1/*`，旧版 `/api/v0/*`）、SDK（lmstudio-js / lmstudio-python）。
- **原生 REST 增值**：`GET /api/v0/models` 返回每模型 loaded/not-loaded、量化格式、最大上下文；chat 响应附带 tok/s 与 TTFT 统计——对自适应调度有用。
- **工具调用**（[Tool Use](https://lmstudio.ai/docs/developer/openai-compat/tools)）：两级支持——Native（Qwen2.5、Llama-3.1/3.2、Mistral 等）与 Default（注入自定义 system prompt 描述工具，tool role 转 user role）。官方明说小模型会输出格式错误的调用，解析失败时降级为普通 content。流式下 tool call 以 delta 分片到达需自行拼接。
- **act API**：SDK 侧自动多轮工具循环（execution rounds）；逻辑在客户端 SDK，直连 REST 时需自己实现等价循环。
- **结构化输出**：`response_format: {type:"json_schema", strict:true, ...}`；GGUF 引擎走 llama.cpp grammar，MLX 引擎走 Outlines；文档警告 <7B 模型常不具备结构化输出能力。
- **模型生命周期**（[Idle TTL and Auto-Evict](https://lmstudio.ai/docs/developer/core/ttl-and-auto-evict)）：JIT loading；请求体可带 `"ttl": 300`；JIT 默认 TTL 60 分钟；Auto-Evict。**对 agent 的影响**：首请求可能吃几十秒加载延迟、长会话中模型可能被 TTL 逐出——客户端要处理冷启动重试。
- **无头部署**：`lms server start`；核心可作 daemon 无 GUI 运行。

## 3. 轻量 agent 如何让小模型可靠调工具

三条技术路线并存：

**a) 原生 function calling（模板级）**：依赖服务器解析器（llama.cpp --jinja、vLLM --tool-call-parser）。[vLLM 文档](https://docs.vllm.ai/en/stable/features/tool_calling/)显示换对 parser 能修复约 80% 的调用失败——**格式解析比模型能力更常是瓶颈**。

**b) 提示词约定格式**：
- Open Interpreter：不依赖 function calling——约定"写 markdown 代码块即执行"；`--local` 模式把 context_window 压到 3000（[How It's Built](https://sean.lyn.ch/how-its-built-open-interpreter/)）。对弱模型最鲁棒的"单工具"方案。
- aichat（Rust，[DeepWiki 分析](https://deepwiki.com/sigoden/aichat/8.4-function-calling-and-tool-execution)）：工具即外部可执行文件；`ToolCall::dedup()` 按 id 去重防死循环。短板：部分本地模型开 FC 仍失败（[issue #1065](https://github.com/sigoden/aichat/issues/1065)），无自动降级。
- smolagents（[GitHub](https://github.com/huggingface/smolagents)）：code-as-action——模型写 Python 片段作为动作，一轮表达多步，消除 JSON 字段不匹配。但 32B 级模型也会在代码中间乱插 final_answer——code-action 对指令遵循有下限要求。
- nanocoder（[GitHub](https://github.com/Nano-Collective/nanocoder)）：local-first 终端 agent，TS 侧参照。

**c) 约束解码兜底**：GBNF/json_schema 保证语法合法，但"合法 JSON 里照样能填错数据"——语义校验（函数名注册表、参数范围）必须在应用层做。

**小模型（4B-30B）可靠性数据与实践守则**（[BFCL](https://gorilla.cs.berkeley.edu/leaderboard.html) 等）：
- Qwen3-32B 约 75.7%（BFCL v3）；Qwen3-4B 多轮基线仅约 15.75%（RL 微调后 56.5%，[FunReason-MT](https://arxiv.org/pdf/2601.15625)）——**多轮工具调用是小模型的悬崖**；单轮工具选择不差：Qwen3-14B 工具选择 F1=0.971（约 GPT-4 的 0.974）、8B=0.933（Docker 2025-06 评测）。
- 实践守则：轮次上限（约 10）防死循环；执行前校验函数名（小模型爱幻觉函数名）；system prompt 明示"仅在需要外部数据时调工具"；工具数控制在 5-10 个（10 个工具定义即 1000+ token）；KV cache 别用 Q4；返回具体错误消息让模型自纠错。

## 4. 小上下文窗口的上下文管理

- 共识分层管线（[Redis](https://redis.io/blog/context-compaction/)、[Arize](https://arize.com/blog/context-management-in-agent-harnesses/)）：工具结果折叠（温和）→ 旧对话段摘要（中度）→ 滑窗只留最近约 4 轮（激进）→ 截断兜底。
- **工具输出是头号杀手**：ReAct 循环里工具观察值常占 70-80% token 预算；最佳实践是摄入时就过滤/截断（[Augment Code](https://www.augmentcode.com/guides/ai-agent-loop-token-cost-context-constraints)）。
- <30k 预算的具体实现（[NousResearch hermes-agent](https://deepwiki.com/NousResearch/hermes-agent/10.1-context-compression)）：工具输出 head-tail 截断 → 旧消息 tag 级压缩 → 超预算按时间序逐出最老消息。
- 主动 vs 被动：主流在 95-99% 容量才触发 lossy 摘要，触发太晚；研究建议阈值前置 + prevention 型（scoping/子 agent）优于 cure 型（[arXiv:2603.05344](https://arxiv.org/pdf/2603.05344)；[arXiv:2604.03515](https://arxiv.org/pdf/2604.03515) 统计 13 个开源 coding agent 出现 7 种 compaction 策略）。
- **与 §1.4 的交叉约束（本地特有）**：任何改写历史的压缩都会打碎 KV 前缀缓存 → 全量 re-prefill。本地 agent 的正确姿势是压缩少而狠，且压缩点配合 `/slots/{id}?action=save`。

## 5. 用户对"大型 agent + 本地模型"的抱怨（实证）

- **prefill 是主要瓶颈而非生成**：OpenClaw [issue #62267](https://github.com/openclaw/openclaw/issues/62267)——长上下文 agent 循环中 prompt 摄入耗时 100 秒级。HN 实测（[Ask HN](https://news.ycombinator.com/item?id=48542100)）：7900XTX 生成约 65 t/s 但 prefill 仅约 600 t/s——20k token 历史每轮重算即 30s+。
- **默认上下文太小**：[OpenHands Local LLMs 文档](https://docs.openhands.dev/openhands/usage/llms/local-llms)明说 Ollama 默认 4096 连 system prompt 都放不下；反向坑：llama.cpp 不设 --ctx-size 可能按模型元数据分配超大上下文直接爆显存。
- **工具调用失败率的体验阈值**：HN 原话 "if tool calling is busted even 5% of the time, it can totally ruin the flow"；忘开 --jinja 是最高频配置事故。
- **大 harness 的开销**：HN 用户点名 Crush 因更小的 system prompt 更适合本地模型；Claude Code 式数十工具 x 每工具百级 token 的 schema 在 8k-32k 本地窗口里是纯负担。
- **整体预期**：HN 共识——本地模型约等于 8-12 个月前的边缘云模型；2026 年推荐起点是 Qwen3.6-35B-A3B（MoE，3B active）。

## 对 Rust agent 架构的直接启示

1. **前缀稳定性当一级设计约束**：system prompt/工具定义完全静态、append-only 消息日志、动态信息置尾；默认 cache_prompt + slot 前缀匹配，长会话切换用 slots save/restore。
2. **能力探测分层**：启动时打 `GET /props` / `GET /api/v0/models` → 决定走原生 tools、Hermes XML 提示词，还是 json_schema 约束兜底。
3. **工具层为小模型减负**：<= 10 个工具、注册表校验函数名、轮次上限、错误消息面向自纠错。
4. **上下文管理与 KV 缓存联动**：head-tail 截断于摄入时；compaction 低频大动作；预算按服务器上报的真实 ctx 计算。
5. **LM Studio 特殊处理**：JIT 冷启动与 TTL 逐出的重试逻辑；利用 TTFT/tok-s 统计做自适应。
