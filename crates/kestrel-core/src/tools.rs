//! 工具集合：主循环持有的工具查找表。
//!
//! 类型在 core（core 拥有 [`Tool`] trait）；具体工具实现在 kestrel-tools。
//! 函数名注册表校验（§8）：小模型爱幻觉函数名，执行前查表，未命中给可操作错误。

use std::collections::HashMap;
use std::sync::Arc;

use kestrel_protocol::ToolSpec;

use crate::ports::Tool;

/// 一组已注册工具，按注册顺序保持稳定（顺序稳定 = 前缀稳定）。
#[derive(Default, Clone)]
pub struct ToolSet {
    tools: HashMap<String, Arc<dyn Tool>>,
    order: Vec<String>,
}

impl ToolSet {
    /// 空集合。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个工具（同名后注册者覆盖，顺序不变）。
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.spec().name.clone();
        if !self.tools.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.tools.insert(name, tool);
    }

    /// 按注册顺序返回全部工具规格。
    #[must_use]
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.order
            .iter()
            .filter_map(|n| self.tools.get(n))
            .map(|t| t.spec().clone())
            .collect()
    }

    /// 预过滤：移除全局 deny 名单里的工具（deny 优先，docs/architecture.md §5.3）。
    ///
    /// 在模型看到工具列表之前就删掉——既是安全边界，也省下这些工具的 schema
    /// token（原则 2）。返回实际移除的数量。
    pub fn deny(&mut self, names: &[String]) -> usize {
        let mut removed = 0;
        for name in names {
            if self.tools.remove(name).is_some() {
                self.order.retain(|n| n != name);
                removed += 1;
            }
        }
        removed
    }

    /// 按名取工具。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// 已注册工具数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// 是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use kestrel_protocol::RiskLevel;

    use super::*;
    use crate::CoreError;
    use crate::ports::{ToolCtx, ToolOutput};

    struct Noop(ToolSpec);

    #[async_trait]
    impl Tool for Noop {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        fn risk(&self, _a: &serde_json::Value) -> RiskLevel {
            RiskLevel::ReadOnly
        }
        async fn call(&self, _a: serde_json::Value, _c: &ToolCtx) -> Result<ToolOutput, CoreError> {
            Ok(ToolOutput {
                ok: true,
                content: String::new(),
            })
        }
    }

    fn tool(name: &str) -> Arc<dyn Tool> {
        Arc::new(Noop(ToolSpec {
            name: name.to_owned(),
            description: name.to_owned(),
            parameters: serde_json::json!({"type": "object"}),
        }))
    }

    #[test]
    fn deny_prefilters_named_tools_and_keeps_order() {
        let mut set = ToolSet::new();
        for n in ["read", "search", "edit", "shell"] {
            set.register(tool(n));
        }
        let removed = set.deny(&["shell".to_owned(), "absent".to_owned()]);
        assert_eq!(removed, 1, "only the present denied tool counts");
        assert!(set.get("shell").is_none(), "denied tool is gone");
        let names: Vec<_> = set.specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["read", "search", "edit"], "order preserved");
    }
}
