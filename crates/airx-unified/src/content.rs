//! 统一内容模型。
//!
//! 这是 **Unified API 的类型**，不是任何 Provider 的原生类型。
//! 转换无法表达时，严格模式报错，宽松模式产生显式 Warning。

use serde_json::Value;

/// 角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 系统。
    System,
    /// 用户。
    User,
    /// 助手。
    Assistant,
    /// 工具结果。
    Tool,
}

/// 统一内容。
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// 文本。
    Text(String),
    /// 图片。
    Image(ImageContent),
    /// 工具调用。
    ToolCall(ToolCall),
    /// 工具结果。
    ToolResult(ToolResult),
    /// thinking / reasoning。
    Thinking(String),
    /// Provider 专属内容（保留原始 JSON）。
    Custom {
        /// 类型名。
        kind: String,
        /// 原始数据。
        data: Value,
    },
}

/// 图片内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageContent {
    /// URL 或 data URI。
    pub url: String,
    /// 细节级别。
    pub detail: Option<String>,
}

impl ImageContent {
    /// 构造图片内容。
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            detail: None,
        }
    }
}

/// 工具调用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// 调用 ID。
    pub id: String,
    /// 工具名。
    pub name: String,
    /// 参数（原始 JSON）。
    pub arguments: Value,
}

/// 工具结果。
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    /// 对应的调用 ID。
    pub tool_call_id: String,
    /// 结果内容。
    pub content: String,
    /// 是否错误。
    pub is_error: bool,
}

/// 统一消息。
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// 角色。
    pub role: Role,
    /// 内容。
    pub content: Vec<Content>,
}

impl Message {
    /// 用户文本消息。
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![Content::Text(text.into())],
        }
    }

    /// 助手文本消息。
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![Content::Text(text.into())],
        }
    }

    /// 系统消息。
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![Content::Text(text.into())],
        }
    }

    /// 文本内容拼接。
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|content| match content {
                Content::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// 统一 token 用量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    /// 输入 token。
    pub input_tokens: u32,
    /// 输出 token。
    pub output_tokens: u32,
    /// 总计。
    pub total_tokens: u32,
}
