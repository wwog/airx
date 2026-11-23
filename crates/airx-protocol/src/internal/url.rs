//! 端点路径与 URL 解析。
//!
//! 模式路径（如 OpenAI 的 chat / responses）可以是相对路径，也可以是完整 URL：
//! - 相对路径会补前导 `/`，与 Provider 的 `base_url` 拼接；
//! - 完整 `http(s)://` URL 原样使用，忽略 `base_url`。

use airx_core::AirxError;
use airx_core::error::ConfigurationError;
use airx_core::http::Uri;

/// 是否为完整 URL。
pub(crate) fn is_absolute_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

/// 规范化并校验模式路径。
pub(crate) fn endpoint_path(path: &str, label: &str) -> Result<String, AirxError> {
    let text = path.trim();
    if text.is_empty() {
        return Err(config_error(format!("{label} 不能为空")));
    }

    if is_absolute_url(text) {
        let uri: Uri = text
            .parse()
            .map_err(|error| config_error(format!("{label} 非法: {error}")))?;
        if uri.authority().is_none() {
            return Err(config_error(format!("{label} 必须是带 host 的绝对 URL")));
        }
        return Ok(text.to_owned());
    }

    if text.starts_with("//") {
        return Err(config_error(format!("{label} 不能使用协议相对地址")));
    }

    let normalized = if text.starts_with('/') {
        text.to_owned()
    } else {
        format!("/{text}")
    };

    normalized
        .parse::<Uri>()
        .map_err(|error| config_error(format!("{label} 非法: {error}")))?;
    Ok(normalized)
}

/// 把模式路径解析为最终 URL。
pub(crate) fn endpoint_uri(base_url: &str, path: &str, label: &str) -> Result<Uri, AirxError> {
    let url = if is_absolute_url(path) {
        path.to_owned()
    } else {
        let path = if path.starts_with('/') {
            path.to_owned()
        } else {
            format!("/{path}")
        };
        format!("{}{}", base_url.trim_end_matches('/'), path)
    };

    url.parse()
        .map_err(|error| config_error(format!("无法构造 {label} URL: {error}")))
}

fn config_error(message: impl Into<String>) -> AirxError {
    AirxError::Configuration(ConfigurationError::new(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_get_a_leading_slash() {
        assert_eq!(endpoint_path("v2/chat", "path").expect("path"), "/v2/chat");
        assert_eq!(
            endpoint_path(" /v2/chat ", "path").expect("path"),
            "/v2/chat"
        );
    }

    #[test]
    fn absolute_urls_are_kept_verbatim() {
        assert_eq!(
            endpoint_path("https://gateway.example.com/v2/chat", "path").expect("path"),
            "https://gateway.example.com/v2/chat"
        );
    }

    #[test]
    fn invalid_paths_are_rejected() {
        for path in ["", " ", "//gateway.example.com/x", "https://", "/a b"] {
            assert!(endpoint_path(path, "path").is_err(), "{path:?}");
        }
    }

    #[test]
    fn uri_joins_base_and_path_or_uses_absolute_url() {
        assert_eq!(
            endpoint_uri("https://api.example.com/v1", "/chat", "chat")
                .expect("uri")
                .to_string(),
            "https://api.example.com/v1/chat"
        );
        assert_eq!(
            endpoint_uri(
                "https://api.example.com/v1",
                "https://other.example.com/chat",
                "chat"
            )
            .expect("uri")
            .to_string(),
            "https://other.example.com/chat"
        );
    }
}
