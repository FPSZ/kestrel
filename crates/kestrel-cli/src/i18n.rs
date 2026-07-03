//! CLI 用户可见文本的极简 catalog（地基 #1/#9，ADR-0008 表现层本地化）。
//!
//! - **只管用户可见文本**：REPL 提示、slash 回执、事件渲染标签。tracing 开发日志
//!   保持英文 / 结构化（§9 日志 vs UI 分离），不走这里。
//! - locale 判定：`KESTREL_LOCALE` > `LC_ALL` > `LC_MESSAGES` > `LANG`；`zh*` -> zh-CN，
//!   其余 -> en-US（进程内只判一次）。读 env 属表现层边缘，不进 core 事件路径。
//! - 英文 key、en-US 回退；未命中 key 原样返回 key（暴露漏翻，不静默）。

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Locale {
    EnUs,
    ZhCn,
}

fn locale() -> Locale {
    static L: OnceLock<Locale> = OnceLock::new();
    *L.get_or_init(|| {
        let raw = std::env::var("KESTREL_LOCALE")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_lowercase();
        if raw.starts_with("zh") {
            Locale::ZhCn
        } else {
            Locale::EnUs
        }
    })
}

/// 查一条用户可见文本。en-US 与 zh-CN 双语；未命中 key 原样返回 key（故 key 取
/// `&'static str`——调用点一律传字面量）。
#[must_use]
pub fn t(key: &'static str) -> &'static str {
    let (en, zh) = match key {
        "cli.help.commands" => (
            "Commands: /think on|off   /mode ask|auto|plan   /help   /quit",
            "命令：/think on|off   /mode ask|auto|plan   /help   /quit",
        ),
        "cli.current" => ("current", "当前"),
        "cli.think" => ("thinking", "思考"),
        "cli.mode" => ("mode", "模式"),
        "cli.think.usage" => ("/think expects on|off", "/think 需 on|off"),
        "cli.mode.usage" => ("/mode expects ask|auto|plan", "/mode 需 ask|auto|plan"),
        "cli.mode.ask" => ("ask", "询问(ask)"),
        "cli.mode.auto" => ("execute-all", "全部执行(auto)"),
        "cli.mode.plan" => ("plan", "计划(plan)"),
        "cli.tool.call" => ("[call]", "[调用]"),
        "cli.tool.result" => ("result", "结果"),
        "cli.tool.failed" => ("failed", "失败"),
        "cli.approve.prompt" => ("approve?", "批准?"),
        "cli.error" => ("[error]", "[错误]"),
        "cli.startup.hint" => (
            "Type a message to start; /quit to exit.",
            "输入消息开始对话，/quit 退出。",
        ),
        _ => return key, // 未命中：原样返回，暴露漏翻。
    };
    match locale() {
        Locale::ZhCn => zh,
        Locale::EnUs => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_key_returns_itself() {
        assert_eq!(t("cli.nonexistent"), "cli.nonexistent");
    }

    #[test]
    fn known_key_resolves_to_a_locale_value() {
        // 不同 locale 下的具体串取决于运行环境；这里只断言命中 key 不回落成 key 本身。
        assert_ne!(t("cli.tool.call"), "cli.tool.call");
    }
}
