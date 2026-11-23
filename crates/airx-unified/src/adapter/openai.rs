//! 统一请求 → OpenAI Chat；OpenAI Chat → 统一响应。
//!
//! 纯函数：不读环境、不发请求。无法表达的内容按
//! [`CompatibilityMode`] 报错或产生 Warning。

use airx_core::AirxError;
use airx_core::error::SerializationError;
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use airx_protocol::openai::chat::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ContentPart, ImageUrl,
    Tool as NativeTool, ToolCall as NativeToolCall,
};
use serde_json::Value;

use crate::compatibility::{Adapted, CompatibilityMode, unsupported};
use crate::content::{Content, Role, TokenUsage, ToolCall};
use crate::request::GenerateRequest;
use crate::response::GenerateResponse;

/// 统一请求 → OpenAI Chat 请求。
pub fn to_openai(
    request: &GenerateRequest,
    mode: CompatibilityMode,
) -> Result<Adapted<ChatCompletionRequest>, AirxError> {
    let mut warnings = Vec::new();
    let mut messages = Vec::new();

    if let Some(instructions) = &request.instructions {
        messages.push(ChatMessage::system(instructions.clone()));
    }

    for message in &request.messages {
        match message.role {
            Role::System => {
                let text = message.text();
                if text.is_empty() {
                    unsupported(
                        mode,
                        &mut warnings,
                        "system 消息包含非文本内容，OpenAI Chat 无法表达",
                    )?;
                } else {
                    messages.push(ChatMessage::system(text));
                }
            }
            Role::User => convert_user(message, mode, &mut warnings, &mut messages)?,
            Role::Assistant => convert_assistant(message, mode, &mut warnings, &mut messages)?,
            Role::Tool => {
                for content in &message.content {
                    match content {
                        Content::ToolResult(result) => messages.push(ChatMessage::tool_result(
                            result.tool_call_id.clone(),
                            result.content.clone(),
                        )),
                        other => unsupported(
                            mode,
                            &mut warnings,
                            format!("tool 消息中的 {other:?} 无法映射到 OpenAI tool 消息"),
                        )?,
                    }
                }
            }
        }
    }

    let mut extensions = Extensions::new();
    merge_extensions(
        &mut extensions,
        request.extensions.openai.clone(),
        mode,
        &mut warnings,
    )?;

    let tools = if request.tools.is_empty() {
        None
    } else {
        Some(
            request
                .tools
                .iter()
                .map(|tool| {
                    NativeTool::function(
                        tool.name.clone(),
                        tool.parameters.clone(),
                        tool.description.clone(),
                    )
                })
                .collect(),
        )
    };

    let native = ChatCompletionRequest {
        model: request.model.clone(),
        messages,
        temperature: request.options.temperature,
        top_p: request.options.top_p,
        max_tokens: request.options.max_tokens,
        stop: (!request.options.stop.is_empty()).then(|| request.options.stop.clone()),
        user: None,
        tools,
        tool_choice: None,
        parallel_tool_calls: None,
        response_format: None,
        stream: None,
        stream_options: None,
        extensions,
    };

    Ok(Adapted { native, warnings })
}

fn convert_user(
    message: &crate::content::Message,
    mode: CompatibilityMode,
    warnings: &mut Vec<crate::compatibility::Warning>,
    messages: &mut Vec<ChatMessage>,
) -> Result<(), AirxError> {
    let mut texts = Vec::new();
    let mut parts = Vec::new();
    let mut has_image = false;

    for content in &message.content {
        match content {
            Content::Text(text) => {
                texts.push(text.clone());
                parts.push(ContentPart::Text { text: text.clone() });
            }
            Content::Image(image) => {
                has_image = true;
                parts.push(ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: image.url.clone(),
                        detail: image.detail.clone(),
                        extensions: Extensions::new(),
                    },
                });
            }
            Content::ToolResult(result) => messages.push(ChatMessage::tool_result(
                result.tool_call_id.clone(),
                result.content.clone(),
            )),
            other => unsupported(
                mode,
                warnings,
                format!("user 消息中的 {other:?} 无法映射到 OpenAI"),
            )?,
        }
    }

    if has_image {
        if parts.is_empty() {
            unsupported(mode, warnings, "user 消息没有可发送的内容")?;
        } else {
            messages.push(ChatMessage::user_parts(parts));
        }
    } else if texts.is_empty() {
        unsupported(mode, warnings, "user 消息没有文本内容")?;
    } else {
        messages.push(ChatMessage::user(texts.concat()));
    }
    Ok(())
}

fn convert_assistant(
    message: &crate::content::Message,
    mode: CompatibilityMode,
    warnings: &mut Vec<crate::compatibility::Warning>,
    messages: &mut Vec<ChatMessage>,
) -> Result<(), AirxError> {
    let mut text = String::new();
    let mut calls = Vec::new();

    for content in &message.content {
        match content {
            Content::Text(part) => text.push_str(part),
            Content::ToolCall(call) => calls.push(NativeToolCall::function(
                call.id.clone(),
                call.name.clone(),
                serde_json::to_string(&call.arguments)
                    .map_err(|error| AirxError::Serialization(SerializationError::from(error)))?,
            )),
            Content::Thinking(_) => {
                unsupported(mode, warnings, "OpenAI Chat 不支持回传 thinking 内容")?
            }
            other => unsupported(
                mode,
                warnings,
                format!("assistant 消息中的 {other:?} 无法映射到 OpenAI"),
            )?,
        }
    }

    if calls.is_empty() {
        messages.push(ChatMessage::assistant(text));
    } else {
        messages.push(ChatMessage::Assistant {
            content: (!text.is_empty()).then_some(text),
            tool_calls: Some(calls),
        });
    }
    Ok(())
}

fn merge_extensions(
    target: &mut Extensions,
    value: Option<Value>,
    mode: CompatibilityMode,
    warnings: &mut Vec<crate::compatibility::Warning>,
) -> Result<(), AirxError> {
    match value {
        None => Ok(()),
        Some(Value::Object(map)) => {
            for (key, value) in map {
                target.insert(key, value);
            }
            Ok(())
        }
        Some(other) => unsupported(
            mode,
            warnings,
            format!("OpenAI 扩展必须是 JSON 对象，实际为 {other}"),
        ),
    }
}

/// OpenAI Chat 响应 → 统一响应。
pub fn from_openai(response: ChatCompletionResponse) -> GenerateResponse {
    let mut content = Vec::new();
    let mut finish_reason = None;

    if let Some(choice) = response.choices.first() {
        finish_reason = choice.finish_reason.clone();
        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content.push(Content::Text(text.clone()));
            }
        }
        if let Some(reasoning) = choice
            .message
            .extensions
            .get("reasoning_content")
            .and_then(Value::as_str)
        {
            if !reasoning.is_empty() {
                content.push(Content::Thinking(reasoning.to_owned()));
            }
        }
        if let Some(calls) = &choice.message.tool_calls {
            for call in calls {
                content.push(Content::ToolCall(ToolCall {
                    id: call.id.clone(),
                    name: call.function.name.clone(),
                    arguments: parse_arguments(&call.function.arguments),
                }));
            }
        }
    }

    let usage = response.usage.map(|usage| TokenUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
    });

    GenerateResponse {
        provider: ProviderId::openai(),
        id: Some(response.id),
        model: Some(response.model),
        content,
        usage,
        finish_reason,
        warnings: Vec::new(),
    }
}

pub(crate) fn parse_arguments(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ImageContent, Message};
    use serde_json::json;

    fn request() -> GenerateRequest {
        GenerateRequest::builder()
            .model("gpt-4o-mini")
            .instructions("be brief")
            .user("hi")
            .tool(crate::request::Tool::new(
                "lookup",
                json!({"type": "object"}),
            ))
            .temperature(0.2)
            .build()
            .expect("request")
    }

    #[test]
    fn maps_instructions_messages_and_tools() {
        let adapted = to_openai(&request(), CompatibilityMode::Strict).expect("adapt");
        assert_eq!(adapted.native.messages[0].role(), "system");
        assert_eq!(adapted.native.messages[1].content(), Some("hi"));
        assert_eq!(adapted.native.tools.as_ref().map(Vec::len), Some(1));
        assert_eq!(adapted.native.temperature, Some(0.2));
        assert!(adapted.warnings.is_empty());
    }

    #[test]
    fn multimodal_user_message_uses_parts() {
        let mut request = request();
        request.messages.push(Message {
            role: Role::User,
            content: vec![
                Content::Text("what is this?".to_owned()),
                Content::Image(ImageContent::new("https://example.com/a.png")),
            ],
        });

        let adapted = to_openai(&request, CompatibilityMode::Strict).expect("adapt");
        let last = adapted.native.messages.last().expect("message");
        match last {
            ChatMessage::User { content } => match content {
                airx_protocol::openai::chat::UserContent::Parts(parts) => {
                    assert_eq!(parts.len(), 2);
                }
                other => panic!("unexpected content: {other:?}"),
            },
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn thinking_is_strict_error_or_warning() {
        let mut request = request();
        request.messages.push(Message {
            role: Role::Assistant,
            content: vec![Content::Thinking("hidden".to_owned())],
        });

        assert!(to_openai(&request, CompatibilityMode::Strict).is_err());

        let adapted = to_openai(&request, CompatibilityMode::BestEffort).expect("adapt");
        assert_eq!(adapted.warnings.len(), 1);
    }

    #[test]
    fn response_maps_text_tool_calls_and_usage() {
        let response: ChatCompletionResponse = serde_json::from_value(json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "ok",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
        }))
        .expect("response");

        let mut unified = from_openai(response);
        assert_eq!(unified.text(), "ok");
        assert_eq!(unified.tool_calls()[0].arguments["q"], "x");
        assert_eq!(unified.usage.map(|u| u.total_tokens), Some(3));

        // 无文本、仅工具调用时 text() 为空。
        unified
            .content
            .retain(|content| !matches!(content, Content::Text(_)));
        assert_eq!(unified.text(), "");
    }
}
