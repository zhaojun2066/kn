//! Session types and data structures.

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ── Session types ────────────────────────────────────────────

/// 会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Created,
    Running,
    Ended,
}

/// 会话类型：Native = agent 拥有 PTY，Relay = desktop 拥有 PTY（agent 仅中继）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Native,
    Relay,
}

/// 当前 PTY 视口尺寸的主控端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportOwner {
    Desktop,
    Ios,
}

impl ViewportOwner {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewportOwner::Desktop => "desktop",
            ViewportOwner::Ios => "ios",
        }
    }

    pub fn from_source(source: &str) -> Self {
        if source.eq_ignore_ascii_case("ios") {
            ViewportOwner::Ios
        } else {
            ViewportOwner::Desktop
        }
    }
}

/// 受管理的 AI CLI 会话。
#[derive(Debug, Clone)]
pub struct ManagedSession {
    /// 会话类型
    pub kind: SessionKind,
    /// 会话 nanoid（s_ + 12 字符），wire 标识符
    pub nid: String,
    /// CLI 工具类型
    pub tool: String,
    /// Profile 名称
    pub profile: Option<String>,
    /// 工作目录
    pub cwd: String,
    /// 会话来源 ("ios" | "desktop")
    pub source: String,
    /// 终端列数
    pub cols: u16,
    /// 终端行数
    pub rows: u16,
    /// 当前 PTY 视口尺寸主控端。
    pub viewport_owner: ViewportOwner,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 当前状态
    pub status: SessionStatus,
    /// 最近的用户输入（截断至 200 字符，供 checkpoint 使用）
    pub last_input: Arc<std::sync::Mutex<String>>,
    /// 最近的 PTY 输出片段（截断至 500 字符，供 checkpoint 使用）
    pub last_output_snippet: Arc<std::sync::Mutex<String>>,
    /// 首条已提交的用户输入摘要；仅作为活跃远程会话的展示元数据。
    pub display_summary: Arc<std::sync::Mutex<Option<String>>>,
    /// 在用户按下回车前累计的终端输入，避免把单个按键误当作摘要。
    pub summary_input_buffer: Arc<std::sync::Mutex<String>>,
    /// 是否已经上报过 session_ended，避免重复发送。
    pub(crate) ended_reported: Arc<AtomicBool>,
    /// 是否接受远程控制（iOS 可见/可控）。关闭后输出不发送到 WSS。
    pub remote_enabled: Arc<AtomicBool>,
    /// Relay 会话的 iOS 输入队列（agent 没有 PTY，输入由此暂存供 desktop 轮询）
    pub relay_inputs: Arc<std::sync::Mutex<Vec<(i64, String)>>>,
}

impl ManagedSession {
    /// 记录最近的用户输入（截断至 200 字符）。
    pub fn record_input(&self, text: &str) {
        let truncated: String = text.chars().take(200).collect();
        *self.last_input.lock().unwrap_or_else(|e| e.into_inner()) = truncated;
    }

    /// 记录最近的 PTY 输出片段（截断至 500 字符）。
    pub fn record_output_snippet(&self, text: &str) {
        let truncated: String = text.chars().take(500).collect();
        *self
            .last_output_snippet
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = truncated;
    }

    /// 获取最近的用户输入。
    pub fn last_input(&self) -> String {
        self.last_input
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 获取最近的 PTY 输出片段。
    pub fn last_output_snippet(&self) -> String {
        self.last_output_snippet
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Records terminal input until its first submitted line becomes the
    /// session display summary. Escape-sequence chunks are navigation/editor
    /// input, never user intent.
    pub fn record_display_summary_input(&self, text: &str) -> Option<String> {
        if text.contains('\u{1b}') {
            return None;
        }
        let mut summary = self
            .display_summary
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if summary.is_some() {
            return None;
        }
        let mut buffer = self
            .summary_input_buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for ch in text.chars() {
            match ch {
                '\r' | '\n' => {
                    let candidate = buffer.split_whitespace().collect::<Vec<_>>().join(" ");
                    buffer.clear();
                    if !candidate.is_empty() {
                        let candidate: String = candidate.chars().take(80).collect();
                        *summary = Some(candidate.clone());
                        return Some(candidate);
                    }
                }
                '\u{8}' | '\u{7f}' => {
                    buffer.pop();
                }
                value if !value.is_control() && buffer.chars().count() < 400 => buffer.push(value),
                _ => {}
            }
        }
        None
    }

    /// 标记 session_ended 是否已上报；首次调用返回 true。
    pub fn mark_ended_reported(&self) -> bool {
        !self.ended_reported.swap(true, Ordering::SeqCst)
    }

    /// 检查 session_ended 是否已上报。
    pub fn ended_reported(&self) -> bool {
        self.ended_reported.load(Ordering::SeqCst)
    }
}

/// 会话摘要（用于列表展示）。
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub nid: String,
    pub kind: SessionKind,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
    pub source: String,
    pub cols: u16,
    pub rows: u16,
    pub viewport_owner: ViewportOwner,
    pub created_at: DateTime<Utc>,
    pub status: SessionStatus,
    pub remote_enabled: bool,
    pub display_summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> ManagedSession {
        ManagedSession {
            kind: SessionKind::Native,
            nid: "s_test".into(),
            tool: "codex".into(),
            profile: None,
            cwd: "/repo".into(),
            source: "ios".into(),
            cols: 80,
            rows: 24,
            viewport_owner: ViewportOwner::Ios,
            created_at: Utc::now(),
            status: SessionStatus::Running,
            last_input: Arc::new(std::sync::Mutex::new(String::new())),
            last_output_snippet: Arc::new(std::sync::Mutex::new(String::new())),
            display_summary: Arc::new(std::sync::Mutex::new(None)),
            summary_input_buffer: Arc::new(std::sync::Mutex::new(String::new())),
            ended_reported: Arc::new(AtomicBool::new(false)),
            remote_enabled: Arc::new(AtomicBool::new(true)),
            relay_inputs: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    #[test]
    fn display_summary_uses_first_completed_input_only() {
        let session = session();
        assert_eq!(session.record_display_summary_input("修复历史"), None);
        assert_eq!(
            session.record_display_summary_input("标题\r"),
            Some("修复历史标题".into())
        );
        assert_eq!(session.record_display_summary_input("later\r"), None);
    }

    #[test]
    fn display_summary_ignores_terminal_escape_sequences() {
        let session = session();
        assert_eq!(session.record_display_summary_input("\u{1b}[A"), None);
        assert_eq!(
            session.record_display_summary_input("真实问题\n"),
            Some("真实问题".into())
        );
    }
}
