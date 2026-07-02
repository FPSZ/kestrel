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
