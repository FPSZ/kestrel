//! 数据/配置目录解析 + 版本化迁移（ADR-0009）。
//!
//! 默认用 OS 标准目录（`directories` crate，不手写平台分支）：
//! - Windows: 配置 `%APPDATA%\Kestrel\config`、数据 `%LOCALAPPDATA%\Kestrel\data`
//! - Linux:   `$XDG_CONFIG_HOME/kestrel`、`$XDG_DATA_HOME/kestrel`
//! - macOS:   `~/Library/Application Support/Kestrel`
//!
//! 解析优先级（对齐 foundations #10 与 ADR-0009 §3）：
//! - 数据根：`KESTREL_DATA_DIR` 环境变量 > 项目内 `.kestrel/`（opt-in）> OS 标准目录。
//! - 配置文件：项目内 `./kestrel.toml`（dev 便利）> `KESTREL_CONFIG_DIR` > `.kestrel/` >
//!   OS 标准配置目录。
//!
//! 首次在新数据目录运行时，若检测到旧的 `<workdir>/sessions` 且新目录尚无数据，
//! **拷贝**迁移（非破坏：不删旧数据）并写下 `layout_version` 标记，保证幂等。
//!
//! 注：env 读取在此发生（IO 边缘 / 组装期），不进 core 的事件路径——不违反 core 确定性。

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::StoreError;

/// 当前数据目录布局版本。破坏性改布局时递增并加迁移钩子。
const LAYOUT_VERSION: u32 = 1;
/// 数据目录里记录布局版本的标记文件（存在即视为已初始化/迁移过）。
const LAYOUT_MARKER: &str = ".layout_version";
/// 项目内 opt-in 数据目录名。
const PROJECT_DIR: &str = ".kestrel";
/// 配置文件名。
const CONFIG_FILE: &str = "kestrel.toml";
/// 旧布局的会话目录名（相对工作目录）。
const LEGACY_SESSIONS: &str = "sessions";

/// 解析后的目录布局。
#[derive(Debug, Clone)]
pub struct Layout {
    data_dir: PathBuf,
    config_file: PathBuf,
}

impl Layout {
    /// 解析布局并确保数据目录存在、旧会话已迁移。`workdir` 是 kestrel 启动目录
    /// （用于 `.kestrel/` opt-in 与旧 `./sessions` 探测），与 agent 的文件操作
    /// 工作目录是两回事。
    pub fn resolve(workdir: &Path) -> Result<Self, StoreError> {
        let os = ProjectDirs::from("", "", "Kestrel");
        let os_data = os.as_ref().map(|d| d.data_local_dir().to_path_buf());
        let os_config = os.as_ref().map(|d| d.config_dir().to_path_buf());

        let env_data = std::env::var_os("KESTREL_DATA_DIR").map(PathBuf::from);
        let env_config = std::env::var_os("KESTREL_CONFIG_DIR").map(PathBuf::from);
        let project = project_dir(workdir);

        let data_dir = decide_data_dir(env_data, project.clone(), os_data, workdir);

        // 配置文件：项目内 ./kestrel.toml 优先（dev 便利），其余按目录优先级。
        let local_cfg = workdir.join(CONFIG_FILE);
        let config_file = if local_cfg.is_file() {
            local_cfg
        } else {
            decide_config_dir(env_config, project, os_config, workdir).join(CONFIG_FILE)
        };

        std::fs::create_dir_all(&data_dir)
            .map_err(|e| StoreError::Io(format!("create data dir {}: {e}", data_dir.display())))?;
        migrate_legacy_sessions(workdir, &data_dir)?;

        Ok(Self {
            data_dir,
            config_file,
        })
    }

    /// 会话事件日志目录（`<data>/sessions`）。
    #[must_use]
    pub fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    /// 模型 profile 目录（`<data>/profiles`，探针覆盖落此，不污染仓库内置）。
    #[must_use]
    pub fn profiles_dir(&self) -> PathBuf {
        self.data_dir.join("profiles")
    }

    /// 配置文件路径。
    #[must_use]
    pub fn config_file(&self) -> &Path {
        &self.config_file
    }

    /// 数据根目录。
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// 项目内 `.kestrel/` opt-in 目录：仅当它已存在时返回（存在=用户选择本地存放）。
fn project_dir(workdir: &Path) -> Option<PathBuf> {
    let p = workdir.join(PROJECT_DIR);
    p.is_dir().then_some(p)
}

/// 数据根决策（纯函数）：env > 项目 `.kestrel/` > OS 标准 > 兜底 `<workdir>/.kestrel`。
fn decide_data_dir(
    env: Option<PathBuf>,
    project: Option<PathBuf>,
    os: Option<PathBuf>,
    workdir: &Path,
) -> PathBuf {
    env.or(project)
        .or(os)
        .unwrap_or_else(|| workdir.join(PROJECT_DIR))
}

/// 配置目录决策（纯函数）：env > 项目 `.kestrel/` > OS 标准 > 兜底 workdir。
fn decide_config_dir(
    env: Option<PathBuf>,
    project: Option<PathBuf>,
    os: Option<PathBuf>,
    workdir: &Path,
) -> PathBuf {
    env.or(project)
        .or(os)
        .unwrap_or_else(|| workdir.to_path_buf())
}

/// 幂等迁移：首次在新数据目录运行且检测到旧 `<workdir>/sessions` 有数据、新目录尚无
/// 数据时，拷贝旧会话日志过来（非破坏），随后写 `layout_version` 标记封顶。
fn migrate_legacy_sessions(workdir: &Path, data_dir: &Path) -> Result<(), StoreError> {
    let marker = data_dir.join(LAYOUT_MARKER);
    if marker.exists() {
        return Ok(()); // 已初始化/迁移过：幂等短路。
    }

    let legacy = workdir.join(LEGACY_SESSIONS);
    let new_sessions = data_dir.join("sessions");
    // 旧目录同时也是新目录时（如 .kestrel 布局巧合），无需迁移。
    if legacy.is_dir()
        && legacy != new_sessions
        && dir_has_jsonl(&legacy)
        && !dir_has_jsonl(&new_sessions)
    {
        std::fs::create_dir_all(&new_sessions)
            .map_err(|e| StoreError::Io(format!("create {}: {e}", new_sessions.display())))?;
        let mut copied = 0u32;
        if let Ok(entries) = std::fs::read_dir(&legacy) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                    && let Some(name) = path.file_name()
                {
                    std::fs::copy(&path, new_sessions.join(name))
                        .map_err(|e| StoreError::Io(format!("migrate {}: {e}", path.display())))?;
                    copied += 1;
                }
            }
        }
        tracing::info!(
            copied,
            from = %legacy.display(),
            to = %new_sessions.display(),
            "migrated legacy sessions (originals kept)"
        );
    }

    std::fs::write(&marker, LAYOUT_VERSION.to_string())
        .map_err(|e| StoreError::Io(format!("write layout marker: {e}")))?;
    Ok(())
}

/// 目录里是否有至少一个 `.jsonl` 文件。
fn dir_has_jsonl(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_priority_env_over_project_over_os() {
        let wd = Path::new("/work");
        let env = Some(PathBuf::from("/env/data"));
        let project = Some(PathBuf::from("/work/.kestrel"));
        let os = Some(PathBuf::from("/os/data"));
        // env 最高。
        assert_eq!(
            decide_data_dir(env.clone(), project.clone(), os.clone(), wd),
            PathBuf::from("/env/data")
        );
        // 无 env 时项目 .kestrel 优先于 OS。
        assert_eq!(
            decide_data_dir(None, project, os.clone(), wd),
            PathBuf::from("/work/.kestrel")
        );
        // 都无时退 OS。
        assert_eq!(
            decide_data_dir(None, None, os, wd),
            PathBuf::from("/os/data")
        );
        // 全无时兜底 workdir/.kestrel。
        assert_eq!(
            decide_data_dir(None, None, None, wd),
            PathBuf::from("/work/.kestrel")
        );
    }

    #[test]
    fn migrate_is_idempotent_and_nondestructive() {
        let base = std::env::temp_dir().join(format!("kestrel-mig-{}", std::process::id()));
        let workdir = base.join("proj");
        let data_dir = base.join("data");
        let legacy = workdir.join("sessions");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(legacy.join("s1.jsonl"), "{\"seq\":0}\n").unwrap();

        // 首次：迁移拷贝，旧文件保留。
        migrate_legacy_sessions(&workdir, &data_dir).unwrap();
        assert!(data_dir.join("sessions").join("s1.jsonl").is_file());
        assert!(
            legacy.join("s1.jsonl").is_file(),
            "legacy kept (non-destructive)"
        );
        assert!(data_dir.join(LAYOUT_MARKER).is_file());

        // 再删掉迁移出来的文件、二次运行：marker 已在，不再迁移（幂等）。
        std::fs::remove_file(data_dir.join("sessions").join("s1.jsonl")).unwrap();
        migrate_legacy_sessions(&workdir, &data_dir).unwrap();
        assert!(
            !data_dir.join("sessions").join("s1.jsonl").exists(),
            "marker present -> no re-migration"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn no_legacy_still_writes_marker() {
        let base = std::env::temp_dir().join(format!("kestrel-nomig-{}", std::process::id()));
        let workdir = base.join("proj");
        let data_dir = base.join("data");
        std::fs::create_dir_all(&workdir).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();

        migrate_legacy_sessions(&workdir, &data_dir).unwrap();
        assert!(data_dir.join(LAYOUT_MARKER).is_file());

        let _ = std::fs::remove_dir_all(&base);
    }
}
