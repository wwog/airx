//! Chat 流式事件。

use super::chunk::ChatCompletionChunk;

/// Chat 流事件。
// `Done` 是零大小标记，与 Chunk 天然不等大；装箱会让事件消费多一次分配。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum ChatStreamEvent {
    /// 一个数据 chunk。
    Chunk(ChatCompletionChunk),
    /// 协议结束标记（`data: [DONE]`）。
    Done,
}
