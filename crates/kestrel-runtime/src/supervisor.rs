//! 启动 + 监督：spawn 引擎、轮询 `/health` 就绪、抓 stderr 日志、握住句柄以便 stop。

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::RuntimeError;
use crate::spec::{EngineSource, LaunchSpec, llama_server_args};

/// 保留的引擎日志行数上限（环形缓冲，够看加载进度与报错，不吃内存）。
const LOG_CAP: usize = 400;

/// 引擎 stderr 日志环（跨 spawn 的读取任务与句柄共享）。
type LogRing = Arc<Mutex<VecDeque<String>>>;

/// 已启动 / 已连接引擎的句柄。握住它 = 握住进程生命周期。
///
/// `Drop` 时若持有子进程会被 `kill_on_drop` 收割（防泄漏僵尸引擎）；正常收尾请显式
/// [`stop`](EngineHandle::stop) 以 await 到进程真正退出。
#[derive(Debug)]
pub struct EngineHandle {
    base_url: String,
    /// 自启模式持有子进程；委托 / 连接模式为 `None`（不归我们管的进程绝不杀）。
    child: Option<Child>,
    source_desc: &'static str,
    /// 自启引擎的 stderr 日志环（委托 / 连接为空）。
    logs: LogRing,
}

impl EngineHandle {
    /// 后端应连接的 `base_url`（不含 `/v1`）。
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 引擎来源简述（`self:llama.cpp` / `delegate` / `connect`），供组装根日志。
    #[must_use]
    pub fn source(&self) -> &str {
        self.source_desc
    }

    /// 最近的引擎 stderr 日志行（自启引擎；委托 / 连接为空）。供 UI 的日志窗。
    #[must_use]
    pub fn recent_logs(&self) -> Vec<String> {
        self.logs
            .lock()
            .map(|g| g.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// 自启子进程是否仍在跑（崩溃检测）。委托 / 连接不归我们管，一律视为「在」。
    /// 取 `&mut self`：`try_wait` 需可变借用子进程。
    pub fn is_alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)), // Ok(None)=仍在跑
            None => true,
        }
    }

    /// 引擎当前是否健康（`GET /health` 返回 2xx）。
    pub async fn health(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        probe_ready(&url, Duration::from_secs(2)).await
    }

    /// 停止引擎。自启的子进程会被杀并 await 到退出；委托 / 连接模式为 no-op
    /// （不是我们起的进程，绝不代杀——尊重用户自己的 qon/qoff 流）。
    pub async fn stop(mut self) -> Result<(), RuntimeError> {
        if let Some(mut child) = self.child.take() {
            let pid = child.id();
            // start_kill 发信号后 wait 收割；已退出则忽略。
            let _ = child.start_kill();
            let _ = child.wait().await;
            tracing::info!(event = "engine.stop", ?pid, "engine process stopped");
        }
        Ok(())
    }
}

/// 按 [`LaunchSpec`] 启动 / 连接引擎，返回一个就绪的 [`EngineHandle`]。
///
/// - 自启：校验白名单 -> spawn -> 轮询 `/health` 就绪（期间若进程早退则报错）。
/// - 委托：探测目标宿主可达才返回；不可达报 [`RuntimeError::HostUnreachable`]。
/// - 连接：零启动，直接返回句柄（不探测——沿用现状「连了再说」，probe 交给 backend）。
///
/// 审计：spawn / ready / delegate / connect 各发一条结构化 `tracing` 记录（英文、可 grep）。
pub async fn launch(spec: LaunchSpec) -> Result<EngineHandle, RuntimeError> {
    match &spec.source {
        EngineSource::SelfLaunch {
            bin,
            model_path,
            gpu_layers,
            extra_args,
        } => {
            validate_bin(bin)?;
            validate_model(model_path)?;
            let args = llama_server_args(model_path, spec.port, spec.n_ctx, gpu_layers, extra_args);

            // 审计轨（配置即授权 + 可审计，ADR-0010 §5）。不打印 api_key 等敏感项。
            tracing::info!(
                event = "engine.launch",
                bin = %bin.display(),
                model = %model_path.display(),
                port = spec.port,
                n_ctx = spec.n_ctx,
                gpu_layers = %gpu_layers,
                "launching self-hosted llama.cpp"
            );

            let mut cmd = Command::new(bin);
            cmd.args(&args);
            cmd.stderr(Stdio::piped()); // 抓引擎日志（llama-server 走 stderr）。
            cmd.kill_on_drop(true); // 句柄泄漏时兜底收割，防僵尸引擎。
            let mut child = cmd.spawn().map_err(|source| RuntimeError::Spawn {
                bin: bin.clone(),
                source,
            })?;

            // stderr -> 环形日志缓冲（后台任务持续追加，句柄据此供 UI 日志窗）。
            let logs: LogRing = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAP)));
            if let Some(stderr) = child.stderr.take() {
                let sink = logs.clone();
                tokio::spawn(async move {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        if let Ok(mut g) = sink.lock() {
                            if g.len() >= LOG_CAP {
                                g.pop_front();
                            }
                            g.push_back(line);
                        }
                    }
                });
            }

            // 就绪失败时把最后几行日志折进错误，让 UI 直接看到「为什么起不来」。
            if let Err(e) = wait_ready_child(&spec, &mut child).await {
                let tail = log_tail(&logs, 6);
                return Err(if tail.is_empty() {
                    e
                } else {
                    RuntimeError::LaunchFailed(format!("{e}\n{tail}"))
                });
            }

            let base_url = spec.resolved_base_url();
            tracing::info!(
                event = "engine.ready",
                pid = ?child.id(),
                base_url = %base_url,
                "engine healthy"
            );
            Ok(EngineHandle {
                base_url,
                child: Some(child),
                source_desc: "self:llama.cpp",
                logs,
            })
        }

        EngineSource::Delegate => {
            let base_url = spec.resolved_base_url();
            let health = spec.health_url();
            if !probe_ready(&health, Duration::from_secs(3)).await {
                // 给个可操作的提示：装了 lms/ollama 但没在跑时点出来。
                if crate::detect::host_tool_available("ollama")
                    || crate::detect::host_tool_available("lms")
                {
                    tracing::warn!(
                        base_url = %base_url,
                        "delegate host not serving; a known host CLI (ollama/lms) is installed \
                         but not started — start it or switch to self-launch"
                    );
                }
                return Err(RuntimeError::HostUnreachable(base_url));
            }
            tracing::info!(event = "engine.delegate", base_url = %base_url, "delegating to running host");
            Ok(EngineHandle {
                base_url,
                child: None,
                source_desc: "delegate",
                logs: empty_logs(),
            })
        }

        EngineSource::Connect => {
            let base_url = spec.resolved_base_url();
            tracing::info!(event = "engine.connect", base_url = %base_url, "pure-connect (no launch)");
            Ok(EngineHandle {
                base_url,
                child: None,
                source_desc: "connect",
                logs: empty_logs(),
            })
        }
    }
}

/// 空日志环（委托 / 连接模式没有我们管的进程，无日志可抓）。
fn empty_logs() -> LogRing {
    Arc::new(Mutex::new(VecDeque::new()))
}

/// 取日志环最后 `n` 行拼成一段（失败诊断用）。
fn log_tail(logs: &LogRing, n: usize) -> String {
    logs.lock()
        .map(|g| {
            let start = g.len().saturating_sub(n);
            g.iter().skip(start).cloned().collect::<Vec<_>>().join("\n")
        })
        .unwrap_or_default()
}

/// 白名单校验：`bin` 必须是**存在的绝对路径的文件**（ADR-0010 §5 防任意路径 spawn）。
fn validate_bin(bin: &Path) -> Result<(), RuntimeError> {
    if bin.is_absolute() && bin.is_file() {
        Ok(())
    } else {
        Err(RuntimeError::BinNotWhitelisted(bin.to_path_buf()))
    }
}

/// 模型文件校验：必须存在且是文件。
fn validate_model(model_path: &Path) -> Result<(), RuntimeError> {
    if model_path.is_file() {
        Ok(())
    } else {
        Err(RuntimeError::ModelNotFound(model_path.to_path_buf()))
    }
}

/// 轮询 `/health` 直到 2xx 或超时；期间若子进程早退（参数/加载失败）立即报错。
async fn wait_ready_child(spec: &LaunchSpec, child: &mut Child) -> Result<(), RuntimeError> {
    let health = spec.health_url();
    let poll = spec.poll_interval;
    let wait = async {
        loop {
            // 进程先于就绪退出 = 启动失败，别干等到超时。
            if let Ok(Some(status)) = child.try_wait() {
                return Err(RuntimeError::EngineExited(status.to_string()));
            }
            if probe_ready(&health, poll).await {
                return Ok(());
            }
            tokio::time::sleep(poll).await;
        }
    };
    match tokio::time::timeout(spec.ready_timeout, wait).await {
        Ok(res) => res,
        Err(_) => Err(RuntimeError::ReadyTimeout(spec.ready_timeout)),
    }
}

/// `GET <health_url>`，2xx 即视为就绪。任何连接 / 超时错误都当「未就绪」（false）。
///
/// `pub(crate)` 以便单测用回环 socket 假冒健康端点，无需真装 llama-server。
pub(crate) async fn probe_ready(health_url: &str, timeout: Duration) -> bool {
    let Ok(client) = reqwest::Client::builder().timeout(timeout).build() else {
        return false;
    };
    matches!(client.get(health_url).send().await, Ok(r) if r.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn validate_bin_rejects_relative_and_missing() {
        // 相对路径（哪怕存在）不接受：白名单要求绝对路径。
        assert!(matches!(
            validate_bin(Path::new("llama-server")),
            Err(RuntimeError::BinNotWhitelisted(_))
        ));
        // 绝对但不存在。
        assert!(matches!(
            validate_bin(Path::new("/definitely/not/here/llama-server")),
            Err(RuntimeError::BinNotWhitelisted(_))
        ));
    }

    #[test]
    fn validate_bin_accepts_existing_absolute_file() {
        // 用一个必定存在的绝对路径文件当替身（本源码文件自身）。
        let me = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(me.is_absolute() && me.is_file());
        assert!(validate_bin(&me).is_ok());
    }

    /// 起一个只回一次 200 的假 /health server，返回其 base_url。
    async fn fake_health_server(ok: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let resp = if ok {
                    "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok"
                } else {
                    "HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\n\r\n"
                };
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.flush().await;
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn probe_ready_true_on_200() {
        let base = fake_health_server(true).await;
        let url = format!("{base}/health");
        assert!(probe_ready(&url, Duration::from_secs(2)).await);
    }

    #[tokio::test]
    async fn probe_ready_false_on_503() {
        let base = fake_health_server(false).await;
        let url = format!("{base}/health");
        assert!(!probe_ready(&url, Duration::from_secs(2)).await);
    }

    #[tokio::test]
    async fn probe_ready_false_on_dead_port() {
        // 未监听的端口：连接被拒 -> 未就绪。
        assert!(!probe_ready("http://127.0.0.1:1/health", Duration::from_millis(300)).await);
    }

    #[tokio::test]
    async fn delegate_unreachable_errors() {
        let spec = LaunchSpec::delegate("http://127.0.0.1:1".to_owned(), 8192);
        assert!(matches!(
            launch(spec).await,
            Err(RuntimeError::HostUnreachable(_))
        ));
    }

    #[tokio::test]
    async fn connect_never_spawns_and_resolves_base_url() {
        let spec = LaunchSpec::connect("http://127.0.0.1:5678".to_owned(), 8192);
        let handle = launch(spec).await.unwrap();
        assert_eq!(handle.base_url(), "http://127.0.0.1:5678");
        // 连接模式 stop 是 no-op（没有子进程可杀）。
        handle.stop().await.unwrap();
    }
}
