//! search 工具：内容搜索（grep）与文件名匹配（glob）合一。
//!
//! 合一的理由：省一个工具 schema 的 token（原则 2）。
//! 结果带截断默认值（如最多 100 条），返回高信号信息。

// TODO(M1): pub struct SearchTool; impl Tool for SearchTool
