//! 流生命周期状态。
//!
//! 与具体事件类型解耦：Provider 事件各自建模，但“流是否正常结束”由统一状态表达。

/// 流状态。
///
/// `Incomplete` 与 `Failed` 必须区分：
/// 前者是“流提前结束、内容可能缺失”，后者是“明确失败”。
/// 网络断开属于 `Incomplete`，不能当作 `Completed`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamStatus {
    /// 正在接收。
    #[default]
    Active,
    /// 正常完成（例如收到协议定义的结束事件）。
    Completed,
    /// 明确失败（协议错误事件或不可恢复错误）。
    Failed,
    /// 流提前结束，内容可能不完整。
    Incomplete,
    /// 调用方取消。
    Cancelled,
}

impl StreamStatus {
    /// 是否已结束（无论成功与否）。
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Active)
    }

    /// 是否正常完成。
    pub fn is_success(self) -> bool {
        matches!(self, Self::Completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_is_not_terminal() {
        assert!(!StreamStatus::Active.is_terminal());
        assert!(!StreamStatus::Active.is_success());
    }

    #[test]
    fn only_completed_is_success() {
        assert!(StreamStatus::Completed.is_success());
        for status in [
            StreamStatus::Failed,
            StreamStatus::Incomplete,
            StreamStatus::Cancelled,
        ] {
            assert!(status.is_terminal());
            assert!(!status.is_success());
        }
    }
}
