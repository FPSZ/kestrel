//! 内置工具集构造：把具体工具装配进 core 的 [`ToolSet`]。
//!
//! 注册表类型（ToolSet）在 core（core 拥有 Tool trait）；本模块只负责
//! 提供 M1 的具体工具组合，保持 core -> tools 的依赖方向不被反转。

use std::sync::Arc;

use kestrel_core::ToolSet;

use crate::{EditTool, ReadTool, SearchTool, ShellTool};

/// M1 内置工具集：read / search / edit / shell（顺序稳定 = 前缀稳定）。
#[must_use]
pub fn builtin() -> ToolSet {
    let mut set = ToolSet::new();
    set.register(Arc::new(ReadTool::default()));
    set.register(Arc::new(SearchTool::default()));
    set.register(Arc::new(EditTool::default()));
    set.register(Arc::new(ShellTool::default()));
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_four_tools_in_stable_order() {
        let set = builtin();
        let names: Vec<_> = set.specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["read", "search", "edit", "shell"]);
    }

    #[test]
    fn unknown_tool_is_none() {
        assert!(builtin().get("teleport").is_none());
    }
}
