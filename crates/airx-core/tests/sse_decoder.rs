//! 用公开 API 验证真实风格的 SSE 流：任意字节切分都应产出相同事件。

use airx_core::stream::{RawEvent, SseDecoder, StreamStatus};

fn decode(input: &str, chunk_size: usize) -> Vec<RawEvent> {
    let mut decoder = SseDecoder::new();
    let mut events = Vec::new();

    for chunk in input.as_bytes().chunks(chunk_size) {
        decoder.push(chunk);
        while let Some(event) = decoder.next_event().expect("decode event") {
            events.push(event);
        }
    }

    if let Some(event) = decoder.finish().expect("finish stream") {
        events.push(event);
    }
    events
}

#[test]
fn openai_chat_stream_is_stable_at_every_byte_boundary() {
    let input = concat!(
        r#"data: {"id":"chatcmpl-1","choices":[{"delta":{"role":"assistant"}}]}"#,
        "\n\n",
        r#"data: {"id":"chatcmpl-1","choices":[{"delta":{"content":"你好"}}]}"#,
        "\n\n",
        r#"data: {"id":"chatcmpl-1","choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "\n\n",
        "data: [DONE]\n\n",
    );

    let expected = decode(input, input.len());
    assert_eq!(expected.len(), 4);
    assert_eq!(expected[3].data, "[DONE]");

    for chunk_size in 1..=input.len() {
        let actual = decode(input, chunk_size);
        assert_eq!(actual, expected, "chunk_size={chunk_size}");
    }
}

#[test]
fn anthropic_stream_keeps_event_names_and_multi_line_data() {
    let input = concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n",
        "\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\n",
        "data: \"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n",
        "\n",
    );

    let events = decode(input, 7);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event.as_deref(), Some("message_start"));
    assert_eq!(events[1].event.as_deref(), Some("content_block_delta"));
    assert!(events[1].data.contains("\"text\":\"Hi\""));
    assert!(events[1].data.contains('\n'));
}

#[test]
fn stream_status_distinguishes_interruption_from_completion() {
    assert!(StreamStatus::Completed.is_success());
    assert!(!StreamStatus::Incomplete.is_success());
    assert!(StreamStatus::Incomplete.is_terminal());
}
