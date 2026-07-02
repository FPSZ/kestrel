# 模型 Profiles

每个模型一个 TOML，描述其工具调用协议档位与编辑格式。来源有二：

1. **内置**（`<model>.toml`）：随仓库分发的实测配置，社区可贡献。
2. **本地覆盖**（`<model>.local.toml`）：能力探针（ARCHITECTURE.md §5.4）
   实测生成，优先级高于内置，不入库。

## 格式（示意，M3 定稿）

```toml
[model]
id = "qwen3-14b"

[protocol]
# native | hermes-xml | json-schema
tool_calls = "native"

[edit]
# search-replace | whole-file
format = "search-replace"
```
