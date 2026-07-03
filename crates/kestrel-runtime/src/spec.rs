//! 启动规格：喂给 [`crate::launch`] 的完整意图（纯数据 + 纯映射，无副作用）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::RuntimeError;

/// 引擎二进制强制绑定的地址：只回环，不暴露公网（ADR-0010 §5）。
pub(crate) const LOOPBACK: &str = "127.0.0.1";
/// `/health` 就绪等待默认上限（本地大模型冷加载可能数十秒）。
const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(120);
/// `/health` 轮询默认间隔。
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(300);

/// 引擎来源（ADR-0010 §2 三种一等公民）。
#[derive(Debug, Clone)]
pub enum EngineSource {
    /// a. 自启 llama.cpp：spawn `llama-server` 并监督其生命周期。
    SelfLaunch {
        /// 引擎二进制**绝对路径**（白名单=显式配置，ADR-0010 §5）。
        bin: PathBuf,
        /// gguf 模型**绝对路径**。
        model_path: PathBuf,
        /// GPU 卸载策略：`auto`（默认，交引擎决定，防 OOM）| `max` | 数字层数。
        gpu_layers: String,
        /// 原样透传给 `llama-server` 的追加参数（高级用户逃生舱）。
        extra_args: Vec<String>,
    },
    /// b. 委托已有宿主：连一个已在跑的 server（lms / ollama / 手起 llama-server）。
    Delegate,
    /// c. 纯连接（现状）：连指定 `base_url`，零启动。
    Connect,
}

/// 启动规格。构造走 [`LaunchSpec::self_launch`] / [`connect`](LaunchSpec::connect) /
/// [`delegate`](LaunchSpec::delegate) 三个构造器，保证安全不变量（自启强制回环）。
#[derive(Debug, Clone)]
pub struct LaunchSpec {
    /// 引擎来源。
    pub source: EngineSource,
    /// 端口（自启：`llama-server --port`；连接：合成 `base_url` 用）。
    pub port: u16,
    /// 上下文长度（自启：`llama-server -c`；probe 成功后以实测值覆盖 ledger）。
    pub n_ctx: u32,
    /// 委托 / 连接模式下的显式 `base_url`（不含 `/v1`）；`None` 则用回环 `host:port` 合成。
    pub base_url: Option<String>,
    /// `/health` 就绪等待上限。
    pub ready_timeout: Duration,
    /// `/health` 轮询间隔。
    pub poll_interval: Duration,
}

impl LaunchSpec {
    /// 自启 llama.cpp。始终绑回环 `127.0.0.1`（安全不变量，不可由外部改写）。
    #[must_use]
    pub fn self_launch(
        bin: PathBuf,
        model_path: PathBuf,
        port: u16,
        n_ctx: u32,
        gpu_layers: String,
        extra_args: Vec<String>,
    ) -> Self {
        Self {
            source: EngineSource::SelfLaunch {
                bin,
                model_path,
                gpu_layers,
                extra_args,
            },
            port,
            n_ctx,
            base_url: None, // 自启：base_url 由回环 host:port 合成，不接受外部注入。
            ready_timeout: DEFAULT_READY_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    /// 纯连接一个已在跑的 `base_url`（不含 `/v1`）。
    #[must_use]
    pub fn connect(base_url: String, n_ctx: u32) -> Self {
        Self {
            source: EngineSource::Connect,
            port: 0,
            n_ctx,
            base_url: Some(base_url),
            ready_timeout: DEFAULT_READY_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    /// 委托一个已在跑的宿主 `base_url`（可达才用，否则 [`crate::launch`] 报错）。
    #[must_use]
    pub fn delegate(base_url: String, n_ctx: u32) -> Self {
        Self {
            source: EngineSource::Delegate,
            port: 0,
            n_ctx,
            base_url: Some(base_url),
            ready_timeout: DEFAULT_READY_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    /// 从原始字段（如 Loadout 解析所得的原语）构造 [`LaunchSpec`]。
    ///
    /// 映射逻辑集中于此，让组装根（cli / server）无需各写一遍 `source -> spec` 的
    /// match，也让 `kestrel-runtime` 不必反依赖 `kestrel-store`（只收原语）。
    ///
    /// `source` 归一化：`self`/`self-launch`/`launch` -> 自启；`delegate`/`host` -> 委托；
    /// 其余 -> 纯连接。自启缺 `bin`/`model_path` 报 [`RuntimeError::InvalidSpec`]；
    /// 委托 / 连接缺 `base_url` 时用回环 `127.0.0.1:port` 合成。
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        source: &str,
        bin: Option<PathBuf>,
        model_path: Option<PathBuf>,
        base_url: Option<String>,
        port: u16,
        n_ctx: u32,
        gpu_layers: String,
        extra_args: Vec<String>,
    ) -> Result<Self, RuntimeError> {
        let synth = || format!("http://{LOOPBACK}:{port}");
        match source.trim().to_ascii_lowercase().as_str() {
            "self" | "self-launch" | "self_launch" | "launch" => {
                let bin = bin.ok_or_else(|| {
                    RuntimeError::InvalidSpec("model.bin required for source=self".to_owned())
                })?;
                let model_path = model_path.ok_or_else(|| {
                    RuntimeError::InvalidSpec(
                        "model.model_path required for source=self".to_owned(),
                    )
                })?;
                Ok(Self::self_launch(
                    bin, model_path, port, n_ctx, gpu_layers, extra_args,
                ))
            }
            "delegate" | "host" => Ok(Self::delegate(base_url.unwrap_or_else(synth), n_ctx)),
            _ => Ok(Self::connect(base_url.unwrap_or_else(synth), n_ctx)),
        }
    }

    /// 后端最终要连接的 `base_url`（不含 `/v1`）。自启用回环 `host:port` 合成；
    /// 委托 / 连接用显式 `base_url`。
    #[must_use]
    pub fn resolved_base_url(&self) -> String {
        match &self.base_url {
            Some(u) => u.trim_end_matches('/').to_owned(),
            None => format!("http://{LOOPBACK}:{}", self.port),
        }
    }

    /// `/health` 端点 URL。
    #[must_use]
    pub fn health_url(&self) -> String {
        format!("{}/health", self.resolved_base_url())
    }
}

/// 构造 `llama-server` 命令行参数（**纯函数**，可测；不含 spawn 副作用）。
///
/// 恒带 `--jinja`：强制向模型暴露工具，消灭「模型看不见工具」这个本地 agent
/// 头号翻车点（ADR-0010 §3）。恒带 `--host 127.0.0.1`：安全约束（§5）。
pub(crate) fn llama_server_args(
    model_path: &Path,
    port: u16,
    n_ctx: u32,
    gpu_layers: &str,
    extra_args: &[String],
) -> Vec<String> {
    let mut args = vec![
        "-m".to_owned(),
        model_path.display().to_string(),
        "--host".to_owned(),
        LOOPBACK.to_owned(),
        "--port".to_owned(),
        port.to_string(),
        "-c".to_owned(),
        n_ctx.to_string(),
        "--jinja".to_owned(),
    ];
    match gpu_layers.trim() {
        // auto / 空：不传 -ngl，交给引擎默认（防 OOM，§5）。
        "auto" | "" => {}
        // max：尽量全卸载到 GPU（大数字，llama.cpp 会 clamp 到实际层数）。
        "max" => {
            args.push("-ngl".to_owned());
            args.push("999".to_owned());
        }
        // 显式数字或其他：原样透传（用户自负）。
        n => {
            args.push("-ngl".to_owned());
            args.push(n.to_owned());
        }
    }
    args.extend_from_slice(extra_args);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_launch_forces_loopback_and_jinja() {
        let args = llama_server_args(&PathBuf::from("/models/q4.gguf"), 8080, 32768, "auto", &[]);
        // 强制回环。
        let host_i = args.iter().position(|a| a == "--host").unwrap();
        assert_eq!(args[host_i + 1], "127.0.0.1");
        // 强制 --jinja（工具可见性）。
        assert!(args.iter().any(|a| a == "--jinja"));
        // auto 不传 -ngl。
        assert!(!args.iter().any(|a| a == "-ngl"));
    }

    #[test]
    fn gpu_layers_max_and_numeric() {
        let max = llama_server_args(&PathBuf::from("/m.gguf"), 1, 1, "max", &[]);
        let i = max.iter().position(|a| a == "-ngl").unwrap();
        assert_eq!(max[i + 1], "999");

        let num = llama_server_args(&PathBuf::from("/m.gguf"), 1, 1, "35", &[]);
        let j = num.iter().position(|a| a == "-ngl").unwrap();
        assert_eq!(num[j + 1], "35");
    }

    #[test]
    fn extra_args_passthrough() {
        let args = llama_server_args(
            &PathBuf::from("/m.gguf"),
            1,
            1,
            "auto",
            &["--flash-attn".to_owned(), "--mlock".to_owned()],
        );
        assert!(args.iter().any(|a| a == "--flash-attn"));
        assert!(args.iter().any(|a| a == "--mlock"));
    }

    #[test]
    fn resolved_base_url_synthesizes_loopback_for_self_launch() {
        let spec = LaunchSpec::self_launch(
            PathBuf::from("/bin/llama-server"),
            PathBuf::from("/m.gguf"),
            9090,
            4096,
            "auto".to_owned(),
            vec![],
        );
        assert_eq!(spec.resolved_base_url(), "http://127.0.0.1:9090");
        assert_eq!(spec.health_url(), "http://127.0.0.1:9090/health");
    }

    #[test]
    fn resolved_base_url_trims_trailing_slash_for_connect() {
        let spec = LaunchSpec::connect("http://127.0.0.1:1234/".to_owned(), 8192);
        assert_eq!(spec.resolved_base_url(), "http://127.0.0.1:1234");
    }

    #[test]
    fn from_parts_self_requires_bin_and_model() {
        // 缺 bin -> InvalidSpec。
        let err = LaunchSpec::from_parts(
            "self",
            None,
            Some(PathBuf::from("/m.gguf")),
            None,
            8080,
            4096,
            "auto".to_owned(),
            vec![],
        );
        assert!(matches!(err, Err(RuntimeError::InvalidSpec(_))));

        // 齐全 -> SelfLaunch。
        let spec = LaunchSpec::from_parts(
            "self-launch",
            Some(PathBuf::from("/bin/llama-server")),
            Some(PathBuf::from("/m.gguf")),
            None,
            8080,
            4096,
            "auto".to_owned(),
            vec![],
        )
        .unwrap();
        assert!(matches!(spec.source, EngineSource::SelfLaunch { .. }));
    }

    #[test]
    fn from_parts_connect_synthesizes_loopback_when_no_base_url() {
        let spec = LaunchSpec::from_parts(
            "connect",
            None,
            None,
            None,
            7777,
            8192,
            "auto".to_owned(),
            vec![],
        )
        .unwrap();
        assert!(matches!(spec.source, EngineSource::Connect));
        assert_eq!(spec.resolved_base_url(), "http://127.0.0.1:7777");
    }

    #[test]
    fn from_parts_unknown_source_falls_back_to_connect() {
        let spec = LaunchSpec::from_parts(
            "banana",
            None,
            None,
            Some("http://127.0.0.1:9000".to_owned()),
            0,
            8192,
            "auto".to_owned(),
            vec![],
        )
        .unwrap();
        assert!(matches!(spec.source, EngineSource::Connect));
    }
}
