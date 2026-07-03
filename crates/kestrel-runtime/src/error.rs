//! 启动器错误类型。

use std::path::PathBuf;

/// 启动 / 监督引擎过程中的错误。
///
/// 组装根拿到 `Err` 后应**优雅回退**到纯连接或直接报错退出，绝不 panic——
/// 起模型失败不该把整个 agent 拖崩。
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// 引擎二进制不满足白名单（非绝对路径 / 不存在 / 不是文件）。
    #[error("engine binary not whitelisted (must be an existing absolute path): {0}")]
    BinNotWhitelisted(PathBuf),

    /// 模型文件路径无效（不存在 / 不是文件）。
    #[error("model file not found: {0}")]
    ModelNotFound(PathBuf),

    /// spawn 引擎进程失败（二进制不可执行、权限不足等）。
    #[error("failed to spawn engine {bin}: {source}")]
    Spawn {
        /// 尝试启动的二进制路径。
        bin: PathBuf,
        /// 底层 IO 错误。
        source: std::io::Error,
    },

    /// 引擎在就绪前退出（参数错误 / 模型加载失败等）。
    #[error("engine exited before becoming ready (status: {0})")]
    EngineExited(String),

    /// 等 `/health` 就绪超时。
    #[error("engine did not become healthy within {0:?}")]
    ReadyTimeout(std::time::Duration),

    /// 委托模式下目标宿主不可达（未在跑）。
    #[error("delegate host unreachable at {0} (start it first, or use self-launch)")]
    HostUnreachable(String),

    /// 启动规格字段不合法 / 缺必填项（如 source=self 却没给 bin/model_path）。
    #[error("invalid launch spec: {0}")]
    InvalidSpec(String),
}
