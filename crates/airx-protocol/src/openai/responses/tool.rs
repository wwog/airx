//! Responses 工具声明。
//!
//! 服务端工具（`web_search` 等）由 OpenAI 后台执行；
//! SDK 只发送声明并解析返回的 `web_search_call` / 引用标注，不发起本地网络请求。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::chat::FunctionTool;

/// Responses 工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsesTool {
    /// 客户端函数工具。
    Function(FunctionTool),
    /// 服务端联网搜索。
    WebSearch(WebSearchTool),
    /// 其他服务端工具 / 未来新增工具（原样透传）。
    Other(Value),
}

impl ResponsesTool {
    /// 构造 function 工具。
    pub fn function(
        name: impl Into<String>,
        parameters: Value,
        description: Option<String>,
    ) -> Self {
        Self::Function(FunctionTool::new(name, parameters, description))
    }

    /// 构造默认的联网搜索工具（`type = web_search`）。
    pub fn web_search() -> Self {
        Self::WebSearch(WebSearchTool::default())
    }

    /// 以原始 JSON 声明工具。
    pub fn raw(value: Value) -> Self {
        Self::Other(value)
    }
}

/// 联网搜索工具。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchTool {
    /// 工具类型。
    #[serde(rename = "type", default)]
    pub kind: WebSearchKind,
    /// 是否允许实时联网（false 表示仅用缓存）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_web_access: Option<bool>,
    /// 域名过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<WebSearchFilters>,
    /// 用户位置（原样结构）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<Value>,
    /// 上下文占用等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<SearchContextSize>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self {
            kind: WebSearchKind::WebSearch,
            external_web_access: None,
            filters: None,
            user_location: None,
            search_context_size: None,
            extensions: Extensions::new(),
        }
    }
}

/// 搜索工具类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSearchKind {
    /// 稳定别名。
    #[default]
    #[serde(rename = "web_search")]
    WebSearch,
    /// 版本化名称。
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch2025_08_26,
}

/// 域名过滤。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WebSearchFilters {
    /// 只允许这些域名（含子域）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 搜索上下文等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchContextSize {
    /// 低。
    Low,
    /// 中。
    Medium,
    /// 高。
    High,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn web_search_tool_serializes_and_roundtrips() {
        let tool = ResponsesTool::web_search();
        let value = serde_json::to_value(&tool).expect("serialize");
        assert_eq!(value, json!({"type": "web_search"}));

        let parsed: ResponsesTool =
            serde_json::from_value(json!({"type": "web_search_2025_08_26"})).expect("parse");
        match parsed {
            ResponsesTool::WebSearch(tool) => {
                assert_eq!(tool.kind, WebSearchKind::WebSearch2025_08_26)
            }
            other => panic!("unexpected: {other:?}"),
        }

        // 未来服务端工具原样保留。
        let other: ResponsesTool =
            serde_json::from_value(json!({"type": "future_tool"})).expect("parse");
        assert!(matches!(other, ResponsesTool::Other(_)));
    }
}
