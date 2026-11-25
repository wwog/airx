//! 分级、按 scope 过滤的日志抽象。
//!
//! 设计约束：
//!
//! - **默认关闭**：没有 logger 时所有日志点直接短路，不格式化、不分配。
//! - **无全局状态**：日志配置随客户端携带，多个客户端可以使用不同的 logger 与级别。
//! - **业务层注入**：实现 [`Logger`] 即可接管输出；airx 不提供文件、轮转、异步队列等后端。
//! - **先过滤再构造**：[`LogHandle::enabled`] 为假时不会构造 [`Record`] 与 [`Field`]。
//!
//! 级别语义与常见日志库一致：[`LogLevel::Info`] 表示输出 `Info` / `Warn` / `Error`，
//! [`LogLevel::Off`] 关闭该 scope。
//!
//! ```rust
//! use airx_core::log::{LogConfig, LogLevel, LogHandle, Logger, Record, Scope};
//!
//! struct Stderr;
//!
//! impl Logger for Stderr {
//!     fn log(&self, record: &Record<'_>) {
//!         eprintln!("[{:?}] {}", record.level, record);
//!     }
//! }
//!
//! let config = LogConfig::new()
//!     .scope(Scope::NET, LogLevel::Info)
//!     .scope(Scope::CALL, LogLevel::Info);
//! let log = LogHandle::new(std::sync::Arc::new(Stderr), config);
//!
//! assert!(log.enabled(Scope::NET, LogLevel::Warn));
//! assert!(!log.enabled(Scope::RETRY, LogLevel::Warn));
//! ```
//!
//! 日志点的写法见 [`airx_log!`](crate::airx_log)。

use std::fmt;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use crate::http::HeaderMap;

/// 日志级别。
///
/// 顺序即过滤强度：配置某个级别表示输出该级别及更严重的级别
/// （`Info` 覆盖 `Info` / `Warn` / `Error`）。数值越小越严重。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum LogLevel {
    /// 关闭该 scope。
    #[default]
    Off = 0,
    /// 失败与不可恢复错误。
    Error = 1,
    /// 可恢复但需要注意的情况（重试、非 2xx）。
    Warn = 2,
    /// 一次请求 / 流的关键节点。
    Info = 3,
    /// 排查问题用的细节。
    Debug = 4,
    /// 最细粒度的事件（逐事件、逐字段）。
    Trace = 5,
}

impl LogLevel {
    /// 稳定小写名称，便于自定义 logger 序列化。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 日志作用域。
///
/// 使用 `&'static str` 新类型而不是封闭枚举：内置 scope 以常量提供，
/// 业务自定义 scope 不需要修改本 crate，比较与拷贝都不分配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scope(&'static str);

impl Scope {
    /// HTTP 收发：请求发出、响应到达、传输失败。
    pub const NET: Self = Self::new("net");
    /// 一次逻辑 API 调用：完成状态、耗时、request id。
    pub const CALL: Self = Self::new("call");
    /// 流式生命周期：建立、结束、截断、失败。
    pub const STREAM: Self = Self::new("stream");
    /// 重试决策与耗尽。
    pub const RETRY: Self = Self::new("retry");
    /// 编解码阶段失败。
    pub const PROTOCOL: Self = Self::new("protocol");

    /// 自定义 scope。
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// scope 名称。
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// 字段值。
///
/// 只接受借用值：日志点不需要为字段分配内存，格式化发生在 logger 内部。
#[derive(Clone, Copy)]
pub enum FieldValue<'a> {
    /// 任何 [`fmt::Display`] 值（字符串、数字、状态码、错误等）。
    Display(&'a dyn fmt::Display),
    /// 错误值；保证保留 source 链的 `Display` 形态。
    Error(&'a (dyn std::error::Error + 'static)),
    /// 任何 [`fmt::Debug`] 值（类型化事件等没有 `Display` 的结构）。
    Debug(&'a dyn fmt::Debug),
}

impl fmt::Display for FieldValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Display(value) => fmt::Display::fmt(value, f),
            Self::Error(error) => fmt::Display::fmt(error, f),
            Self::Debug(value) => fmt::Debug::fmt(value, f),
        }
    }
}

/// 结构化日志字段。
#[derive(Clone, Copy)]
pub struct Field<'a> {
    /// 字段名（编译期常量，便于 logger 建立索引）。
    pub key: &'static str,
    /// 字段值。
    pub value: FieldValue<'a>,
}

impl<'a> Field<'a> {
    /// 以任意可显示值构造字段。
    pub fn new<T: fmt::Display>(key: &'static str, value: &'a T) -> Self {
        Self {
            key,
            value: FieldValue::Display(value),
        }
    }

    /// 以错误构造字段。
    pub fn error(key: &'static str, error: &'a (dyn std::error::Error + 'static)) -> Self {
        Self {
            key,
            value: FieldValue::Error(error),
        }
    }

    /// 以 [`fmt::Debug`] 值构造字段。
    pub fn debug<T: fmt::Debug>(key: &'static str, value: &'a T) -> Self {
        Self {
            key,
            value: FieldValue::Debug(value),
        }
    }
}

impl fmt::Display for Field<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.key, self.value)
    }
}

impl fmt::Debug for Field<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// 字段列表的 `Display` 包装，用于整体打印 `key=value key=value`。
#[derive(Clone, Copy)]
pub struct Fields<'a>(pub &'a [Field<'a>]);

impl fmt::Display for Fields<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, field) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            fmt::Display::fmt(field, f)?;
        }
        Ok(())
    }
}

/// 交给 [`Logger`] 的一条记录。
pub struct Record<'a> {
    /// 级别，永远不是 [`LogLevel::Off`]。
    pub level: LogLevel,
    /// 产生日志的 scope。
    pub scope: Scope,
    /// 惰性格式化的消息。
    pub message: fmt::Arguments<'a>,
    /// 结构化字段。
    pub fields: &'a [Field<'a>],
}

impl fmt::Display for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.scope, self.message)?;
        if !self.fields.is_empty() {
            write!(f, " {}", Fields(self.fields))?;
        }
        Ok(())
    }
}

/// 日志输出实现。
///
/// 实现必须同步、快速、不可 panic；需要异步落盘时自行投递到 channel 后立即返回。
/// airx 不会捕获实现中的 panic，也不允许日志失败影响请求结果（[`Logger::log`] 无返回值）。
pub trait Logger: Send + Sync {
    /// 输出一条记录。仅在 [`LogHandle::enabled`] 为真时被调用。
    fn log(&self, record: &Record<'_>);
}

/// 自定义 Header 脱敏 / 渲染回调：拿到全量 HeaderMap，返回写入日志的文本。
pub type HeaderRedactor = Arc<dyn Fn(&HeaderMap) -> String + Send + Sync>;

/// scope → 级别 的过滤配置。
///
/// 查找规则：精确匹配 scope 优先，未匹配时回退到 [`LogConfig::default_level`]。
/// 不做前缀继承（`net` 不会自动覆盖 `net.http`），保证规则可预测。
#[derive(Clone)]
pub struct LogConfig {
    default_level: LogLevel,
    scopes: Vec<(Scope, LogLevel)>,
    body_limit: usize,
    header_redactor: Option<HeaderRedactor>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            default_level: LogLevel::Off,
            scopes: Vec::new(),
            body_limit: DEFAULT_BODY_LIMIT,
            header_redactor: None,
        }
    }
}

impl fmt::Debug for LogConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogConfig")
            .field("default_level", &self.default_level)
            .field("scopes", &self.scopes)
            .field("body_limit", &self.body_limit)
            .field("header_redactor", &self.header_redactor.is_some())
            .finish()
    }
}

impl LogConfig {
    /// 空配置：所有 scope 关闭。
    pub fn new() -> Self {
        Self::default()
    }

    /// 所有未显式配置的 scope 使用同一级别。
    pub fn all(level: LogLevel) -> Self {
        Self {
            default_level: level,
            ..Self::default()
        }
    }

    /// 设置默认级别（未精确匹配 scope 时生效）。
    #[must_use]
    pub fn default_level(mut self, level: LogLevel) -> Self {
        self.default_level = level;
        self
    }

    /// 为某个 scope 设置级别；同一 scope 后设置者生效。
    #[must_use]
    pub fn scope(mut self, scope: Scope, level: LogLevel) -> Self {
        self.scopes.push((scope, level));
        self
    }

    /// 批量设置 `(scope, level)`。
    pub fn scopes<I: IntoIterator<Item = (Scope, LogLevel)>>(mut self, scopes: I) -> Self {
        self.scopes.extend(scopes);
        self
    }

    /// 该 scope 生效的级别。
    pub fn level_for(&self, scope: Scope) -> LogLevel {
        self.scopes
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate == scope)
            .map(|(_, level)| *level)
            .unwrap_or(self.default_level)
    }

    /// Debug 级别下 body 预览的上限（字节）；`Trace` 不受该上限限制。
    #[must_use]
    pub fn body_limit(mut self, bytes: usize) -> Self {
        self.body_limit = bytes;
        self
    }

    /// 当前 body 预览上限。
    pub fn body_limit_bytes(&self) -> usize {
        self.body_limit
    }

    /// 自定义 Header 脱敏 / 渲染。
    ///
    /// 回调拿到**全量** [`HeaderMap`]（包含认证信息），返回值原样写进日志。
    /// 设置后完全替换内置脱敏，不会在自动脱敏结果上追加。
    #[must_use]
    pub fn header_redactor<F>(mut self, redactor: F) -> Self
    where
        F: Fn(&HeaderMap) -> String + Send + Sync + 'static,
    {
        self.header_redactor = Some(Arc::new(redactor));
        self
    }

    /// 渲染 Header：有手动脱敏时用回调，否则用内置 [`redact_headers`]。
    pub fn render_headers(&self, headers: &HeaderMap) -> String {
        match &self.header_redactor {
            Some(redactor) => redactor(headers),
            None => redact_headers(headers),
        }
    }
}

/// Debug 级别下 body 预览的默认上限（字节）。
pub const DEFAULT_BODY_LIMIT: usize = 4 * 1024;

/// 内置脱敏的敏感 Header（大小写不敏感）。
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "x-goog-api-key",
    "cookie",
    "set-cookie",
];

/// 把 Header 渲染成 `name: value; ...`，敏感值替换为 `<redacted>`。
pub fn redact_headers(headers: &HeaderMap) -> String {
    let mut rendered = String::new();
    for (name, value) in headers {
        if !rendered.is_empty() {
            rendered.push_str("; ");
        }
        rendered.push_str(name.as_str());
        rendered.push_str(": ");
        if SENSITIVE_HEADERS
            .iter()
            .any(|sensitive| sensitive.eq_ignore_ascii_case(name.as_str()))
        {
            rendered.push_str("<redacted>");
        } else {
            rendered.push_str(value.to_str().unwrap_or("<non-utf8>"));
        }
    }
    rendered
}

/// content-type 是否属于可以直接按文本展示的类型。
pub fn is_textual_content_type(content_type: &str) -> bool {
    let mime = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    mime.starts_with("text/")
        || mime.ends_with("json")
        || mime.ends_with("+json")
        || mime.ends_with("xml")
        || mime.ends_with("+xml")
        || matches!(
            mime.as_str(),
            "application/x-www-form-urlencoded" | "application/javascript" | "application/graphql"
        )
}

/// 生成 body 预览。
///
/// - 空 body → `<empty>`；
/// - 文本 → lossy UTF-8，`limit` 为 `Some` 时截断并标注总字节数；
/// - 二进制 → `<binary N bytes>`；
/// - 缺少 content-type 时用 UTF-8 有效性判断文本。
pub fn body_preview(content_type: Option<&str>, body: &[u8], limit: Option<usize>) -> String {
    if body.is_empty() {
        return "<empty>".to_owned();
    }

    let textual = match content_type {
        Some(content_type) => is_textual_content_type(content_type),
        None => std::str::from_utf8(body).is_ok(),
    };
    if !textual {
        return format!("<binary {} bytes>", body.len());
    }

    let text = String::from_utf8_lossy(body);
    let total = text.len();
    match limit {
        Some(limit) if total > limit => {
            let mut cut = limit.min(total);
            while cut > 0 && !text.is_char_boundary(cut) {
                cut -= 1;
            }
            format!("{}… ({} bytes total)", &text[..cut], total)
        }
        _ => text.into_owned(),
    }
}

/// 日志分发句柄：先按 scope + level 过滤，再调用 [`Logger`]。
///
/// 只持有 `Arc`，克隆成本与 `Arc` 相同，随请求传递不会复制配置。
#[derive(Clone, Default)]
pub struct LogHandle {
    logger: Option<Arc<dyn Logger>>,
    config: Arc<LogConfig>,
}

impl LogHandle {
    /// 关闭的句柄：所有日志点短路。
    pub fn disabled() -> Self {
        Self::default()
    }

    /// 以给定 logger 与配置创建。
    pub fn new(logger: Arc<dyn Logger>, config: LogConfig) -> Self {
        Self {
            logger: Some(logger),
            config: Arc::new(config),
        }
    }

    /// 是否启用了 logger。
    pub fn has_logger(&self) -> bool {
        self.logger.is_some()
    }

    /// 当前过滤配置。
    pub fn config(&self) -> &LogConfig {
        &self.config
    }

    /// 该 scope 在当前级别下是否输出。
    pub fn enabled(&self, scope: Scope, level: LogLevel) -> bool {
        self.logger.is_some() && level != LogLevel::Off && level <= self.config.level_for(scope)
    }

    /// 分发一条记录。
    ///
    /// 调用方应先检查 [`LogHandle::enabled`]（[`airx_log!`](crate::airx_log) 会代劳），
    /// 这里再检查一次以防手工调用绕过过滤。
    pub fn log(
        &self,
        scope: Scope,
        level: LogLevel,
        message: fmt::Arguments<'_>,
        fields: &[Field<'_>],
    ) {
        if !self.enabled(scope, level) {
            return;
        }
        let Some(logger) = &self.logger else {
            return;
        };
        logger.log(&Record {
            level,
            scope,
            message,
            fields,
        });
    }

    /// [`LogLevel::Error`] 日志。
    pub fn error(&self, scope: Scope, message: fmt::Arguments<'_>, fields: &[Field<'_>]) {
        self.log(scope, LogLevel::Error, message, fields);
    }

    /// [`LogLevel::Warn`] 日志。
    pub fn warn(&self, scope: Scope, message: fmt::Arguments<'_>, fields: &[Field<'_>]) {
        self.log(scope, LogLevel::Warn, message, fields);
    }

    /// [`LogLevel::Info`] 日志。
    pub fn info(&self, scope: Scope, message: fmt::Arguments<'_>, fields: &[Field<'_>]) {
        self.log(scope, LogLevel::Info, message, fields);
    }

    /// [`LogLevel::Debug`] 日志。
    pub fn debug(&self, scope: Scope, message: fmt::Arguments<'_>, fields: &[Field<'_>]) {
        self.log(scope, LogLevel::Debug, message, fields);
    }

    /// [`LogLevel::Trace`] 日志。
    pub fn trace(&self, scope: Scope, message: fmt::Arguments<'_>, fields: &[Field<'_>]) {
        self.log(scope, LogLevel::Trace, message, fields);
    }
}

impl fmt::Debug for LogHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LogHandle")
            .field("has_logger", &self.logger.is_some())
            .field("config", &self.config)
            .finish()
    }
}

/// 已耗时。
///
/// WASM 目标不保证单调时钟可用，此时 [`Elapsed`] 输出 `-`，避免平台差异。
#[derive(Debug, Clone, Copy)]
pub struct Elapsed(
    #[cfg(not(target_arch = "wasm32"))] Duration,
    #[cfg(target_arch = "wasm32")] (),
);

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(not(target_arch = "wasm32"))]
        {
            write!(f, "{}ms", self.0.as_millis())
        }
        #[cfg(target_arch = "wasm32")]
        {
            f.write_str("-")
        }
    }
}

/// 跨平台的耗时计时器；WASM 下退化为空计时，只用于日志不影响逻辑。
#[derive(Debug, Clone, Copy)]
pub struct Timer(
    #[cfg(not(target_arch = "wasm32"))] std::time::Instant,
    #[cfg(target_arch = "wasm32")] (),
);

impl Timer {
    /// 开始计时。
    pub fn start() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self(std::time::Instant::now())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self(())
        }
    }

    /// 距开始的时间。
    pub fn elapsed(&self) -> Elapsed {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Elapsed(self.0.elapsed())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Elapsed(())
        }
    }
}

#[cfg(feature = "tracing")]
mod tracing_logger {
    use std::fmt;

    use super::{LogLevel, Logger, Record};

    /// 把 airx 日志转发到 `tracing`。
    ///
    /// 内置 scope 映射为固定 target（`airx::net`、`airx::call`、`airx::stream`、
    /// `airx::retry`、`airx::protocol`），其它 scope 归到 `airx`，因此可以直接用
    /// `RUST_LOG=airx::net=info,airx::call=info` 控制。级别过滤仍在 airx 侧完成
    /// （[`crate::log::LogConfig`]），tracing 只是输出通道。
    #[derive(Debug, Clone, Copy, Default)]
    pub struct TracingLogger;

    impl TracingLogger {
        /// 创建适配器。
        pub fn new() -> Self {
            Self
        }
    }

    /// `tracing` 要求 target 是编译期常量，因此按 scope 分派到固定字面量。
    macro_rules! emit {
        ($target:literal, $record:expr, $fields:expr, $message:expr) => {
            match $record.level {
                LogLevel::Error => tracing::error!(
                    target: $target,
                    fields = %$fields,
                    "{}",
                    $message
                ),
                LogLevel::Warn => tracing::warn!(
                    target: $target,
                    fields = %$fields,
                    "{}",
                    $message
                ),
                LogLevel::Info => tracing::info!(
                    target: $target,
                    fields = %$fields,
                    "{}",
                    $message
                ),
                LogLevel::Debug => tracing::debug!(
                    target: $target,
                    fields = %$fields,
                    "{}",
                    $message
                ),
                LogLevel::Trace => tracing::trace!(
                    target: $target,
                    fields = %$fields,
                    "{}",
                    $message
                ),
                LogLevel::Off => {}
            }
        };
    }

    impl Logger for TracingLogger {
        fn log(&self, record: &Record<'_>) {
            struct Fields<'a>(&'a [super::Field<'a>]);

            impl fmt::Display for Fields<'_> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    fmt::Display::fmt(&super::Fields(self.0), f)
                }
            }

            let message = format_args!("{}", record.message);
            let fields = Fields(record.fields);

            match record.scope.as_str() {
                "net" => emit!("airx::net", record, fields, message),
                "call" => emit!("airx::call", record, fields, message),
                "stream" => emit!("airx::stream", record, fields, message),
                "retry" => emit!("airx::retry", record, fields, message),
                "protocol" => emit!("airx::protocol", record, fields, message),
                _ => emit!("airx", record, fields, message),
            }
        }
    }
}

#[cfg(feature = "tracing")]
pub use tracing_logger::TracingLogger;

/// 记录一条结构化日志。
///
/// 先判断 [`LogHandle::enabled`]，关闭时不构造字段、不格式化消息。
///
/// ```rust
/// use airx_core::airx_log;
/// use airx_core::log::{LogConfig, LogHandle, LogLevel, Logger, Record, Scope};
/// use std::sync::Arc;
///
/// struct Collector;
///
/// impl Logger for Collector {
///     fn log(&self, record: &Record<'_>) {
///         assert_eq!(record.to_string(), "[net] request sent method=GET status=200");
///     }
/// }
///
/// let log = LogHandle::new(Arc::new(Collector), LogConfig::all(LogLevel::Info));
/// airx_log!(log, Scope::NET, LogLevel::Info, "request sent",
///     method = "GET",
///     status = 200u16,
/// );
/// ```
#[macro_export]
macro_rules! airx_log {
    ($log:expr, $scope:expr, $level:expr, $message:literal $(,)?) => {{
        let log = &$log;
        if log.enabled($scope, $level) {
            log.log($scope, $level, format_args!($message), &[]);
        }
    }};
    ($log:expr, $scope:expr, $level:expr, $message:literal, $($key:ident = $value:expr),+ $(,)?) => {{
        let log = &$log;
        if log.enabled($scope, $level) {
            log.log(
                $scope,
                $level,
                format_args!($message),
                &[$($crate::log::Field::new(stringify!($key), &$value)),+],
            );
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Collector {
        records: Mutex<Vec<String>>,
    }

    impl Collector {
        fn lines(&self) -> Vec<String> {
            self.records.lock().expect("lock").clone()
        }
    }

    impl Logger for Collector {
        fn log(&self, record: &Record<'_>) {
            self.records.lock().expect("lock").push(record.to_string());
        }
    }

    fn handle(config: LogConfig) -> (LogHandle, Arc<Collector>) {
        let collector = Arc::new(Collector::default());
        (LogHandle::new(collector.clone(), config), collector)
    }

    #[test]
    fn default_config_is_silent() {
        let (log, collector) = handle(LogConfig::new());
        airx_log!(log, Scope::NET, LogLevel::Error, "boom");
        assert!(collector.lines().is_empty());
        assert!(!log.enabled(Scope::NET, LogLevel::Error));
    }

    #[test]
    fn disabled_handle_is_silent() {
        let log = LogHandle::disabled();
        airx_log!(log, Scope::NET, LogLevel::Error, "boom");
        assert!(!log.has_logger());
        assert!(!log.enabled(Scope::NET, LogLevel::Info));
    }

    #[test]
    fn scope_level_covers_more_severe_levels() {
        let config = LogConfig::new()
            .scope(Scope::NET, LogLevel::Info)
            .scope(Scope::CALL, LogLevel::Info);

        assert!(config.level_for(Scope::NET) == LogLevel::Info);
        assert!(matches!(config.level_for(Scope::NET), LogLevel::Info));
        assert_eq!(config.level_for(Scope::RETRY), LogLevel::Off);

        let (log, collector) = handle(config);
        airx_log!(log, Scope::NET, LogLevel::Error, "a");
        airx_log!(log, Scope::NET, LogLevel::Warn, "b");
        airx_log!(log, Scope::NET, LogLevel::Info, "c");
        airx_log!(log, Scope::NET, LogLevel::Debug, "d");
        airx_log!(log, Scope::RETRY, LogLevel::Error, "e");

        let lines = collector.lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "[net] a");
        assert_eq!(lines[1], "[net] b");
        assert_eq!(lines[2], "[net] c");
    }

    #[test]
    fn default_level_applies_to_unlisted_scopes() {
        let config = LogConfig::all(LogLevel::Warn).scope(Scope::STREAM, LogLevel::Trace);
        assert_eq!(config.level_for(Scope::NET), LogLevel::Warn);
        assert_eq!(config.level_for(Scope::STREAM), LogLevel::Trace);
    }

    #[test]
    fn fields_render_in_order() {
        let (log, collector) = handle(LogConfig::all(LogLevel::Trace));
        airx_log!(
            log,
            Scope::NET,
            LogLevel::Info,
            "response received",
            method = "POST",
            status = 201u16,
            retry = false,
        );

        let lines = collector.lines();
        assert_eq!(
            lines,
            vec!["[net] response received method=POST status=201 retry=false"]
        );
    }

    #[test]
    fn off_level_never_logs() {
        let (log, collector) = handle(LogConfig::all(LogLevel::Trace));
        assert!(!log.enabled(Scope::NET, LogLevel::Off));
        airx_log!(log, Scope::NET, LogLevel::Off, "never");
        assert!(collector.lines().is_empty());
    }

    #[test]
    fn timer_reports_elapsed() {
        let timer = Timer::start();
        let rendered = timer.elapsed().to_string();
        #[cfg(not(target_arch = "wasm32"))]
        assert!(rendered.ends_with("ms"), "{rendered}");
        #[cfg(target_arch = "wasm32")]
        assert_eq!(rendered, "-");
    }

    #[test]
    fn error_field_uses_display_instead_of_debug() {
        #[derive(Debug)]
        struct Boom;

        impl fmt::Display for Boom {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("boom")
            }
        }

        impl std::error::Error for Boom {}

        let (log, collector) = handle(LogConfig::all(LogLevel::Error));
        let error = Boom;
        airx_log!(
            log,
            Scope::PROTOCOL,
            LogLevel::Error,
            "decode failed",
            phase = "decode",
            detail = &error,
        );

        assert_eq!(
            collector.lines(),
            vec!["[protocol] decode failed phase=decode detail=boom"]
        );

        // `Field::error` 与 Display 走同一条渲染路径。
        let (log, collector) = handle(LogConfig::all(LogLevel::Error));
        log.error(
            Scope::PROTOCOL,
            format_args!("failed"),
            &[Field::error("error", &error)],
        );
        assert_eq!(collector.lines(), vec!["[protocol] failed error=boom"]);
    }

    #[test]
    fn debug_field_renders_with_debug() {
        let (log, collector) = handle(LogConfig::all(LogLevel::Trace));
        let event = vec![1u8, 2];
        log.trace(
            Scope::STREAM,
            format_args!("event decoded"),
            &[Field::debug("event", &event)],
        );
        assert_eq!(
            collector.lines(),
            vec!["[stream] event decoded event=[1, 2]"]
        );
    }

    fn sample_headers() -> HeaderMap {
        use crate::http::{HeaderName, HeaderValue, header};

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_static("api-secret"),
        );
        headers
    }

    #[test]
    fn redact_headers_hides_known_secrets() {
        let rendered = redact_headers(&sample_headers());
        assert!(rendered.contains("authorization: <redacted>"), "{rendered}");
        assert!(rendered.contains("x-api-key: <redacted>"), "{rendered}");
        assert!(
            rendered.contains("content-type: application/json"),
            "{rendered}"
        );
        assert!(!rendered.contains("Bearer secret"), "{rendered}");
        assert!(!rendered.contains("api-secret"), "{rendered}");
    }

    #[test]
    fn header_redactor_replaces_builtin_redaction() {
        use crate::http::header;

        let config = LogConfig::all(LogLevel::Trace).header_redactor(|headers| {
            format!(
                "count={} auth={}",
                headers.len(),
                headers.contains_key(header::AUTHORIZATION)
            )
        });

        // 手动回调拿到全量 Header，并完全替换内置脱敏。
        assert_eq!(
            config.render_headers(&sample_headers()),
            "count=3 auth=true"
        );
    }

    #[test]
    fn body_limit_defaults_to_constant() {
        assert_eq!(LogConfig::new().body_limit_bytes(), DEFAULT_BODY_LIMIT);
        assert_eq!(LogConfig::new().body_limit(16).body_limit_bytes(), 16);
    }

    #[test]
    fn body_preview_truncates_text() {
        let body = br#"{"hello":"world"}"#;
        assert_eq!(
            body_preview(Some("application/json; charset=utf-8"), body, Some(8)),
            "{\"hello\"… (17 bytes total)"
        );
        assert_eq!(
            body_preview(Some("application/json"), body, None),
            r#"{"hello":"world"}"#
        );
    }

    #[test]
    fn body_preview_keeps_char_boundary() {
        let body = "中文".as_bytes();
        assert_eq!(
            body_preview(Some("text/plain"), body, Some(4)),
            "中… (6 bytes total)"
        );
    }

    #[test]
    fn body_preview_summarizes_binary() {
        let body = [0u8, 159, 146, 150];
        assert_eq!(
            body_preview(Some("application/octet-stream"), &body, None),
            "<binary 4 bytes>"
        );
    }

    #[test]
    fn body_preview_handles_empty_and_missing_content_type() {
        assert_eq!(body_preview(Some("application/json"), b"", None), "<empty>");
        assert_eq!(body_preview(None, b"plain", None), "plain");
        assert_eq!(body_preview(None, &[0xff, 0xfe], None), "<binary 2 bytes>");
    }

    #[test]
    fn textual_content_type_detection() {
        assert!(is_textual_content_type("text/event-stream"));
        assert!(is_textual_content_type("application/json; charset=utf-8"));
        assert!(is_textual_content_type("application/x-ndjson"));
        assert!(is_textual_content_type("application/problem+json"));
        assert!(is_textual_content_type("application/x-www-form-urlencoded"));
        assert!(!is_textual_content_type("application/octet-stream"));
        assert!(!is_textual_content_type("multipart/form-data; boundary=x"));
    }
}
