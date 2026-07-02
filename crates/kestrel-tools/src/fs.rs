//! fs 工具：read 与 edit。
//!
//! 编辑格式为弱模型设计（调研依据 docs/architecture.md 第 8 章）：
//! - 默认 SEARCH/REPLACE 块（最贴训练分布），按模型 profile 可切 whole-file。
//! - 解析宽容：容忍空白差异；匹配失败返回最近似片段供模型自纠错。
//! - 禁止行号定位（脆弱且被实证否决）。
//! - 强制编辑前先 Read（防对不存在内容的盲改）。

// TODO(M1): pub struct ReadTool; pub struct EditTool; impl Tool for ...
