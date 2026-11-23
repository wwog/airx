//! 协议与 Provider 无关的标识类型。
//!
//! 三个概念必须分开：
//!
//! - [`ApiVersion`]：Provider 接口版本，影响请求路径与 Header。
//! - [`ModelId`]：模型标识，只用于选择模型。
//! - [`ProviderId`]：Provider 标识，只用于标签与错误归属，不决定协议。
//!
//! `ProviderId` / `ModelId` 用字符串新类型而非枚举，
//! 新增 Provider 不需要修改 `airx-core`。

use std::fmt;

use serde::{Deserialize, Serialize};

/// Provider 标识。
///
/// 预置 OpenAI / Anthropic / Gemini / DeepSeek 的构造器，
/// 同时允许第三方扩展使用任意自定义标识。
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(Box<str>);

impl ProviderId {
    /// 创建 Provider 标识。
    pub fn new(id: impl Into<Box<str>>) -> Self {
        Self(id.into())
    }

    /// 以 `&str` 返回标识。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// OpenAI。
    pub fn openai() -> Self {
        Self::new("openai")
    }

    /// Anthropic。
    pub fn anthropic() -> Self {
        Self::new("anthropic")
    }

    /// Gemini。
    pub fn gemini() -> Self {
        Self::new("gemini")
    }

    /// DeepSeek。
    pub fn deepseek() -> Self {
        Self::new("deepseek")
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ProviderId").field(&self.as_str()).finish()
    }
}

impl From<&str> for ProviderId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ProviderId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for ProviderId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// 模型标识。
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModelId(Box<str>);

impl ModelId {
    /// 创建模型标识。
    pub fn new(id: impl Into<Box<str>>) -> Self {
        Self(id.into())
    }

    /// 以 `&str` 返回标识。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ModelId").field(&self.as_str()).finish()
    }
}

impl From<&str> for ModelId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ModelId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for ModelId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Provider 的 API 版本（如 Anthropic 的 `2023-06-01`）。
///
/// 与 [`ModelId`] 语义分离：版本影响请求路径与 Header，模型只用于选择。
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiVersion(Box<str>);

impl ApiVersion {
    /// 创建 API 版本。
    pub fn new(version: impl Into<Box<str>>) -> Self {
        Self(version.into())
    }

    /// 以 `&str` 返回版本。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ApiVersion").field(&self.as_str()).finish()
    }
}

impl From<&str> for ApiVersion {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ApiVersion {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for ApiVersion {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_roundtrips_and_displays() {
        let id = ProviderId::new("acme");
        assert_eq!(id.as_str(), "acme");
        assert_eq!(id.to_string(), "acme");
        assert_eq!(format!("{id:?}"), r#"ProviderId("acme")"#);

        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, r#""acme""#);
        let back: ProviderId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
    }

    #[test]
    fn builtin_provider_ids_are_stable() {
        assert_eq!(ProviderId::openai().as_str(), "openai");
        assert_eq!(ProviderId::anthropic().as_str(), "anthropic");
        assert_eq!(ProviderId::gemini().as_str(), "gemini");
        assert_eq!(ProviderId::deepseek().as_str(), "deepseek");
    }

    #[test]
    fn model_and_api_version_are_distinct_types() {
        let model = ModelId::new("gpt-5");
        let version = ApiVersion::new("2023-06-01");
        assert_eq!(model.to_string(), "gpt-5");
        assert_eq!(version.to_string(), "2023-06-01");
    }
}
