//! 工具端口。

use kestrel_protocol::{RiskLevel, ToolSpec};

use crate::CoreError;

/// 工具执行上下文（工作目录、取消信号、输出预算）。
#[derive(Debug, Clone)]
pub struct ToolCtx {
    /// 工作目录（工具的文件操作以此为界）。
    pub workdir: std::path::PathBuf,
    /// 本次调用允许返回的最大字节数（摄入即截断，§5.2 由调用方定预算）。
    pub max_output_bytes: usize,
    /// 取消信号（贯穿到子进程，原则见 §5.1 铁律）。
    pub cancel: tokio_util::sync::CancellationToken,
}

/// 工具执行结果。
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// 是否成功。失败时 `content` 必须是面向模型自纠错的具体错误
    /// （最近似片段、行号），而非笼统的失败描述。
    pub ok: bool,
    /// 结果文本（已按 `ToolCtx::max_output_bytes` 截断）。
    pub content: String,
}

/// 工具端口。实现方：`kestrel-tools`。
///
/// 实现纪律：
/// - `spec()` 必须完全静态（前缀稳定性）；schema 逐 token 手工优化，
///   全工具总预算 <= 1400 token（原则 2）。
/// - `risk()` 按实际参数自报风险，宁高勿低。
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// 静态规格（进入模型上下文）。
    fn spec(&self) -> &ToolSpec;

    /// 按实际参数评估本次调用的风险等级（权限门的输入）。
    fn risk(&self, args: &serde_json::Value) -> RiskLevel;

    /// 执行。可恢复的失败返回 `Ok(ToolOutput { ok: false, .. })` 喂回模型；
    /// 只有基础设施故障才返回 `Err`。
    async fn call(&self, args: serde_json::Value, ctx: &ToolCtx) -> Result<ToolOutput, CoreError>;
}
