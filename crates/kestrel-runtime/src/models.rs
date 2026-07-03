//! 本地 GGUF 模型发现 + 元数据解析（喂 LM Studio 式「模型表」）。
//!
//! **只读、不启动**：递归扫描模型目录里的 `*.gguf`，读文件头的 GGUF 元数据拿到
//! 架构 / 量化 / 参数量，配合文件大小，让前端能像 LM Studio 那样列出你的本地模型。
//!
//! GGUF 是小端二进制格式：magic `GGUF` + version(u32) + tensor_count(u64) +
//! kv_count(u64) + kv_count 个 KV 元数据项。我们只取 `general.*` 少数键（都排在
//! 大数组如 tokenizer 之前），遇到超大数组即早退，保证有界、不把整文件读进内存。

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

/// 单个本地 GGUF 模型文件（语言中立：路径 / 名称 / 枚举码 / 数字）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModelFile {
    /// gguf 绝对路径。
    pub path: String,
    /// 显示名（GGUF `general.name`，缺则用文件名去扩展名）。
    pub name: String,
    /// 架构（GGUF `general.architecture`，如 `qwen3moe` / `llama`），未知为空。
    pub arch: String,
    /// 量化（如 `Q4_K_M` / `F16`），从文件名或 `general.file_type` 推出，未知为空。
    pub quant: String,
    /// 参数量标签（如 `35B-A3B` / `8B`），从 GGUF 或文件名推出，未知为空。
    pub params: String,
    /// 文件大小（字节）。
    pub size_bytes: u64,
}

/// 常见本地模型目录（LM Studio 默认位置 + 通用），返回首个存在的。用户没在 UI 指定
/// 目录时的兜底；不是全盘搜索，只探已知位置。
#[must_use]
pub fn default_models_dir() -> Option<std::path::PathBuf> {
    let mut cands = Vec::new();
    for var in ["USERPROFILE", "HOME"] {
        if let Some(v) = std::env::var_os(var) {
            let base = std::path::PathBuf::from(v);
            cands.push(base.join(".lmstudio").join("models"));
            cands.push(base.join(".cache").join("lm-studio").join("models"));
            cands.push(base.join("models"));
        }
    }
    cands.into_iter().find(|p| p.is_dir())
}

/// 递归扫描 `root` 下的 `*.gguf`（有界深度，处理 LM Studio 的 发布者/仓库 嵌套）。
/// 目录不存在或不可读时返回空。按显示名排序。
#[must_use]
pub fn discover_models(root: &Path) -> Vec<ModelFile> {
    let mut out = Vec::new();
    walk(root, 0, 8, &mut out);
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// 有界深度递归收集 gguf。
fn walk(dir: &Path, depth: u32, max_depth: u32, out: &mut Vec<ModelFile>) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth + 1, max_depth, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
        {
            // 多分片只收第一片（...-00001-of-000NN.gguf），避免同一模型列多行。
            if is_nonfirst_shard(&path) {
                continue;
            }
            out.push(model_from_path(&path));
        }
    }
}

/// 从单个 gguf 文件构造 [`ModelFile`]（元数据尽力而为，解析失败仍以文件名兜底）。
fn model_from_path(path: &Path) -> ModelFile {
    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_owned();

    let meta = File::open(path)
        .ok()
        .and_then(|mut f| parse_gguf(&mut f).ok())
        .unwrap_or_default();

    let arch = meta.arch;
    // 量化：文件名优先（与 LM Studio 显示一致），回退 GGUF file_type 枚举。
    let quant = quant_from_name(&file_stem)
        .or_else(|| meta.file_type.and_then(ftype_to_quant))
        .unwrap_or_default();
    // 参数量：GGUF size_label 优先，回退文件名里的 NNB 模式。
    let params = if meta.size_label.is_empty() {
        params_from_name(&file_stem).unwrap_or_default()
    } else {
        meta.size_label
    };
    let name = if meta.name.is_empty() {
        file_stem
    } else {
        meta.name
    };

    ModelFile {
        path: display_path(path),
        name,
        arch,
        quant,
        params,
        size_bytes,
    }
}

/// 从 GGUF 头解析出的少量 `general.*` 元数据。
#[derive(Debug, Default)]
struct GgufMeta {
    arch: String,
    name: String,
    size_label: String,
    file_type: Option<u32>,
}

// GGUF 值类型码。
const T_UINT8: u32 = 0;
const T_INT8: u32 = 1;
const T_UINT16: u32 = 2;
const T_INT16: u32 = 3;
const T_UINT32: u32 = 4;
const T_INT32: u32 = 5;
const T_FLOAT32: u32 = 6;
const T_BOOL: u32 = 7;
const T_STRING: u32 = 8;
const T_ARRAY: u32 = 9;
const T_UINT64: u32 = 10;
const T_INT64: u32 = 11;
const T_FLOAT64: u32 = 12;

/// 解析 GGUF 头的元数据 KV，抽取 `general.architecture/name/size_label/file_type`。
/// 有界：遇到超大数组（如 tokenizer）即停（general.* 都排在其前）。
fn parse_gguf<R: Read + Seek>(r: &mut R) -> std::io::Result<GgufMeta> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;
    if &magic != b"GGUF" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a gguf file",
        ));
    }
    let _version = read_u32(r)?;
    let _tensor_count = read_u64(r)?;
    let kv_count = read_u64(r)?;

    let mut meta = GgufMeta::default();
    for _ in 0..kv_count {
        let key = read_gguf_string(r)?;
        let vtype = read_u32(r)?;
        match (key.as_str(), vtype) {
            ("general.architecture", T_STRING) => meta.arch = read_gguf_string(r)?,
            ("general.name", T_STRING) => meta.name = read_gguf_string(r)?,
            ("general.size_label", T_STRING) => meta.size_label = read_gguf_string(r)?,
            ("general.file_type", T_UINT32) => meta.file_type = Some(read_u32(r)?),
            // 其余值：跳过。遇超大数组会早退（返回已收集的）。
            _ => {
                if !skip_value(r, vtype)? {
                    break;
                }
            }
        }
    }
    Ok(meta)
}

/// 跳过一个值。返回 `false` 表示遇到超大数组、应停止解析（后续都是大块数据）。
fn skip_value<R: Read + Seek>(r: &mut R, vtype: u32) -> std::io::Result<bool> {
    match vtype {
        T_UINT8 | T_INT8 | T_BOOL => seek_fwd(r, 1)?,
        T_UINT16 | T_INT16 => seek_fwd(r, 2)?,
        T_UINT32 | T_INT32 | T_FLOAT32 => seek_fwd(r, 4)?,
        T_UINT64 | T_INT64 | T_FLOAT64 => seek_fwd(r, 8)?,
        T_STRING => {
            let len = read_u64(r)?;
            seek_fwd(r, len)?;
        }
        T_ARRAY => {
            let elem = read_u32(r)?;
            let count = read_u64(r)?;
            // 超大数组（tokenizer 等）：早退，general.* 已在其前解析完。
            if count > 4096 {
                return Ok(false);
            }
            for _ in 0..count {
                if !skip_value(r, elem)? {
                    return Ok(false);
                }
            }
        }
        _ => {
            return Ok(false); // 未知类型：无法安全跳过，停。
        }
    }
    Ok(true)
}

fn seek_fwd<R: Seek>(r: &mut R, n: u64) -> std::io::Result<()> {
    r.seek(SeekFrom::Current(i64::try_from(n).unwrap_or(i64::MAX)))?;
    Ok(())
}

fn read_u32<R: Read>(r: &mut R) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64<R: Read>(r: &mut R) -> std::io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn read_gguf_string<R: Read>(r: &mut R) -> std::io::Result<String> {
    let len = read_u64(r)?;
    // 防御：元数据字符串不该超过 64KiB，超了视为损坏。
    if len > 65_536 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "gguf string too long",
        ));
    }
    let mut buf = vec![0u8; usize::try_from(len).unwrap_or(0)];
    r.read_exact(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// 常见完整量化子串（含下划线，从最具体到最泛，避免 `Q4_0` 抢 `Q4_K_M`）。
const QUANT_PATS: &[&str] = &[
    "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q4_K_S", "Q4_K_M", "Q5_K_S", "Q5_K_M", "Q2_K", "Q6_K", "Q4_0",
    "Q4_1", "Q5_0", "Q5_1", "Q8_0", "IQ1_S", "IQ2_XXS", "IQ2_XS", "IQ3_XXS", "IQ3_S", "IQ4_XS",
    "IQ4_NL", "BF16", "F16", "F32",
];

/// 从文件名抽量化标签（`Q4_K_M` / `IQ4_XS` / `F16` / `BF16` / `Q8_0` ...）。
fn quant_from_name(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    QUANT_PATS
        .iter()
        .find(|p| upper.contains(*p))
        .map(|p| (*p).to_owned())
}

/// 从文件名抽参数量标签（`35B` / `8B` / `70B` / `A3B` 复合尽力）。
fn params_from_name(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    for token in upper.split(['-', '.', '_', ' ']) {
        // 形如 <数字>B，可带小数（如 3.5B）。
        if let Some(stripped) = token.strip_suffix('B')
            && !stripped.is_empty()
            && stripped.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return Some(token.to_owned());
        }
    }
    None
}

/// GGUF `general.file_type` 枚举 -> 量化名（LLAMA_FTYPE 常见值）。
fn ftype_to_quant(ft: u32) -> Option<String> {
    let s = match ft {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        _ => return None,
    };
    Some(s.to_owned())
}

/// 多分片非首片（`...-00002-of-00003.gguf`）？只列首片。
fn is_nonfirst_shard(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    // 匹配 -<num>-of-<num> 结尾，且 num != 00001。
    let up = stem.to_ascii_lowercase();
    if let Some(idx) = up.rfind("-of-") {
        let before = &up[..idx];
        if let Some(dash) = before.rfind('-') {
            let part = &before[dash + 1..];
            if part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty() {
                let trimmed = part.trim_start_matches('0');
                return trimmed != "1" && !trimmed.is_empty();
            }
        }
    }
    false
}

/// 展示用路径：去掉 Windows 扩展长度前缀 `\\?\`。
fn display_path(p: &Path) -> String {
    p.to_string_lossy().trim_start_matches(r"\\?\").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 手搓一个最小 GGUF 字节流：magic + version + 0 tensors + 给定 KV。
    fn build_gguf(kvs: &[(&str, u32, Vec<u8>)]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"GGUF");
        b.extend_from_slice(&3u32.to_le_bytes()); // version
        b.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
        b.extend_from_slice(&(kvs.len() as u64).to_le_bytes());
        for (k, vtype, val) in kvs {
            b.extend_from_slice(&(k.len() as u64).to_le_bytes());
            b.extend_from_slice(k.as_bytes());
            b.extend_from_slice(&vtype.to_le_bytes());
            b.extend_from_slice(val);
        }
        b
    }

    fn gguf_str(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u64).to_le_bytes().to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    }

    #[test]
    fn parse_general_metadata() {
        let bytes = build_gguf(&[
            ("general.architecture", T_STRING, gguf_str("qwen3moe")),
            ("general.name", T_STRING, gguf_str("Qwen3 8B")),
            ("general.size_label", T_STRING, gguf_str("8B")),
            ("general.file_type", T_UINT32, 15u32.to_le_bytes().to_vec()),
        ]);
        let meta = parse_gguf(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(meta.arch, "qwen3moe");
        assert_eq!(meta.name, "Qwen3 8B");
        assert_eq!(meta.size_label, "8B");
        assert_eq!(meta.file_type, Some(15));
        assert_eq!(ftype_to_quant(15).as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn parse_skips_unwanted_and_stops_on_huge_array() {
        let mut kvs: Vec<(&str, u32, Vec<u8>)> = vec![
            ("general.architecture", T_STRING, gguf_str("llama")),
            ("some.u32", T_UINT32, 7u32.to_le_bytes().to_vec()),
        ];
        // 一个 count=100000 的字符串数组：应触发早退，不 OOM。
        let mut huge = Vec::new();
        huge.extend_from_slice(&T_STRING.to_le_bytes());
        huge.extend_from_slice(&100_000u64.to_le_bytes());
        kvs.push(("tokenizer.ggml.tokens", T_ARRAY, huge));
        let bytes = build_gguf(&kvs);
        let meta = parse_gguf(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(meta.arch, "llama");
    }

    #[test]
    fn non_gguf_is_error() {
        assert!(parse_gguf(&mut Cursor::new(b"NOPE....".to_vec())).is_err());
    }

    #[test]
    fn quant_from_filename() {
        assert_eq!(
            quant_from_name("Qwen3.6-35B-A3B-Uncensored-Q4_K_M").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(quant_from_name("model-f16").as_deref(), Some("F16"));
        assert_eq!(quant_from_name("plain-model"), None);
    }

    #[test]
    fn params_from_filename() {
        assert_eq!(params_from_name("Qwen3-8B-instruct").as_deref(), Some("8B"));
        assert_eq!(params_from_name("mixtral-8x7b"), None); // 非纯 NNB，不误判
        assert_eq!(params_from_name("model-70B").as_deref(), Some("70B"));
    }

    #[test]
    fn shard_detection() {
        assert!(is_nonfirst_shard(Path::new("m-00002-of-00003.gguf")));
        assert!(!is_nonfirst_shard(Path::new("m-00001-of-00003.gguf")));
        assert!(!is_nonfirst_shard(Path::new("plain.gguf")));
    }

    #[test]
    fn discover_missing_dir_is_empty() {
        let p = std::env::temp_dir().join("kestrel-no-such-models-dir-xyz");
        assert!(discover_models(&p).is_empty());
    }
}
