//! shell 工具：执行命令（Windows 上 PowerShell，其他平台 sh）。
//!
//! 实现要点：
//! - 输出按 `ToolCtx::max_output_bytes` head-tail 截断（摄入即截断，§5.2）。
//! - 取消信号必须真正杀掉子进程树（§5.1 铁律）。
//! - risk()：默认 Mutating；识别到删除/系统目录写入等模式升级 Destructive；
//!   识别到出网命令升级 External。宁高勿低。

// TODO(M1): pub struct ShellTool; impl Tool for ShellTool
