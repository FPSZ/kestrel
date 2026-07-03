//! 发现（不启动）：扫描本机 `llama-server` 二进制候选 + 探测已在跑的本地引擎。
//!
//! **只发现、不 spawn**（ADR-0010 §5 铁律）：扫描产出候选，真正启动仍需用户把路径
//! 写进 loadout / 确认（配置即授权）。发现本身不越权、不联网外发、不碰任意进程。
//!
//! 扫描范围是**有界**的（PATH + 少量常见安装目录），不做全盘遍历——那既慢又惊悚。
//! 探测只打**回环**常见端口（llama.cpp / LM Studio / Ollama 默认口），全部只读 GET。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

/// 单条引擎二进制候选（语言中立：只有路径与来源标记）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BinaryCandidate {
    /// 二进制绝对路径。
    pub path: String,
    /// 是否来自 `PATH`（true=可直接调用；false=从常见目录发现）。
    pub on_path: bool,
}

/// 单个已在跑的本地引擎（语言中立：URL / 枚举码 / 数字）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunningEngine {
    /// 基址（不含 `/v1`），可直接作 `connect` / `delegate` 的 `base_url`。
    pub base_url: String,
    /// 后端类型码：`llamacpp` | `lmstudio` | `openai` | `ollama`。
    pub kind: String,
    /// 探到的真实上下文长度（llama.cpp `/props`），无则 `None`。
    pub n_ctx: Option<u32>,
    /// 探到的模型标识（`/v1/models` 首个），无则 `None`。
    pub model: Option<String>,
}

/// 一次扫描的完整结果（供前端一把梭渲染）。
#[derive(Debug, Clone, Serialize, Default)]
pub struct ScanResult {
    /// 发现的引擎二进制候选。
    pub binaries: Vec<BinaryCandidate>,
    /// 探到的已在跑本地引擎。
    pub running: Vec<RunningEngine>,
}

/// llama.cpp 服务器二进制文件名（按平台）。
#[cfg(windows)]
const BIN_NAMES: &[&str] = &["llama-server.exe"];
#[cfg(not(windows))]
const BIN_NAMES: &[&str] = &["llama-server"];

/// 探测的回环常见端口：llama.cpp(8080/8081/8000) · LM Studio(1234) · Ollama(11434)。
const COMMON_PORTS: &[u16] = &[8080, 8081, 8000, 1234, 11434];

/// 扫描 + 探测一把梭。
pub async fn scan() -> ScanResult {
    ScanResult {
        binaries: discover_binaries(),
        running: discover_running().await,
    }
}

/// 扫描 `llama-server` 二进制候选：`PATH` + 少量常见安装目录（有界，不全盘遍历）。
#[must_use]
pub fn discover_binaries() -> Vec<BinaryCandidate> {
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let path_set: BTreeSet<PathBuf> = path_dirs.iter().cloned().collect();

    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    // PATH 优先（on_path=true），再扫常见目录。
    for dir in path_dirs.iter().chain(common_dirs().iter()) {
        for name in BIN_NAMES {
            let cand = dir.join(name);
            if cand.is_file() {
                let abs = std::fs::canonicalize(&cand).unwrap_or(cand);
                let key = abs.to_string_lossy().to_string();
                if seen.insert(key.clone()) {
                    out.push(BinaryCandidate {
                        path: display_path(&abs),
                        on_path: path_set.contains(dir),
                    });
                }
            }
        }
    }
    out
}

/// 常见安装目录（env 基址 + 典型子路径 + 几个绝对常见位置）。有界集合。
fn common_dirs() -> Vec<PathBuf> {
    let subs = [
        "llama.cpp",
        "llama.cpp/build/bin",
        "llama-cpp",
        "tools/llama.cpp",
        ".local/bin",
        "bin",
    ];
    let mut dirs = Vec::new();
    for var in [
        "LOCALAPPDATA",
        "PROGRAMFILES",
        "ProgramFiles(x86)",
        "USERPROFILE",
        "HOME",
    ] {
        if let Some(v) = std::env::var_os(var) {
            let base = PathBuf::from(v);
            for sub in &subs {
                dirs.push(base.join(sub));
            }
            dirs.push(base);
        }
    }
    for p in [
        "/usr/local/bin",
        "/usr/bin",
        "/opt/llama.cpp",
        "/opt/llama.cpp/build/bin",
    ] {
        dirs.push(PathBuf::from(p));
    }
    dirs
}

/// 探测回环常见端口上已在跑的引擎（只读 GET，短超时，顺序探测）。
pub async fn discover_running() -> Vec<RunningEngine> {
    let mut out = Vec::new();
    for &port in COMMON_PORTS {
        if let Some(eng) = probe_port(port).await {
            out.push(eng);
        }
    }
    out
}

/// 探一个回环端口：先 llama.cpp `/props`（带 `n_ctx`），再 OpenAI 兼容 `/v1/models`，
/// 最后 Ollama `/api/tags`。任一命中即返回，端口给出 `kind` 提示。
async fn probe_port(port: u16) -> Option<RunningEngine> {
    let base = format!("http://127.0.0.1:{port}");
    let to = Duration::from_millis(400);

    // llama.cpp：/props 带真实 n_ctx，最有信息量，先探。
    if let Some(v) = get_json(&format!("{base}/props"), to).await {
        let n_ctx = v
            .get("default_generation_settings")
            .and_then(|g| g.get("n_ctx"))
            .or_else(|| v.get("n_ctx"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok());
        return Some(RunningEngine {
            base_url: base,
            kind: "llamacpp".to_owned(),
            n_ctx,
            model: None,
        });
    }
    // OpenAI 兼容：/v1/models。1234 口按 LM Studio 归类，其余按通用 openai。
    if let Some(v) = get_json(&format!("{base}/v1/models"), to).await {
        let model = v
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.get("id"))
            .and_then(|id| id.as_str())
            .map(str::to_owned);
        return Some(RunningEngine {
            base_url: base,
            kind: if port == 1234 { "lmstudio" } else { "openai" }.to_owned(),
            n_ctx: None,
            model,
        });
    }
    // Ollama：/api/tags。归为 delegate 用的 ollama 宿主。
    if get_json(&format!("{base}/api/tags"), to).await.is_some() {
        return Some(RunningEngine {
            base_url: base,
            kind: "ollama".to_owned(),
            n_ctx: None,
            model: None,
        });
    }
    None
}

/// `GET url`，2xx 且 body 是 JSON 才返回；任何错误/超时/非 JSON 都当「没探到」。
async fn get_json(url: &str, timeout: Duration) -> Option<serde_json::Value> {
    let client = reqwest::Client::builder().timeout(timeout).build().ok()?;
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json::<serde_json::Value>().await.ok()
}

/// 展示用路径：去掉 Windows 扩展长度前缀 `\\?\`。
fn display_path(p: &Path) -> String {
    p.to_string_lossy().trim_start_matches(r"\\?\").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_binaries_is_bounded_and_absolute() {
        // 不 panic；返回的都是绝对路径的现存文件（可能为空，取决于本机装没装）。
        let found = discover_binaries();
        for c in &found {
            let p = Path::new(&c.path);
            assert!(p.is_absolute(), "candidate must be absolute: {}", c.path);
        }
    }

    #[test]
    fn common_dirs_nonempty_on_any_platform() {
        // 至少有几个绝对兜底目录，保证扫描逻辑总能跑。
        assert!(!common_dirs().is_empty());
    }

    #[tokio::test]
    async fn probe_dead_port_is_none() {
        // 端口 1 必定没引擎在跑。
        assert!(probe_port(1).await.is_none());
    }

    #[tokio::test]
    async fn scan_shape_is_serializable() {
        // scan 结果能 JSON 化（语言中立契约，供前端）。
        let r = ScanResult::default();
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("binaries") && s.contains("running"));
    }
}
