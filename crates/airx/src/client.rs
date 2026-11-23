//! Client 构建入口。
//!
//! 构建结果就是**具体协议的客户端**（`OpenAi` / `Anthropic` / `Gemini`），
//! 创建后即可复用：后续请求不再需要选择 Provider，也不再出现
//! `client.anthropic()?.messages()` 这类重复入口。

#[cfg(feature = "anthropic")]
use crate::builder::AnthropicBuilder;
use crate::builder::ClientBuilder;
#[cfg(feature = "gemini")]
use crate::builder::GeminiBuilder;
#[cfg(feature = "openai")]
use crate::builder::OpenAiBuilder;

/// 入口命名空间。
///
/// 三个快捷入口直接进入对应 Provider 的配置：
///
/// ```ignore
/// let anthropic = Client::anthropic().api_key("...").build()?;
/// let openai = Client::openai().api_key("...").build()?;
/// let gemini = Client::gemini().api_key("...").build()?;
/// ```
///
/// 需要先配置 Transport / 超时 / 重试 / Header 时使用 [`Client::builder`]：
///
/// ```ignore
/// let client = Client::builder()
///     .transport(mock)
///     .timeout(Duration::from_secs(60))
///     .anthropic()
///     .api_key("...")
///     .build()?;
/// ```
pub struct Client {
    _private: (),
}

impl Client {
    /// 通用构建器（先配置执行级选项，再进入 Provider）。
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// 直接进入 OpenAI 配置。
    #[cfg(feature = "openai")]
    pub fn openai() -> OpenAiBuilder {
        ClientBuilder::new().openai()
    }

    /// 直接进入 Anthropic 配置。
    #[cfg(feature = "anthropic")]
    pub fn anthropic() -> AnthropicBuilder {
        ClientBuilder::new().anthropic()
    }

    /// 直接进入 Gemini 配置。
    #[cfg(feature = "gemini")]
    pub fn gemini() -> GeminiBuilder {
        ClientBuilder::new().gemini()
    }
}
