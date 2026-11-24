//! Chat Completions 流式 chunk。

use airx_core::json::Extensions;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::usage::Usage;

/// 流式 chunk。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// 响应 ID。
    pub id: String,
    /// 对象类型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    /// 创建时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    /// 模型。
    pub model: String,
    /// 候选增量（usage 专用 chunk 为空）。
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    /// 用量（仅 `include_usage` 时的最后一个 chunk）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// 系统指纹（与完整响应同名字段一致）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// 实际服务等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 候选增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkChoice {
    /// 候选序号。
    pub index: u32,
    /// 增量。
    pub delta: ChunkDelta,
    /// 结束原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// 对数概率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Value>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 增量内容。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChunkDelta {
    /// 角色（仅首个 chunk）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// 文本增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// 推理增量（DeepSeek / 部分兼容网关）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// 服务端联网工具产生的引用（DeepSeek 在最终 chunk 一次性返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<Value>>,
    /// 服务端工具调用记录（DeepSeek 在最终 chunk 一次性返回）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_tool_calls: Option<Vec<Value>>,
    /// 拒答增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// 已弃用的 function_call 增量（官方保留旧字段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call: Option<ChunkFunctionCall>,
    /// 工具调用增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChunkToolCall>>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 工具调用增量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkToolCall {
    /// 工具调用序号（用于合并）。
    pub index: u32,
    /// 调用 ID（仅首个增量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// 类型。
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// 函数增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<ChunkFunctionCall>,
    /// custom 工具增量（`{name?, input?}`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<ChunkCustomToolCall>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// custom 工具调用增量。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChunkCustomToolCall {
    /// 工具名（仅首个增量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// free-form 输入增量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// 函数调用增量。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChunkFunctionCall {
    /// 函数名（仅首个增量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 参数增量（JSON 字符串，可能不完整）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_text_and_tool_call_deltas() {
        let json = json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant",
                    "tool_calls": [{
                        "index": 0,
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":"}
                    }]
                }
            }]
        });

        let chunk: ChatCompletionChunk = serde_json::from_value(json).expect("chunk");
        let delta = &chunk.choices[0].delta;
        assert_eq!(delta.role.as_deref(), Some("assistant"));
        let call = &delta.tool_calls.as_ref().expect("tool calls")[0];
        assert_eq!(call.index, 0);
        assert_eq!(
            call.function.as_ref().expect("function").name.as_deref(),
            Some("lookup")
        );
    }
}

#[cfg(test)]
mod reasoning_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_streaming_reasoning_content() {
        let chunk: ChatCompletionChunk = serde_json::from_value(json!({
            "id": "1",
            "object": "chat.completion.chunk",
            "model": "m",
            "choices": [{"index": 0, "delta": {"reasoning_content": "用户"}}]
        }))
        .expect("chunk");
        assert_eq!(
            chunk.choices[0].delta.reasoning_content.as_deref(),
            Some("用户")
        );
    }
}

#[cfg(test)]
mod typed_stream_fields_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chunk_system_fingerprint_and_service_tier_are_typed() {
        let chunk: ChatCompletionChunk = serde_json::from_value(json!({
            "id": "1",
            "model": "m",
            "system_fingerprint": "fp_1",
            "service_tier": "default",
            "choices": []
        }))
        .expect("chunk");
        assert_eq!(chunk.system_fingerprint.as_deref(), Some("fp_1"));
        assert_eq!(chunk.service_tier.as_deref(), Some("default"));
        assert!(!chunk.extensions.contains_key("system_fingerprint"));
        assert!(!chunk.extensions.contains_key("service_tier"));
    }

    #[test]
    fn delta_refusal_and_function_call_are_typed() {
        let chunk: ChatCompletionChunk = serde_json::from_value(json!({
            "id": "1",
            "model": "m",
            "choices": [{
                "index": 0,
                "delta": {
                    "refusal": "cannot",
                    "function_call": {"name": "lookup", "arguments": "{}"}
                }
            }]
        }))
        .expect("chunk");
        let delta = &chunk.choices[0].delta;
        assert_eq!(delta.refusal.as_deref(), Some("cannot"));
        assert_eq!(
            delta
                .function_call
                .as_ref()
                .and_then(|call| call.name.as_deref()),
            Some("lookup")
        );
        assert!(delta.extensions.is_empty());
    }
}
