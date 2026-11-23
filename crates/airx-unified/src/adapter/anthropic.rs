//! 统一请求 → Anthropic Messages；Messages → 统一响应。

use airx_core::AirxError;
use airx_core::json::Extensions;
use airx_core::model::ProviderId;
use airx_protocol::anthropic::messages::{
    ContentBlock, ImageBlock, Message as NativeMessage, MessageContent, MessagesRequest,
    MessagesResponse, Role as NativeRole, TextBlock, Tool as NativeTool, ToolResultBlock,
    ToolUseBlock,
};
use serde_json::{Value, json};

use crate::compatibility::{Adapted, CompatibilityMode, Warning, unsupported};
use crate::content::{Content, Role, TokenUsage, ToolCall};
use crate::request::GenerateRequest;
use crate::response::GenerateResponse;

const DEFAULT_MAX_TOKENS: u32 = 4096;

/// 统一请求 → Anthropic Messages 请求。
pub fn to_anthropic(
    request: &GenerateRequest,
    mode: CompatibilityMode,
) -> Result<Adapted<MessagesRequest>, AirxError> {
    let mut warnings = Vec::new();
    let mut messages = Vec::new();
    let mut system_parts = Vec::new();

    if let Some(instructions) = &request.instructions {
        system_parts.push(instructions.clone());
    }
    for message in &request.messages {
        if message.role == Role::System {
            let text = message.text();
            if text.is_empty() {
                unsupported(
                    mode,
                    &mut warnings,
                    "system 消息包含非文本内容，Anthropic 无法表达",
                )?;
            } else {
                system_parts.push(text);
            }
        }
    }

    for message in &request.messages {
        match message.role {
            Role::System => continue,
            Role::User => messages.push(NativeMessage {
                role: NativeRole::User,
                content: convert_content(&message.content, true, mode, &mut warnings)?,
            }),
            Role::Assistant => messages.push(NativeMessage {
                role: NativeRole::Assistant,
                content: convert_content(&message.content, false, mode, &mut warnings)?,
            }),
            Role::Tool => {
                let mut blocks = Vec::new();
                for content in &message.content {
                    match content {
                        Content::ToolResult(result) => {
                            blocks.push(ContentBlock::ToolResult(ToolResultBlock {
                                tool_use_id: result.tool_call_id.clone(),
                                content: Some(Value::String(result.content.clone())),
                                is_error: result.is_error.then_some(true),
                                extensions: Extensions::new(),
                            }));
                        }
                        other => unsupported(
                            mode,
                            &mut warnings,
                            format!("tool 消息中的 {other:?} 无法映射到 Anthropic"),
                        )?,
                    }
                }
                messages.push(NativeMessage {
                    role: NativeRole::User,
                    content: MessageContent::Blocks(blocks),
                });
            }
        }
    }

    let tools = if request.tools.is_empty() {
        None
    } else {
        Some(
            request
                .tools
                .iter()
                .map(|tool| {
                    let mut native = NativeTool::new(tool.name.clone(), tool.parameters.clone());
                    native.description = tool.description.clone();
                    native
                })
                .collect(),
        )
    };

    let mut extensions = Extensions::new();
    if let Some(Value::Object(map)) = request.extensions.anthropic.clone() {
        for (key, value) in map {
            extensions.insert(key, value);
        }
    } else if request.extensions.anthropic.is_some() {
        unsupported(mode, &mut warnings, "Anthropic 扩展必须是 JSON 对象")?;
    }

    let native = MessagesRequest {
        model: request.model.clone(),
        max_tokens: request.options.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        messages,
        system: (!system_parts.is_empty()).then(|| system_parts.join("\n\n")),
        temperature: request.options.temperature,
        top_p: request.options.top_p,
        top_k: None,
        stop_sequences: (!request.options.stop.is_empty()).then(|| request.options.stop.clone()),
        tools,
        tool_choice: None,
        thinking: None,
        stream: None,
        extensions,
    };

    Ok(Adapted { native, warnings })
}

fn convert_content(
    contents: &[Content],
    is_user: bool,
    mode: CompatibilityMode,
    warnings: &mut Vec<Warning>,
) -> Result<MessageContent, AirxError> {
    let mut blocks = Vec::new();
    let mut only_text = true;

    for content in contents {
        match content {
            Content::Text(text) => blocks.push(ContentBlock::Text(TextBlock {
                text: text.clone(),
                extensions: Extensions::new(),
            })),
            Content::Image(image) => {
                only_text = false;
                blocks.push(ContentBlock::Image(ImageBlock {
                    source: json!({"type": "url", "url": image.url}),
                    extensions: Extensions::new(),
                }));
            }
            Content::ToolCall(call) if !is_user => {
                only_text = false;
                blocks.push(ContentBlock::ToolUse(ToolUseBlock {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    input: call.arguments.clone(),
                    extensions: Extensions::new(),
                }));
            }
            Content::ToolResult(result) => {
                only_text = false;
                blocks.push(ContentBlock::ToolResult(ToolResultBlock {
                    tool_use_id: result.tool_call_id.clone(),
                    content: Some(Value::String(result.content.clone())),
                    is_error: result.is_error.then_some(true),
                    extensions: Extensions::new(),
                }));
            }
            Content::Thinking(thinking) if !is_user => unsupported(
                mode,
                warnings,
                format!(
                    "Anthropic 需要 thinking signature 才能回传历史思考，当前内容无法表达：{thinking}"
                ),
            )?,
            Content::Custom { data, .. } => {
                only_text = false;
                blocks.push(ContentBlock::Other(data.clone()));
            }
            other => unsupported(
                mode,
                warnings,
                format!("{other:?} 无法映射到 Anthropic 消息"),
            )?,
        }
    }

    if only_text && blocks.len() == 1 {
        if let Some(ContentBlock::Text(text)) = blocks.into_iter().next() {
            return Ok(MessageContent::Text(text.text));
        }
        unreachable!("only_text 保证只有一个文本块");
    }

    Ok(MessageContent::Blocks(blocks.into_iter().collect()))
}

/// Messages 响应 → 统一响应。
pub fn from_anthropic(response: MessagesResponse) -> GenerateResponse {
    let mut content = Vec::new();

    for block in response.content {
        match block {
            ContentBlock::Text(text) => content.push(Content::Text(text.text)),
            ContentBlock::ToolUse(call) => content.push(Content::ToolCall(ToolCall {
                id: call.id,
                name: call.name,
                arguments: call.input,
            })),
            ContentBlock::Thinking(block) => content.push(Content::Thinking(block.thinking)),
            other => {
                let value = serde_json::to_value(&other).unwrap_or(Value::Null);
                let kind = value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
                content.push(Content::Custom { kind, data: value });
            }
        }
    }

    let usage = response.usage.map(|usage| TokenUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.input_tokens + usage.output_tokens,
    });

    GenerateResponse {
        provider: ProviderId::anthropic(),
        id: Some(response.id),
        model: Some(response.model),
        content,
        usage,
        finish_reason: response.stop_reason,
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ImageContent, Message};
    use serde_json::json;

    fn request() -> GenerateRequest {
        GenerateRequest::builder()
            .model("claude-3-5-sonnet")
            .instructions("be brief")
            .user("hi")
            .max_tokens(128)
            .build()
            .expect("request")
    }

    #[test]
    fn maps_system_and_messages() {
        let adapted = to_anthropic(&request(), CompatibilityMode::Strict).expect("adapt");
        assert_eq!(adapted.native.system.as_deref(), Some("be brief"));
        assert_eq!(adapted.native.max_tokens, 128);
        match &adapted.native.messages[0].content {
            MessageContent::Text(text) => assert_eq!(text, "hi"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn images_become_blocks() {
        let mut request = request();
        request.messages.push(Message {
            role: Role::User,
            content: vec![
                Content::Text("look".to_owned()),
                Content::Image(ImageContent::new("https://example.com/a.png")),
            ],
        });
        let adapted = to_anthropic(&request, CompatibilityMode::Strict).expect("adapt");
        match &adapted.native.messages[1].content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert!(matches!(blocks[1], ContentBlock::Image(_)));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn assistant_thinking_is_strict_error() {
        let mut request = request();
        request.messages.push(Message {
            role: Role::Assistant,
            content: vec![Content::Thinking("hmm".to_owned())],
        });
        assert!(to_anthropic(&request, CompatibilityMode::Strict).is_err());

        let adapted = to_anthropic(&request, CompatibilityMode::BestEffort).expect("adapt");
        assert_eq!(adapted.warnings.len(), 1);
    }

    #[test]
    fn response_maps_blocks_and_usage() {
        let response: MessagesResponse = serde_json::from_value(json!({
            "id": "msg_1",
            "model": "claude",
            "content": [
                {"type": "text", "text": "ok"},
                {"type": "thinking", "thinking": "..."},
                {"type": "tool_use", "id": "t1", "name": "lookup", "input": {"q": 1}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 3, "output_tokens": 4}
        }))
        .expect("response");

        let unified = from_anthropic(response);
        assert_eq!(unified.text(), "ok");
        assert_eq!(unified.tool_calls()[0].name, "lookup");
        assert_eq!(unified.usage.map(|u| u.total_tokens), Some(7));
        assert_eq!(unified.finish_reason.as_deref(), Some("tool_use"));
    }
}
