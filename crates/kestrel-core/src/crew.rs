//! 机组作业路由（docs/architecture.md §6.6）。
//!
//! 把作业类型确定性地映射到机组角色——纯代码路由，绝不让主脑
//! 花一次昂贵推理去"决定谁来干"（反噱头纪律 §6.4）。
//!
//! 并发纪律：副手/书记作业跑在独立 tokio 任务、独立后端进程；
//! 产物仅在轮次边界并入主脑上下文（保前缀稳定）。副手永不阻塞主脑。

use kestrel_protocol::CrewRole;

/// 机组作业类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// 主循环的一轮推理。
    Turn,
    /// 历史压缩（异地，§5.2）。
    Compact,
    /// 长工具输出摘要。
    Summarize,
    /// 文件预读。
    Prefetch,
    /// 记忆/项目检索。
    Retrieve,
    /// 高危动作复核。
    Review,
}

/// 作业到角色的确定性路由。
#[must_use]
pub fn route(job: JobKind) -> CrewRole {
    match job {
        JobKind::Turn => CrewRole::Lead,
        JobKind::Compact | JobKind::Summarize | JobKind::Prefetch => CrewRole::Copilot,
        JobKind::Retrieve => CrewRole::Librarian,
        JobKind::Review => CrewRole::Critic,
    }
}

// TODO(M2): CrewPool —— 角色到已加载后端的映射表（TOML 配置或 auto 分配），
// 未配备的角色优雅降级（无审校则回退普通 y/n，无书记则回退关键词检索，§6.5）。
