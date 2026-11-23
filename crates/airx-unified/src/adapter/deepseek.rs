//! 统一请求 → DeepSeek；DeepSeek → 统一响应。
//!
//! DeepSeek 的结构与 OpenAI Chat 兼容，转换复用 OpenAI adapter，
//! 但产物仍交给 DeepSeek 端点与兼容策略处理。

use airx_core::AirxError;
use airx_core::model::ProviderId;
use airx_protocol::openai::chat::{ChatCompletionRequest, ChatCompletionResponse};

use crate::compatibility::{Adapted, CompatibilityMode};
use crate::request::GenerateRequest;
use crate::response::GenerateResponse;

/// 统一请求 → DeepSeek Chat 请求。
pub fn to_deepseek(
    request: &GenerateRequest,
    mode: CompatibilityMode,
) -> Result<Adapted<ChatCompletionRequest>, AirxError> {
    super::openai::to_openai(request, mode)
}

/// DeepSeek Chat 响应 → 统一响应。
pub fn from_deepseek(response: ChatCompletionResponse) -> GenerateResponse {
    let mut unified = super::openai::from_openai(response);
    unified.provider = ProviderId::deepseek();
    unified
}
