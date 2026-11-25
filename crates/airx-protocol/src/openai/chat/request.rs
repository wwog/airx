//! Chat Completions 请求。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::json::Extensions;
use airx_core::model::ModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::reasoning::{ReasoningEffort, Verbosity};

use super::message::ChatMessage;
use super::tool::{ResponseFormat, Tool, ToolChoice};

/// 提示词缓存选项（Chat）。
///
/// 官方仅 `gpt-5.6+` 支持 `mode` / `ttl`；airx 不做模型能力校验。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PromptCacheOptions {
    /// `implicit`（默认）或 `explicit`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// 缓存断点的最短生命周期（`30m`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

/// Chat Completions 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// 模型。
    pub model: ModelId,
    /// 消息列表。
    pub messages: Vec<ChatMessage>,
    /// 采样温度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 核采样。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// 频率惩罚（降低重复 token）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// 存在惩罚（鼓励新话题）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// 最大输出 token（已弃用；o-series 不兼容，推荐 `max_completion_tokens`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// 最大输出 token（含推理 token，官方推荐字段）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    /// 推理投入等级。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// 输出详尽程度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    /// 停止序列。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// 是否返回对数概率。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// 每个 token 位置返回的候选数（需 `logprobs = true`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    /// 一次返回的候选数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// 可复现实验的随机种子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// 终端用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// 安全标识（官方推荐替代 `user`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    /// Prompt 缓存键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// Prompt 缓存选项。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_options: Option<PromptCacheOptions>,
    /// Prompt 缓存保留策略（已弃用；推荐 `prompt_cache_options.ttl`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<String>,
    /// 服务等级（`auto` / `default` / `flex` / `priority` 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// 是否存储本次补全（stored completions 的前置条件）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// 元数据（键值对，便于审计 / 筛选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// 工具列表。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// 工具选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// 是否允许并行工具调用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// 结构化输出格式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// 流式选项（仅 `Chat::stream` 生效）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    /// 未识别的扩展字段。
    #[serde(flatten)]
    pub extensions: Extensions,
}

impl ChatCompletionRequest {
    /// 创建构建器。
    pub fn builder() -> ChatCompletionRequestBuilder {
        ChatCompletionRequestBuilder::default()
    }
}

/// 流式选项。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOptions {
    /// 最后一个 chunk 是否携带 usage。
    #[serde(default)]
    pub include_usage: bool,
    /// 是否启用流式混淆（`false` 可省带宽）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

/// [`ChatCompletionRequest`] 构建器。
#[derive(Debug, Default, Clone)]
pub struct ChatCompletionRequestBuilder {
    model: Option<ModelId>,
    messages: Vec<ChatMessage>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    frequency_penalty: Option<f32>,
    presence_penalty: Option<f32>,
    max_tokens: Option<u32>,
    max_completion_tokens: Option<u32>,
    reasoning_effort: Option<ReasoningEffort>,
    verbosity: Option<Verbosity>,
    stop: Option<Vec<String>>,
    logprobs: Option<bool>,
    top_logprobs: Option<u32>,
    n: Option<u32>,
    seed: Option<i64>,
    user: Option<String>,
    safety_identifier: Option<String>,
    prompt_cache_key: Option<String>,
    prompt_cache_options: Option<PromptCacheOptions>,
    prompt_cache_retention: Option<String>,
    service_tier: Option<String>,
    store: Option<bool>,
    metadata: Option<Value>,
    tools: Option<Vec<Tool>>,
    tool_choice: Option<ToolChoice>,
    parallel_tool_calls: Option<bool>,
    response_format: Option<ResponseFormat>,
    stream_options: Option<StreamOptions>,
    extensions: Extensions,
}

impl ChatCompletionRequestBuilder {
    /// 设置模型（必填）。
    #[must_use]
    pub fn model(mut self, model: impl Into<ModelId>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// 追加一条消息。
    #[must_use]
    pub fn message(mut self, message: ChatMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// 一次性设置完整消息列表（适合 Agent 循环复用历史）。
    #[must_use]
    pub fn messages(mut self, messages: impl IntoIterator<Item = ChatMessage>) -> Self {
        self.messages.extend(messages);
        self
    }

    /// 追加 system 消息。
    #[must_use]
    pub fn system(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::system(content))
    }

    /// 追加 developer 消息（o-series / 新模型的系统级指令）。
    #[must_use]
    pub fn developer(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::developer(content))
    }

    /// 追加 user 消息。
    #[must_use]
    pub fn user(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::user(content))
    }

    /// 追加 assistant 消息。
    #[must_use]
    pub fn assistant(self, content: impl Into<String>) -> Self {
        self.message(ChatMessage::assistant(content))
    }

    /// 设置 temperature。
    #[must_use]
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// 设置 top_p。
    #[must_use]
    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    /// 设置频率惩罚。
    #[must_use]
    pub fn frequency_penalty(mut self, frequency_penalty: f32) -> Self {
        self.frequency_penalty = Some(frequency_penalty);
        self
    }

    /// 设置存在惩罚。
    #[must_use]
    pub fn presence_penalty(mut self, presence_penalty: f32) -> Self {
        self.presence_penalty = Some(presence_penalty);
        self
    }

    /// 设置最大输出 token。
    #[must_use]
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// 设置 `max_completion_tokens`（推荐；包含推理 token）。
    #[must_use]
    pub fn max_completion_tokens(mut self, max_completion_tokens: u32) -> Self {
        self.max_completion_tokens = Some(max_completion_tokens);
        self
    }

    /// 设置推理投入等级。
    #[must_use]
    pub fn reasoning_effort(mut self, reasoning_effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(reasoning_effort);
        self
    }

    /// 设置输出详尽程度。
    #[must_use]
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.verbosity = Some(verbosity);
        self
    }

    /// 设置停止序列。
    #[must_use]
    pub fn stop(mut self, stop: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.stop = Some(stop.into_iter().map(Into::into).collect());
        self
    }

    /// 是否返回对数概率。
    #[must_use]
    pub fn logprobs(mut self, logprobs: bool) -> Self {
        self.logprobs = Some(logprobs);
        self
    }

    /// 设置每个 token 位置返回的候选数。
    #[must_use]
    pub fn top_logprobs(mut self, top_logprobs: u32) -> Self {
        self.top_logprobs = Some(top_logprobs);
        self
    }

    /// 设置候选数。
    #[must_use]
    pub fn n(mut self, n: u32) -> Self {
        self.n = Some(n);
        self
    }

    /// 设置随机种子（可复现实验）。
    #[must_use]
    pub fn seed(mut self, seed: i64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// 设置终端用户标识（官方已标记为被 `safety_identifier` 取代）。
    #[must_use]
    pub fn user_id(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// 设置安全标识。
    #[must_use]
    pub fn safety_identifier(mut self, identifier: impl Into<String>) -> Self {
        self.safety_identifier = Some(identifier.into());
        self
    }

    /// 设置 Prompt 缓存键。
    #[must_use]
    pub fn prompt_cache_key(mut self, key: impl Into<String>) -> Self {
        self.prompt_cache_key = Some(key.into());
        self
    }

    /// 设置 Prompt 缓存选项。
    #[must_use]
    pub fn prompt_cache_options(mut self, options: PromptCacheOptions) -> Self {
        self.prompt_cache_options = Some(options);
        self
    }

    /// 设置 Prompt 缓存保留策略（已弃用）。
    #[must_use]
    pub fn prompt_cache_retention(mut self, retention: impl Into<String>) -> Self {
        self.prompt_cache_retention = Some(retention.into());
        self
    }

    /// 设置服务等级。
    #[must_use]
    pub fn service_tier(mut self, service_tier: impl Into<String>) -> Self {
        self.service_tier = Some(service_tier.into());
        self
    }

    /// 是否存储本次补全（stored completions 前置条件）。
    #[must_use]
    pub fn store(mut self, store: bool) -> Self {
        self.store = Some(store);
        self
    }

    /// 设置元数据。
    #[must_use]
    pub fn metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置工具列表。
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = Some(tools.into_iter().collect());
        self
    }

    /// 设置工具选择。
    #[must_use]
    pub fn tool_choice(mut self, tool_choice: ToolChoice) -> Self {
        self.tool_choice = Some(tool_choice);
        self
    }

    /// 设置是否允许并行工具调用。
    #[must_use]
    pub fn parallel_tool_calls(mut self, allowed: bool) -> Self {
        self.parallel_tool_calls = Some(allowed);
        self
    }

    /// 设置结构化输出格式。
    #[must_use]
    pub fn response_format(mut self, format: ResponseFormat) -> Self {
        self.response_format = Some(format);
        self
    }

    /// 设置流式选项（仅 `Chat::stream` 生效）。
    #[must_use]
    pub fn stream_options(mut self, options: StreamOptions) -> Self {
        self.stream_options = Some(options);
        self
    }

    /// 添加一个协议扩展字段。
    #[must_use]
    pub fn extension(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extensions.insert(key.into(), value.into());
        self
    }

    /// 构建请求。
    pub fn build(self) -> Result<ChatCompletionRequest, AirxError> {
        let model = self.model.ok_or_else(|| {
            AirxError::Configuration(ConfigurationError::new("ChatCompletionRequest.model 必填"))
        })?;

        if self.messages.is_empty() {
            return Err(AirxError::Configuration(ConfigurationError::new(
                "ChatCompletionRequest.messages 不能为空",
            )));
        }

        Ok(ChatCompletionRequest {
            model,
            messages: self.messages,
            temperature: self.temperature,
            top_p: self.top_p,
            frequency_penalty: self.frequency_penalty,
            presence_penalty: self.presence_penalty,
            max_tokens: self.max_tokens,
            max_completion_tokens: self.max_completion_tokens,
            reasoning_effort: self.reasoning_effort,
            verbosity: self.verbosity,
            stop: self.stop,
            logprobs: self.logprobs,
            top_logprobs: self.top_logprobs,
            n: self.n,
            seed: self.seed,
            user: self.user,
            safety_identifier: self.safety_identifier,
            prompt_cache_key: self.prompt_cache_key,
            prompt_cache_options: self.prompt_cache_options,
            prompt_cache_retention: self.prompt_cache_retention,
            service_tier: self.service_tier,
            store: self.store,
            metadata: self.metadata,
            tools: self.tools,
            tool_choice: self.tool_choice,
            parallel_tool_calls: self.parallel_tool_calls,
            response_format: self.response_format,
            stream_options: self.stream_options,
            extensions: self.extensions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::chat::tool::JsonSchemaSpec;
    use serde_json::json;

    #[test]
    fn builder_requires_model_and_messages() {
        assert!(ChatCompletionRequest::builder().build().is_err());
        assert!(
            ChatCompletionRequest::builder()
                .model("gpt-4o-mini")
                .build()
                .is_err()
        );
    }

    #[test]
    fn optional_fields_are_omitted_when_none() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(
            value,
            json!({
                "model": "gpt-4o-mini",
                "messages": [{"role": "user", "content": "hi"}],
            })
        );
    }

    #[test]
    fn tools_and_stream_options_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .tools([Tool::function("lookup", json!({"type": "object"}), None)])
            .tool_choice(ToolChoice::auto())
            .stream_options(StreamOptions {
                include_usage: true,
                include_obfuscation: None,
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["tools"][0]["type"], "function");
        assert_eq!(value["tool_choice"], "auto");
        assert_eq!(value["stream_options"]["include_usage"], true);
    }

    #[test]
    fn response_format_json_schema_is_typed() {
        let request = ChatCompletionRequest::builder()
            .model("m")
            .user("hi")
            .response_format(ResponseFormat::JsonSchema {
                json_schema: JsonSchemaSpec {
                    name: "answer".to_owned(),
                    description: None,
                    schema: json!({"type": "object"}),
                    strict: Some(true),
                    extensions: Extensions::new(),
                },
            })
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["response_format"]["type"], "json_schema");
    }

    #[test]
    fn extensions_are_flattened() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .extension("logit_bias", json!({"50256": -100}))
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["logit_bias"]["50256"], -100);
    }
}

#[cfg(test)]
mod modern_params_tests {
    use super::*;
    use crate::openai::common::reasoning::{ReasoningEffort, Verbosity};

    #[test]
    fn max_completion_tokens_and_output_controls_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("o3")
            .user("hi")
            .max_completion_tokens(128)
            .reasoning_effort(ReasoningEffort::High)
            .verbosity(Verbosity::Low)
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["max_completion_tokens"], 128);
        assert_eq!(value["reasoning_effort"], "high");
        assert_eq!(value["verbosity"], "low");
        assert!(value.get("max_tokens").is_none());
    }
}

#[cfg(test)]
mod production_params_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sampling_params_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .frequency_penalty(0.25)
            .presence_penalty(-0.5)
            .logprobs(true)
            .top_logprobs(3)
            .n(2)
            .seed(42)
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["frequency_penalty"], 0.25);
        assert_eq!(value["presence_penalty"], -0.5);
        assert_eq!(value["logprobs"], true);
        assert_eq!(value["top_logprobs"], 3);
        assert_eq!(value["n"], 2);
        assert_eq!(value["seed"], 42);
    }

    #[test]
    fn observability_and_storage_params_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .store(true)
            .metadata(json!({"trace": "t-1"}))
            .service_tier("flex")
            .safety_identifier("user-hash")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["store"], true);
        assert_eq!(value["metadata"]["trace"], "t-1");
        assert_eq!(value["service_tier"], "flex");
        assert_eq!(value["safety_identifier"], "user-hash");
    }

    #[test]
    fn prompt_cache_params_serialize() {
        let request = ChatCompletionRequest::builder()
            .model("gpt-5.6")
            .user("hi")
            .prompt_cache_key("cache-1")
            .prompt_cache_options(PromptCacheOptions {
                mode: Some("explicit".to_owned()),
                ttl: Some("30m".to_owned()),
                extensions: Extensions::new(),
            })
            .prompt_cache_retention("24h")
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["prompt_cache_key"], "cache-1");
        assert_eq!(value["prompt_cache_options"]["mode"], "explicit");
        assert_eq!(value["prompt_cache_options"]["ttl"], "30m");
        assert_eq!(value["prompt_cache_retention"], "24h");
    }

    #[test]
    fn new_params_are_omitted_when_unset() {
        let request = ChatCompletionRequest::builder()
            .model("m")
            .user("hi")
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        for key in [
            "frequency_penalty",
            "presence_penalty",
            "logprobs",
            "top_logprobs",
            "n",
            "seed",
            "store",
            "metadata",
            "service_tier",
            "safety_identifier",
            "prompt_cache_key",
            "prompt_cache_options",
            "prompt_cache_retention",
        ] {
            assert!(value.get(key).is_none(), "{key} should be omitted");
        }
    }

    #[test]
    fn stream_options_include_obfuscation_serializes() {
        let request = ChatCompletionRequest::builder()
            .model("m")
            .user("hi")
            .stream_options(StreamOptions {
                include_usage: true,
                include_obfuscation: Some(false),
            })
            .build()
            .expect("request");
        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["stream_options"]["include_usage"], true);
        assert_eq!(value["stream_options"]["include_obfuscation"], false);
    }
}

#[cfg(test)]
mod json_mode_tests {
    use super::*;
    use crate::openai::chat::tool::ResponseFormat;

    #[test]
    fn json_object_mode_serializes_for_deepseek_compatible_deployments() {
        let request = ChatCompletionRequest::builder()
            .model("deepseek-flash")
            .system("你只输出 json，示例：{\"question\":\"q\",\"answer\":\"a\"}")
            .user("以 json 返回")
            .response_format(ResponseFormat::JsonObject)
            .max_tokens(512)
            .build()
            .expect("request");

        let value = serde_json::to_value(&request).expect("serialize");
        assert_eq!(value["response_format"]["type"], "json_object");
        assert_eq!(value["max_tokens"], 512);
    }
}
