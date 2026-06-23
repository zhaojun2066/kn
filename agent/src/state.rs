//! Agent 状态机 + 崩溃持久化。
//!
//! 管理 9 种运行状态及其转换，通过 broadcast channel 推送状态变更。
//! 崩溃计数持久化到文件，启动后 60 秒自动重置。

use crate::error::{AgentError, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};

// ── State enum ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Stopped,
    Starting,
    Unbound,
    Binding,
    Connected,
    Idle,
    Running,
    Reconnecting,
}

impl AgentState {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Unbound => "unbound",
            Self::Binding => "binding",
            Self::Connected => "connected",
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Reconnecting => "reconnecting",
        }
    }
}

// ── State events ────────────────────────────────────────────

/// Events that can trigger state transitions.
#[derive(Debug, Clone)]
pub enum StateEvent {
    /// Agent 启动
    Start,
    /// WSS 连接建立（has_token: 是否有 device_token）
    WsConnected { has_token: bool },
    /// WSS 断开，开始重连
    WsDisconnected,
    /// 重连成功
    WsReconnected,
    /// 开始设备绑定
    BindInit,
    /// 绑定初始化成功（收到 bind_code）
    BindInitOk,
    /// 绑定完成（收到 device_token）
    BindResult,
    /// 绑定超时
    BindTimeout,
    /// 会话创建中 → 运行中
    SessionStarted,
    /// 所有会话结束
    AllSessionsEnded,
    /// 远程暂停
    Pause,
    /// 恢复
    Resume,
    /// 停止
    Stop,
}

// ── StateMachine ────────────────────────────────────────────

/// Agent 状态机，线程安全，推送通知。
pub struct StateMachine {
    state: RwLock<AgentState>,
    crash_count: AtomicU32,
    started_at: Instant,
    notify: broadcast::Sender<AgentState>,
}

impl StateMachine {
    /// 创建新的状态机实例。
    /// `crash_count` 从磁盘加载（0 表示首次启动或已重置）。
    pub fn new(crash_count: u32) -> Self {
        let (tx, _) = broadcast::channel(16);
        Self {
            state: RwLock::new(AgentState::Stopped),
            crash_count: AtomicU32::new(crash_count),
            started_at: Instant::now(),
            notify: tx,
        }
    }

    /// 当前状态（只读，不阻塞写者）。
    pub async fn current(&self) -> AgentState {
        *self.state.read().await
    }

    /// 订阅状态变更通知。
    pub fn subscribe(&self) -> broadcast::Receiver<AgentState> {
        self.notify.subscribe()
    }

    /// 执行状态转换。无效转换返回错误。
    pub async fn transition(&self, event: StateEvent) -> Result<AgentState> {
        let mut state = self.state.write().await;
        let next = Self::next_state(*state, &event)?;
        *state = next;
        // 推送通知（忽略无接收者的错误）
        let _ = self.notify.send(next);
        Ok(next)
    }

    /// 状态转换表（穷尽匹配，编译器检查遗漏）。
    fn next_state(current: AgentState, event: &StateEvent) -> Result<AgentState> {
        use AgentState::*;
        use StateEvent::*;

        match (current, event) {
            // 启动流程
            (Stopped, Start) => Ok(Starting),
            (Starting, WsConnected { has_token: true }) => Ok(Connected),
            (Starting, WsConnected { has_token: false }) => Ok(Unbound),

            // 绑定流程
            (Unbound, BindInit) => Ok(Binding),
            (Binding, BindInit) => Ok(Binding), // 幂等：已在绑定中
            (Binding, BindInitOk) => Ok(Binding), // 保持 Binding，等待结果
            (Binding, BindResult) => Ok(Connected),
            (Binding, BindTimeout) => Ok(Unbound),

            // 会话管理
            (Connected, SessionStarted) => Ok(Running),
            (Running, SessionStarted) => Ok(Running), // 已运行，保持
            (Running, AllSessionsEnded) => Ok(Idle),
            (Idle, SessionStarted) => Ok(Running),

            // 连接管理
            (Connected, WsDisconnected) => Ok(Reconnecting),
            (Running, WsDisconnected) => Ok(Reconnecting),
            (Idle, WsDisconnected) => Ok(Reconnecting),
            (Reconnecting, WsReconnected) => Ok(Connected),
            (Reconnecting, WsConnected { has_token: true }) => Ok(Connected),

            // 暂停/恢复（恢复前应由调用方验证 WSS 状态）
            (Connected, Pause) => Ok(Idle),
            (Running, Pause) => Ok(Idle),
            // Resume: 从 Idle 恢复到 Connected 的前提是 WSS 连接仍然存活。
            // 调用方应在 resume 前通过 WSS 心跳确认连接状态。
            (Idle, Resume) => Ok(Connected),

            // Token 失效/AUTH_REJECTED：回到未绑定状态
            // 从任何活跃/重连状态回到 Unbound（需重新绑定）
            (Connected, WsConnected { has_token: false }) => Ok(Unbound),
            (Running, WsConnected { has_token: false }) => Ok(Unbound),
            (Idle, WsConnected { has_token: false }) => Ok(Unbound),
            (Reconnecting, WsConnected { has_token: false }) => Ok(Unbound),

            // 停止
            (_, Stop) => Ok(Stopped),

            // 无效转换
            (current, event) => Err(AgentError::StateTransition {
                from: current.name().to_string(),
                event: format!("{:?}", event),
            }),
        }
    }

    // ── Crash count management ──────────────────────────────

    /// 当前崩溃计数。
    pub fn crash_count(&self) -> u32 {
        self.crash_count.load(Ordering::Relaxed)
    }

    /// 递增崩溃计数并返回新值。
    pub fn increment_crash(&self) -> u32 {
        self.crash_count.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 重置崩溃计数为 0。
    pub fn reset_crash(&self) {
        self.crash_count.store(0, Ordering::Relaxed);
    }

    /// 是否处于安全模式（崩溃次数 > 5）。
    pub fn in_safe_mode(&self) -> bool {
        self.crash_count() > 5
    }

    /// 运行时长（秒）。
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// 从磁盘加载崩溃计数。
    pub fn load_crash_count() -> u32 {
        let path = crash_count_path();
        match std::fs::read_to_string(&path) {
            Ok(s) => s.trim().parse().unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// 持久化崩溃计数到磁盘（原子写入）。
    pub fn persist_crash_count(count: u32) {
        let path = crash_count_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, count.to_string()) {
            eprintln!("[agent] 崩溃计数写入失败: {}", e);
            return;
        }
        if let Ok(f) = std::fs::File::open(&tmp) {
            let _ = f.sync_all();
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            eprintln!("[agent] 崩溃计数持久化失败: {}", e);
        }
    }

    /// 清除崩溃计数文件。
    pub fn clear_crash_count() {
        let path = crash_count_path();
        let _ = std::fs::remove_file(&path);
    }
}

fn crash_count_path() -> PathBuf {
    kn_common::path::config_dir().join("agent").join("crash_count")
}

/// 仅测试用：覆盖 crash count 路径。
#[cfg(test)]
fn set_test_crash_dir(dir: &std::path::Path) {
    // 直接设置 KN_HOME 是测试中最可靠的方式（每次调用 config_dir() 会重新读取）
    // 但需要串行执行，用 --test-threads=1
    std::env::set_var("KN_HOME", dir.to_str().unwrap());
}

#[cfg(test)]
fn clear_test_env() {
    std::env::remove_var("KN_HOME");
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sm(crash_count: u32) -> StateMachine {
        StateMachine::new(crash_count)
    }

    #[tokio::test]
    async fn test_startup_flow_with_token() {
        let m = sm(0);
        assert_eq!(m.current().await, AgentState::Stopped);
        m.transition(StateEvent::Start).await.unwrap();
        assert_eq!(m.current().await, AgentState::Starting);
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Connected);
    }

    #[tokio::test]
    async fn test_startup_flow_without_token() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Unbound);
    }

    #[tokio::test]
    async fn test_bind_flow() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        m.transition(StateEvent::BindInit).await.unwrap();
        assert_eq!(m.current().await, AgentState::Binding);
        m.transition(StateEvent::BindResult).await.unwrap();
        assert_eq!(m.current().await, AgentState::Connected);
    }

    #[tokio::test]
    async fn test_bind_timeout() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        m.transition(StateEvent::BindInit).await.unwrap();
        m.transition(StateEvent::BindTimeout).await.unwrap();
        assert_eq!(m.current().await, AgentState::Unbound);
    }

    #[tokio::test]
    async fn test_session_flow() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        m.transition(StateEvent::SessionStarted).await.unwrap();
        assert_eq!(m.current().await, AgentState::Running);
        m.transition(StateEvent::AllSessionsEnded).await.unwrap();
        assert_eq!(m.current().await, AgentState::Idle);
    }

    #[tokio::test]
    async fn test_disconnect_reconnect() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        m.transition(StateEvent::WsDisconnected).await.unwrap();
        assert_eq!(m.current().await, AgentState::Reconnecting);
        m.transition(StateEvent::WsReconnected).await.unwrap();
        assert_eq!(m.current().await, AgentState::Connected);
    }

    #[tokio::test]
    async fn test_invalid_transition_rejected() {
        let m = sm(0);
        // Cannot go from Stopped to Running directly
        let result = m.transition(StateEvent::SessionStarted).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_stop_from_any_state() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        m.transition(StateEvent::Stop).await.unwrap();
        assert_eq!(m.current().await, AgentState::Stopped);
    }

    #[test]
    fn test_crash_count_persistence() {
        let dir = std::env::temp_dir().join(format!("kn-test-crash-{}", std::process::id()));
        set_test_crash_dir(&dir);

        // Initial load should be 0
        assert_eq!(StateMachine::load_crash_count(), 0);

        // Persist and reload
        StateMachine::persist_crash_count(3);
        assert_eq!(StateMachine::load_crash_count(), 3);

        // Clear
        StateMachine::clear_crash_count();
        assert_eq!(StateMachine::load_crash_count(), 0);

        clear_test_env();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_safe_mode() {
        let m = sm(0);
        assert!(!m.in_safe_mode());
        m.increment_crash();
        m.increment_crash();
        m.increment_crash();
        m.increment_crash();
        m.increment_crash();
        m.increment_crash(); // 6 > 5
        assert!(m.in_safe_mode());
    }

    #[tokio::test]
    async fn test_broadcast_notification() {
        let m = sm(0);
        let mut rx = m.subscribe();

        m.transition(StateEvent::Start).await.unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received, AgentState::Starting);
    }

    #[tokio::test]
    async fn test_auth_rejected_from_connected() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Connected);
        // Token 失效 → Unbound
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Unbound);
    }

    #[tokio::test]
    async fn test_auth_rejected_from_reconnecting() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        m.transition(StateEvent::WsDisconnected).await.unwrap();
        assert_eq!(m.current().await, AgentState::Reconnecting);
        // Token 失效 → Unbound
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Unbound);
    }

    #[tokio::test]
    async fn test_auth_rejected_from_running() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        m.transition(StateEvent::SessionStarted).await.unwrap();
        assert_eq!(m.current().await, AgentState::Running);
        // Token 失效 → Unbound
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Unbound);
    }

    #[tokio::test]
    async fn test_auth_rejected_from_idle() {
        let m = sm(0);
        m.transition(StateEvent::Start).await.unwrap();
        m.transition(StateEvent::WsConnected { has_token: true }).await.unwrap();
        m.transition(StateEvent::SessionStarted).await.unwrap();
        m.transition(StateEvent::AllSessionsEnded).await.unwrap();
        assert_eq!(m.current().await, AgentState::Idle);
        // Token 失效 → Unbound
        m.transition(StateEvent::WsConnected { has_token: false }).await.unwrap();
        assert_eq!(m.current().await, AgentState::Unbound);
    }
}
