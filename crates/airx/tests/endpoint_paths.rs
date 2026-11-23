//! 模式路径：官方默认值、构建时覆盖与运行时覆盖。

#![cfg(any(feature = "openai", feature = "anthropic"))]

use airx_transport::MockTransport;

#[cfg(feature = "openai")]
mod openai {
    use airx::Endpoint;
    use airx::prelude::*;

    use super::*;

    fn client() -> OpenAi {
        Client::builder()
            .transport(MockTransport::new())
            .openai()
            .api_key("sk-test")
            .build()
            .expect("client")
    }

    fn chat_request() -> ChatCompletionRequest {
        ChatCompletionRequest::builder()
            .model("gpt-4o-mini")
            .user("hi")
            .build()
            .expect("request")
    }

    fn responses_request() -> ResponsesRequest {
        ResponsesRequest::builder()
            .model("gpt-4o-mini")
            .input("hi")
            .build()
            .expect("request")
    }

    #[test]
    fn defaults_are_the_official_paths() {
        let client = client();

        let chat = client
            .chat()
            .endpoint()
            .encode(&chat_request())
            .expect("chat encode");
        assert_eq!(chat.uri.path(), "/v1/chat/completions");

        let responses = client
            .responses()
            .endpoint()
            .encode(&responses_request())
            .expect("responses encode");
        assert_eq!(responses.uri.path(), "/v1/responses");
    }

    #[test]
    fn build_time_paths_are_reused_by_chat_and_responses() {
        let client = Client::builder()
            .transport(MockTransport::new())
            .openai()
            .api_key("sk-test")
            .chat_path("v2/chat")
            .responses_path("https://gateway.example.com/respond")
            .build()
            .expect("client");

        let chat = client
            .chat()
            .endpoint()
            .encode(&chat_request())
            .expect("chat encode");
        assert_eq!(chat.uri.path(), "/v1/v2/chat");

        let responses = client
            .responses()
            .endpoint()
            .encode(&responses_request())
            .expect("responses encode");
        assert_eq!(
            responses.uri.to_string(),
            "https://gateway.example.com/respond"
        );
    }

    #[test]
    fn runtime_paths_override_without_mutating_the_client() {
        let client = client();

        let chat = client.chat_at("/runtime/chat").expect("chat_at");
        let http = chat
            .endpoint()
            .encode(&chat_request())
            .expect("chat encode");
        assert_eq!(http.uri.path(), "/v1/runtime/chat");

        // 客户端本身仍使用构建时配置。
        let default = client
            .chat()
            .endpoint()
            .encode(&chat_request())
            .expect("default chat encode");
        assert_eq!(default.uri.path(), "/v1/chat/completions");

        let responses = client
            .responses_at("https://gateway.example.com/runtime")
            .expect("responses_at");
        let http = responses
            .endpoint()
            .encode(&responses_request())
            .expect("responses encode");
        assert_eq!(http.uri.to_string(), "https://gateway.example.com/runtime");
    }

    #[test]
    fn invalid_runtime_path_is_rejected() {
        let client = client();
        assert!(client.chat_at("").is_err());
        assert!(client.responses_at("//host/path").is_err());
    }
}

#[cfg(feature = "anthropic")]
mod anthropic {
    use airx::Endpoint;
    use airx::prelude::*;

    use super::*;

    fn client() -> Anthropic {
        Client::builder()
            .transport(MockTransport::new())
            .anthropic()
            .api_key("sk-ant-test")
            .build()
            .expect("client")
    }

    fn request() -> MessagesRequest {
        MessagesRequest::builder()
            .model("claude-sonnet-4-5")
            .max_tokens(64)
            .user("hi")
            .build()
            .expect("request")
    }

    #[test]
    fn default_messages_path_is_official() {
        let http = client()
            .messages()
            .endpoint()
            .encode(&request())
            .expect("encode");
        assert_eq!(http.uri.path(), "/v1/messages");
    }

    #[test]
    fn build_time_and_runtime_paths_are_overridable() {
        let client = Client::builder()
            .transport(MockTransport::new())
            .anthropic()
            .api_key("sk-ant-test")
            .messages_path("v2/messages")
            .build()
            .expect("client");

        let http = client
            .messages()
            .endpoint()
            .encode(&request())
            .expect("encode");
        assert_eq!(http.uri.path(), "/v2/messages");

        let runtime = client
            .messages_at("https://gateway.example.com/anthropic/messages")
            .expect("messages_at");
        let http = runtime.endpoint().encode(&request()).expect("encode");
        assert_eq!(
            http.uri.to_string(),
            "https://gateway.example.com/anthropic/messages"
        );
    }
}
