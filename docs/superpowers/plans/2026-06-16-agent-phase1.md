# Agent Phase 1 — Rust 骨架 + WSS + 设备绑定

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 kn repo 内新建 Agent 独立二进制 (`kn-agent`)，实现 launchd 守护、设备绑定完整流程、WSS 长连接、session 基本管理。

**Architecture:** 采用 Cargo workspace 组织——`common/` (公共库) + `agent/` (守护进程 binary crate) + `desktop/src-tauri/` (Tauri app)。三方共享 `common/` 中的 `commands.rs`、`profile_cmd.rs`、`fingerprint.rs`、`PtyOutputSink` trait。

```
kn/                              # Monorepo 根
├── Cargo.toml                   # [workspace]
├── common/                      # kn-common (公共库)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── commands.rs          # home_dir(), find_binary()
│       ├── profile_cmd.rs       # profile 读取
│       ├── fingerprint.rs       # IOPlatformUUID
│       ├── config_crypto.rs     # AES-256-GCM env var 加密 + macOS Keychain
│       └── pty_trait.rs         # PtyOutputSink trait
├── agent/                       # kn-agent (binary)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── state.rs
│       ├── ws_client.rs
│       ├── session.rs
│       ├── proto.rs
│       ├── ipc.rs
│       └── launchd.rs
└── desktop/src-tauri/           # kn (Tauri app)
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── pty.rs
        └── ...
```

**Tech Stack:** Rust + tokio + tokio-tungstenite + portable-pty + serde/serde_json + fs2

**Prerequisites:** `feat/remote-control-design` 分支已存在，设计文档已就绪。

---

## Pre-flight: Workspace 搭建 + 公共库提取

### Task 1: Cargo workspace + kn-common 公共库 + kn-agent 骨架

**Files:**
- Create: `Cargo.toml` (repo root, workspace)
- Create: `common/Cargo.toml`
- Create: `common/src/lib.rs`
- Create: `common/src/commands.rs` (从 `desktop/src-tauri/src/commands.rs` 迁移)
- Create: `common/src/profile_cmd.rs` (从 `desktop/src-tauri/src/profile_cmd.rs` 迁移)
- Create: `common/src/fingerprint.rs` (设备指纹)
- Create: `common/src/pty_trait.rs` (PtyOutputSink + 共享类型)
- Create: `agent/Cargo.toml`
- Create: `agent/src/main.rs`
- Modify: `desktop/src-tauri/Cargo.toml` (添加 `kn-common` 依赖，移除已迁移模块)
- Modify: `desktop/src-tauri/src/lib.rs` (更新 import 路径)
- Modify: `desktop/src-tauri/src/commands.rs` (指向 common 或删除重导出)

- [ ] **Step 1: 创建 repo 根 Cargo.toml (workspace)**

创建 `/Users/zhaojun/workspace/me/shark/kn/Cargo.toml`:

```toml
[workspace]
members = [
    "common",
    "agent",
    "desktop/src-tauri",
]
resolver = "2"

[workspace.package]
version = "0.0.0"
edition = "2021"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
fs2 = "0.4"
sha2 = "0.10"
chrono = { version = "0.4", features = ["serde"] }
base64 = "0.22"
```

- [ ] **Step 2: 创建 common/ crate (kn-common)**

创建 `common/Cargo.toml`:

```toml
[package]
name = "kn-common"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
fs2.workspace = true
sha2.workspace = true
chrono.workspace = true
base64.workspace = true
```

创建 `common/src/lib.rs`:

```rust
//! kn-common — Desktop 与 Agent 共享的公共模块

pub mod commands;
pub mod profile_cmd;
pub mod fingerprint;
pub mod pty_trait;
pub mod config_crypto;
```

- [ ] **Step 3: 迁入共享代码**

**commands.rs** — 从 `desktop/src-tauri/src/commands.rs` 复制 `home_dir()` 和 `find_binary()` 两个函数到 `common/src/commands.rs`。去掉 Tauri 特有的依赖。**注意**：当前 `find_binary` 在 desktop crate 中为 `pub(crate)`，迁移到 `kn_common` 后需改为 `pub` 以便 Agent crate 跨 crate 调用。

**profile_cmd.rs** — 从 `desktop/src-tauri/src/profile_cmd.rs` 完整复制到 `common/src/profile_cmd.rs`。该文件已不依赖 Tauri。

**fingerprint.rs** — 创建 `common/src/fingerprint.rs`，实现 `machine_id()` 函数（见下方 Task 2）。

**config_crypto.rs** — 创建 `common/src/config_crypto.rs`，实现 config value 的加密/解密：

```rust
//! profile env var value 加密存储 — AES-256-GCM + macOS Keychain
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, OsRng};
use security_framework::os::macos::keychain::SecKeychain;
use security_framework::os::macos::item::{ItemSearchOptions, ItemAddOptions};

const KEYCHAIN_SERVICE: &str = "com.kn.agent";
const KEYCHAIN_ACCOUNT: &str = "config-key";
const CIPHER_PREFIX: &str = "kn:v1:";

/// 首次运行时生成 256-bit 主密钥存入 macOS Keychain，后续读取
fn load_or_create_key() -> Result<Key<Aes256Gcm>, String> {
    // 1. 读 Keychain
    if let Some(data) = keychain_get(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        if data.len() == 32 { return Ok(*Key::<Aes256Gcm>::from_slice(&data)); }
    }
    // 2. 不存在 → 生成新密钥 → 存 Keychain
    let key = Aes256Gcm::generate_key(OsRng);
    keychain_add(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, key.as_slice())?;
    Ok(key)
}

/// 加密 value → "kn:v1:{nonce_hex}{ciphertext_hex}"
pub fn encrypt_value(value: &str) -> Result<String, String> {
    let key = load_or_create_key()?;
    let cipher = Aes256Gcm::new(&key);
    let nonce_bytes = rand::random::<[u8; 12]>();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, value.as_bytes()).map_err(|e| e.to_string())?;
    let mut result = String::from(CIPHER_PREFIX);
    for b in &nonce_bytes { result.push_str(&format!("{:02x}", b)); }
    for b in &ct { result.push_str(&format!("{:02x}", b)); }
    Ok(result)
}

/// 解密 "kn:v1:..." → 原始 value，无前缀的明文直接返回（向前兼容）
pub fn decrypt_value(encoded: &str) -> Result<String, String> {
    if !encoded.starts_with(CIPHER_PREFIX) { return Ok(encoded.to_string()); }
    let hex = &encoded[CIPHER_PREFIX.len()..];
    let nonce_bytes = hex_to_bytes(&hex[..24])?;
    let ct = hex_to_bytes(&hex[24..])?;
    let key = load_or_create_key()?;
    let cipher = Aes256Gcm::new(&key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plain = cipher.decrypt(nonce, ct.as_ref()).map_err(|e| e.to_string())?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}
```

`common/Cargo.toml` 新增依赖：
```toml
aes-gcm = "0.10"
rand = "0.8"
security-framework = "2"  # macOS Keychain API
```

**pty_trait.rs** — 创建 `common/src/pty_trait.rs`:

```rust
use std::sync::{Arc, Mutex};
use std::io::Write;

/// PTY 输出接收器 — Tauri 和 Agent 各自实现
pub trait PtyOutputSink: Send + 'static {
    fn send(&self, data: &[u8]) -> Result<(), String>;
    fn on_ready(&self) -> Result<(), String> { Ok(()) }
    fn on_exit(&self, code: i32) -> Result<(), String> { Ok(()) }
    fn on_error(&self, msg: &str) -> Result<(), String> { Ok(()) }
}

pub type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;
pub type SharedChild = Arc<Mutex<Option<Box<dyn portable_pty::Child + Send>>>>;
```

注意：`pty_trait.rs` 中引用 `portable_pty::Child` 需要在 `common/Cargo.toml` 中添加 `portable-pty = "0.8"`。

**config.yaml 跨进程写安全**：Agent 和 Desktop 是独立进程，共享 `~/.kn/config.yaml`。`profile_cmd.rs`（迁入 common/）已包含完整的写入保护：

- **跨进程文件锁**：`fs2::lock_exclusive()` on `~/.kn/.config.lock`，与 Python CLI 的 `fcntl.flock` 互操作
- **3 代备份轮转**：`.bak → .bak.1 → .bak.2 → .bak.3`，每次写入前旋转，防止坏写覆盖唯一恢复点
- **原子写**：tmp 文件 → `fsync` → `rename`，与 Python CLI `lib/config.py` 的写入模式一致

Agent 在 tokio 异步上下文中调用 `profile_cmd.rs` 的写函数时，需用 `tokio::task::spawn_blocking` 包装。Desktop 和 Agent 复用同一套 `common/` 中的写入函数，无需各自实现。

- [ ] **Step 4: 创建 agent/ crate (kn-agent)**

创建 `agent/Cargo.toml`:

```toml
[package]
name = "kn-agent"
version.workspace = true
edition.workspace = true

[[bin]]
name = "kn-agent"
path = "src/main.rs"

[dependencies]
kn-common = { path = "../common" }
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
futures-util = "0.3"
nanoid = "0.4"                    # session_id 去中心化生成
	http = "1"                        # ws_client: WSS 请求构造
	hostname = "0.3"                  # ws_client: WSS 握手 header X-KN-Hostname + QR 码（0.3+ 返回 OsString，支持 to_string_lossy）
	portable-pty = "0.8"              # PTY spawn (Task 15)
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
```

创建 `agent/src/main.rs`:

```rust
//! kn-agent 独立二进制入口 — launchd 守护进程
//!
//! 用法:
//!   kn-agent                          # launchd 启动（默认）
//!   kn-agent --new --tool <t> ...     # CLI 模式: 通过 IPC 创建 session

fn main() {
    eprintln!("kn-agent starting...");
    // Phase 1: 直接连 WSS，后续加 tokio runtime + IPC + CLI
}
```

- [ ] **Step 5: 更新 Desktop crate 的 Cargo.toml**

修改 `desktop/src-tauri/Cargo.toml`:

```toml
[dependencies]
kn-common = { path = "../../../common" }
# 删除原来对 commands.rs / profile_cmd.rs 的直接模块依赖
# 改为 use kn_common::commands / use kn_common::profile_cmd
tauri = { version = "2", features = [] }
# ... 其他依赖不变
portable-pty = "0.8"
# reqwest: Desktop 用 blocking，Agent 用 async。两个 crate 独立，不冲突
reqwest = { version = "0.12", features = ["blocking", "json"] }
# Agent Cargo.toml 中: reqwest = { version = "0.12", features = ["json"] }  (默认 async)
```

注意：当前 Tauri Rust 代码中 `use crate::commands::` 改为 `use kn_common::commands::`，`use crate::profile_cmd::` 改为 `use kn_common::profile_cmd::`。导入路径更新后确保 Desktop 编译通过。

- [ ] **Step 6: 更新 Desktop lib.rs import**

在 `desktop/src-tauri/src/lib.rs` 中更新所有 `crate::commands` 和 `crate::profile_cmd` 引用为 `kn_common::commands` 和 `kn_common::profile_cmd`。

`desktop/src-tauri/src/commands.rs` 可改为简单的重导出文件:

```rust
// 重导出 common 中的函数，保持向后兼容
pub use kn_common::commands::*;
```

或直接删除并在所有引用处改为 `use kn_common::commands`。

- [ ] **Step 7: 编译验证**

```bash
cd /Users/zhaojun/workspace/me/shark/kn
cargo check --workspace --all-targets
```

预期: `kn-common`、`kn-agent`、`kn` 全部编译通过。

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml common/ agent/
git add desktop/src-tauri/Cargo.toml
git add desktop/src-tauri/src/lib.rs  # import 更新
git add desktop/src-tauri/src/commands.rs  # 重导出或删除
git add desktop/src-tauri/src/profile_cmd.rs  # 改为重导出或删除
git commit -m "feat: Cargo workspace with kn-common + kn-agent + kn"
```

---

### Task 2: 设备指纹采集

**说明**：`machine_id()` 已在 Task 1 Step 3 中创建于 `common/src/fingerprint.rs`。本 Task 不做额外工作，只需在后续 Agent 代码中通过 `use kn_common::fingerprint::machine_id;` 引用即可。

**验证**：

```bash
cd /Users/zhaojun/workspace/me/shark/kn
cargo test -p kn-common fingerprint::tests
# 或在 Agent 代码中引用后通过 agent 测试：
cargo test --bin kn-agent
```

---

### Task 3: Agent 状态机

**Files:**
- Create: `agent/src/state.rs`

- [ ] **Step 1: 实现状态机**

创建 `agent/src/state.rs`：

```rust
//! Agent 状态机
//!
//! stopped → starting → connected → idle ⇄ running
//!                  ↘ unbound → binding → connected
//! 任何状态 → reconnecting → (恢复或 stopped)

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
    pub fn as_str(&self) -> &'static str {
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

/// 状态机管理器（线程安全）
pub struct StateMachine {
    state: std::sync::Mutex<AgentState>,
    crash_count: std::sync::atomic::AtomicU32,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(AgentState::Stopped),
            crash_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    pub fn current(&self) -> AgentState {
        *self.state.lock().unwrap()
    }

    pub fn transition(&self, event: StateEvent) -> Result<AgentState, String> {
        let mut state = self.state.lock().unwrap();
        let (from, to) = match (*state, event) {
            (AgentState::Stopped, StateEvent::Start) => (AgentState::Stopped, AgentState::Starting),
            (AgentState::Starting, StateEvent::WsConnected { has_token: true }) => (AgentState::Starting, AgentState::Connected),
            (AgentState::Starting, StateEvent::WsConnected { has_token: false }) => (AgentState::Starting, AgentState::Unbound),
            (AgentState::Starting, StateEvent::WsFailed) => (AgentState::Starting, AgentState::Stopped),
            (AgentState::Connected, StateEvent::HandshakeDone) => (AgentState::Connected, AgentState::Idle),
            (AgentState::Unbound, StateEvent::BindInitOk) => (AgentState::Unbound, AgentState::Binding),
            (AgentState::Binding, StateEvent::BindResult) => (AgentState::Binding, AgentState::Connected),
            (AgentState::Binding, StateEvent::BindTimeout) => (AgentState::Binding, AgentState::Unbound),
            (AgentState::Idle, StateEvent::SessionCreated) => (AgentState::Idle, AgentState::Running),
            (AgentState::Running, StateEvent::AllSessionsEnded) => (AgentState::Running, AgentState::Idle),
            (s, StateEvent::WsDisconnected) if s != AgentState::Stopped && s != AgentState::Reconnecting
                => (s, AgentState::Reconnecting),
            // 重连成功后回到 Connected，后续 HandshakeDone → Idle（避免从 Idle 触发 HandshakeDone 死路）
            (AgentState::Reconnecting, StateEvent::WsConnected { .. }) => (AgentState::Reconnecting, AgentState::Connected),
            // 兜底：已处于 Idle 时收到 HandshakeDone（如重连后协议版本协商被跳过），无操作
            (AgentState::Idle, StateEvent::HandshakeDone) => (AgentState::Idle, AgentState::Idle),
            // 注意：WsFailed 仅在 ws_client 指数退避重试全部耗尽后（当前为无限重试时不触发）才由上层调用。
            // ws_client 内部的重试循环不反馈给 StateMachine；仅当重试循环决定放弃时才触发此事件。
            // 如果 ws_client 实现为无限重试，则此转换永不触发；Agent 通过 launchd KeepAlive 兜底。
            (AgentState::Reconnecting, StateEvent::WsFailed) => (AgentState::Reconnecting, AgentState::Stopped),
            (s, StateEvent::Pause) if s != AgentState::Stopped => (s, AgentState::Stopped),
            _ => return Err(format!("无效转换: {:?} + {:?}", *state, event)),
        };
        *state = to;
        Ok(to)
    }

    // Crash 退避
    pub fn increment_crash(&self) -> u32 {
        self.crash_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
    }

    pub fn reset_crash(&self) {
        self.crash_count.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn crash_count(&self) -> u32 {
        self.crash_count.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn in_safe_mode(&self) -> bool {
        self.crash_count() > 5
    }
}

#[derive(Debug)]
pub enum StateEvent {
    Start,
    WsConnected { has_token: bool },
    HandshakeDone,
    WsFailed,
    WsDisconnected,
    BindInitOk,
    BindResult,
    BindTimeout,
    SessionCreated,
    AllSessionsEnded,
    Pause,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_lifecycle() {
        let sm = StateMachine::new();
        assert_eq!(sm.current(), AgentState::Stopped);

        sm.transition(StateEvent::Start).unwrap();
        assert_eq!(sm.current(), AgentState::Starting);

        sm.transition(StateEvent::WsConnected { has_token: true }).unwrap();
        assert_eq!(sm.current(), AgentState::Connected);

        sm.transition(StateEvent::HandshakeDone).unwrap();
        assert_eq!(sm.current(), AgentState::Idle);

        sm.transition(StateEvent::SessionCreated).unwrap();
        assert_eq!(sm.current(), AgentState::Running);

        sm.transition(StateEvent::AllSessionsEnded).unwrap();
        assert_eq!(sm.current(), AgentState::Idle);
    }

    #[test]
    fn test_unbound_to_binding() {
        let sm = StateMachine::new();
        sm.transition(StateEvent::Start).unwrap();
        sm.transition(StateEvent::WsConnected { has_token: false }).unwrap();
        assert_eq!(sm.current(), AgentState::Unbound);

        sm.transition(StateEvent::BindInitOk).unwrap();
        assert_eq!(sm.current(), AgentState::Binding);

        sm.transition(StateEvent::BindResult).unwrap();
        assert_eq!(sm.current(), AgentState::Connected);
    }

    #[test]
    fn test_crash_count() {
        let sm = StateMachine::new();
        assert_eq!(sm.crash_count(), 0);
        assert!(!sm.in_safe_mode());

        for _ in 0..6 {
            sm.increment_crash();
        }
        assert!(sm.in_safe_mode());

        sm.reset_crash();
        assert!(!sm.in_safe_mode());
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cd /Users/zhaojun/workspace/me/shark/kn && cargo test --bin kn-agent state::tests
```

预期：3 个测试全部 PASS。

- [ ] **Step 3: Commit**

```bash
git add agent/src/state.rs
git commit -m "feat(agent): add state machine with crash backoff"
```

---

### Task 4: 消息协议定义

**Files:**
- Create: `agent/src/proto.rs`

- [ ] **Step 1: 实现消息类型**

创建 `agent/src/proto.rs`：

```rust
//! WSS 消息协议定义
//!
//! 所有消息使用 JSON，外层结构统一（type + seq + ts + session_id + data）

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 客户端 → 服务端（inbound）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    Ping,
    StartSession {
        session_id: String,  // s_ + 12位 nanoid，由发起方本地生成
        tool: String,
        profile: Option<String>,
        cwd: String,
        cols: u16,
        rows: u16,
    },
    ResizePty {
        session_id: String,
        cols: u16,
        rows: u16,
    },
    Redeem {
        code: String,
    },
    Input {
        session_id: String,
        text: String,
    },
    Ctrl {
        session_id: String,
        signal: String,
    },
    KillSession {
        session_id: String,
        reason: Option<String>,
    },
    LockSession {
        session_id: String,
        lock: bool,
    },
    /// 写入文件到 Agent 本地磁盘
    /// **安全约束（设计文档 §6.3）**：
    ///   - path 必须在 session cwd 子树内（Agent 端校验，拒绝 ../ 逃逸）
    ///   - 需 iOS 端二次确认后才能执行（Agent 收到后先返回 write_file_confirm_request，
    ///     iOS 用户确认后再发 write_file_confirmed）
    WriteFile {
        session_id: String,
        path: String,
        content: String,
    },
    /// 读取 Agent 本地文件（仅限 cwd 子树）
    ReadOutputLog {
        session_id: String,
        offset: Option<u64>,
        limit: Option<u64>,
    },
    Ack {
        msg_seq: u64,
    },
    /// Agent 上报可用 profile 列表（仅名称/类型/描述，不含 env vars）
    /// 在 WSS 连接建立后立即推送，profile 变更时重新推送
    ProfileList {
        profiles: Vec<ProfileInfo>,
    },
}

/// 服务端 → 客户端（outbound）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    Pong,
    Connected {
        ws_session_id: String,
        protocol_version: Option<u32>,
    },
    BindResult {
        device_token: Option<String>,
        error: Option<String>,
    },
    RedeemResult {
        ok: bool,
        plan: Option<String>,
        message: Option<String>,
    },
    SessionCreated {
        session_id: String,
        status: String,
    },
    SessionEnded {
        session_id: String,
        reason: String,
        exit_code: Option<i32>,
    },
    SessionInterrupted {
        session_id: String,
        last_input: String,
        cwd: String,
        tool: String,
        profile: Option<String>,
    },
    Output {
        session_id: String,
        ansi_text: String,
    },
    StateChange {
        session_id: String,
        change: String,
        by: String,
    },
    ProfileUpdate {
        profiles: Vec<ProfileInfo>,
    },
    AgentError {
        device_id: Option<String>,
        code: String,
        message: String,
    },
    CurrentState {
        sessions: Vec<SessionState>,
    },
    MissedMessages {
        messages: Vec<serde_json::Value>,
        from_seq: u64,
        to_seq: u64,
    },
    ErrorNotify {
        code: String,
        message: String,
    },
    /// 设备在线状态变化通知（iOS 端设备列表实时更新）
    DeviceStatus {
        device_id: u64,
        status: String,  // "online" | "offline" | "paused"
    },
}

/// Profile 简要信息（profile_list / profile_update 共用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub name: String,
    pub tool: String,
    pub desc: Option<String>,
}

/// Session 状态快照（current_state 用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub status: String,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
}

impl ClientMessage {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl ServerMessage {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// ── 重放攻击防护说明 ──
// 设计文档 §6.6 提到 "每条指令带 nonce"，但 WSS 协议外层使用 `seq`（单调递增）
// 替代 random nonce。理由：
//   - seq 由发送方分配（会话内单调递增），云端在 Redis 中以 `msg:dedup:{session_id}:{seq}`
//     (TTL 5min) 实现幂等去重，效果等价于 nonce 一次一密
//   - seq 兼用作"丢包检测 + 乱序重组"（比 random nonce 多一个能力维度）
//   - 5 分钟去重窗口足以覆盖网络重传和弱网重发窗口
// 如果未来需要更强的防重放（超过 5min 窗口），可在消息外层增加 nonce 字段做双因子。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_input() {
        let msg = ClientMessage::Input {
            session_id: "s1".into(),
            text: "hello\n".into(),
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("input"));
        assert!(json.contains("hello"));
    }

    #[test]
    fn test_deserialize_session_created() {
        let json = r#"{"type":"session_created","session_id":"s1","status":"pending"}"#;
        let msg = ServerMessage::from_json(json).unwrap();
        match msg {
            ServerMessage::SessionCreated { session_id, status } => {
                assert_eq!(session_id, "s1");
                assert_eq!(status, "pending");
            }
            _ => panic!("wrong type"),
        }
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cd desktop && cargo test --bin kn-agent proto::tests
```

- [ ] **Step 3: Commit**

```bash
git add agent/src/proto.rs
git commit -m "feat(agent): add WSS message protocol types"
```

---

### Task 5: WebSocket 客户端

**Files:**
- Create: `agent/src/ws_client.rs`
- Modify: `agent/src/main.rs` (添加 `mod ws_client`, 集成 WSS 连接)

- [ ] **Step 0: 添加 reqwest 依赖**

`agent/Cargo.toml` 新增：
```toml
reqwest = { version = "0.12", features = ["json"] }  # async 模式，Task 12 bind-init HTTP 调用需要
```

- [ ] **Step 1: 实现 WSS 客户端**

创建 `agent/src/ws_client.rs`：

```rust
//! WebSocket 客户端 — 连接云服务 WSS，自动重连，心跳保活
//!
//! 正式连接模式，服务端按 token 字符长度自动路由（无需 DB 查询）：
//!   - 正式连接 (Authorization: Bearer <device_token>, >6 字符)：日常运行，全部功能
//!     device 不存在时服务端以 close code 4003 关闭，Agent 回到 unbound 重新绑定
//! 
//! 注意：绑定流程不再通过 WSS 临时连接。Agent 在绑定期间通过 HTTP 短轮询
//! GET /api/v1/device/bind-result?code=xxx 获取 device_token，收到后再建立正式 WSS。
//! 
//! 服务端 URL 从环境变量 KN_CLOUD_URL 读取，默认 wss://api.shark.kim

use kn_common::fingerprint;
use crate::proto::{ClientMessage, ServerMessage};
use crate::state::{AgentState, StateEvent, StateMachine};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const PING_INTERVAL_SECS: u64 = 15;
const PONG_TIMEOUT_SECS: u64 = 90; // 扛住 macOS 短暂休眠
const SESSION_FAILED_AFTER_SECS: u64 = 1800; // 30min，避免休眠时误杀 session

/// 获取云服务 URL（环境变量可覆盖，默认 production）
fn cloud_ws_url() -> String {
    std::env::var("KN_CLOUD_URL")
        .unwrap_or_else(|_| "wss://api.shark.kim".into())
        .trim_end_matches('/')
        .to_string()
}

/// Agent 连接 WSS（正式模式）
pub async fn connect(
    device_token: &str,
    state: Arc<StateMachine>,
    tx: mpsc::UnboundedSender<ServerMessage>,
) -> Result<(), String> {
    let machine_id = fingerprint::machine_id()?;
    let os_version = std::process::Command::new("sw_vers")
        .arg("-productVersion").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());
    let url = format!("{}/v1/ws", cloud_ws_url());
    let auth_header = format!("Bearer {}", device_token);
    run_connection(&url, &auth_header, &machine_id, &os_version, &hostname, state, tx).await
}

async fn run_connection(
    url: &str,
    auth_header: &str,
    machine_id: &str,
    os_version: &str,
    hostname: &str,
    state: Arc<StateMachine>,
    tx: mpsc::UnboundedSender<ServerMessage>,
) -> Result<(), String> {
    let request = http::Request::builder()
        .uri(url)
        .header("Authorization", auth_header)
        .header("X-KN-Role", "kn-agent")
        .header("X-KN-Machine-Id", machine_id)
        .header("X-KN-Protocol-Version", "1")
        .header("X-KN-Agent-Version", env!("CARGO_PKG_VERSION"))
        .header("X-KN-OS-Version", &os_version)
        .header("X-KN-Hostname", &hostname)
        .body(())
        .map_err(|e| format!("构建请求失败: {}", e))?;
    let (ws_stream, _) = connect_async(request)
        .await
        .map_err(|e| format!("WSS 连接失败: {}", e))?;

    state.transition(StateEvent::WsConnected { has_token: true })
        .map_err(|e| format!("状态转换失败: {}", e))?;

    let (mut write, mut read) = ws_stream.split();
    let mut ping_tick = interval(Duration::from_secs(PING_INTERVAL_SECS));

    // 消息去重：记录已处理的 (session_id, seq) 组合，避免重连后重复投递
    let mut seen_seqs: HashSet<(String, u64)> = HashSet::new();

    loop {
        tokio::select! {
            _ = ping_tick.tick() => {
                // 发 ping
                let ping = ClientMessage::Ping;
                let json = serde_json::to_string(&ping).unwrap();
                if write.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
                // 等 pong
                // **已知限制**：pong 等待期间会阻塞消息读取，90s 内服务端推送的 start_session
                // 等消息无法被处理。v2 优化方向：使用独立的 last_pong_time 变量 + 定时检查，
                // 而非阻塞 timeout。当前为 Phase 1 简化实现，大部分场景下 pong 在 1s 内返回。
                let pong_fut = timeout(Duration::from_secs(PONG_TIMEOUT_SECS), read.next());
                match pong_fut.await {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        if let Ok(ServerMessage::Pong) = ServerMessage::from_json(&text) {
                            continue;
                        }
                        // 不是 pong，可能是其他消息，转发给处理器
                        if let Ok(msg) = ServerMessage::from_json(&text) {
                            let _ = tx.send(msg);
                        }
                    }
                    _ => break, // 超时或断线
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // 去重：解析外层 JSON 取 seq 字段，最近 1000 个 seq hash 去重
                        if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let (Some(sid), Some(seq)) = (
                                raw.get("session_id").and_then(|v| v.as_str()),
                                raw.get("seq").and_then(|v| v.as_u64()),
                            ) {
                                let dedup_key = (sid.to_string(), seq);
                                if seen_seqs.contains(&dedup_key) { continue; }
                                seen_seqs.insert(dedup_key);
                                if seen_seqs.len() > 1000 { seen_seqs.clear(); }
                            }
                        }
                        if let Ok(msg) = ServerMessage::from_json(&text) {
                            let _ = tx.send(msg);
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Err(_)) => break,
                    None => break,
                    _ => {}
                }
            }
        }
    }

    state.transition(StateEvent::WsDisconnected).ok();
    Err("WSS 连接断开".into())
}
```

- [ ] **Step 2: 在 main.rs 中声明模块**

`agent/src/main.rs` 中确保 `mod ws_client;` 已声明（与其他 `mod` 声明并列，Task 1 已创建骨架）。

- [ ] **Step 3: 更新 main.rs 做冒烟测试**

修改 `agent/src/main.rs`：

```rust
use std::sync::Arc;
use tokio::sync::mpsc;

use kn_common::fingerprint;
mod state;
mod ws_client;
mod session;
mod proto;

use state::{StateMachine, StateEvent};

#[tokio::main]
async fn main() {
    let state = Arc::new(StateMachine::new());
    state.transition(StateEvent::Start).unwrap();

    // 检查 device_token 是否存在
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let token_path = format!("{}/.kn/agent/device_token", home);
    let has_token = std::path::Path::new(&token_path).exists();

    if has_token {
        let token = std::fs::read_to_string(&token_path)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !token.is_empty() {
            let (tx, mut rx) = mpsc::unbounded_channel();
            // 正式连接
            // ws_client::connect("wss://api.shark.kim/v1/ws", &token, state.clone(), tx).await;
            eprintln!("[agent] device_token 已就绪，待 cloud 端点启用后连接");
        }
    } else {
        eprintln!("[agent] 未绑定设备，等待 Desktop 触发绑定");
    }
}
```

- [ ] **Step 4: 编译验证**

```bash
cd /Users/zhaojun/workspace/me/shark/kn && cargo check --bin kn-agent
```

- [ ] **Step 4.5: 协议版本检查**

在 ws_client.rs 的消息处理循环中，收到 `Connected` 消息后检查 `protocol_version`：

```rust
// 消息处理中:
if let ServerMessage::Connected { ws_session_id: _, protocol_version } = &msg {
    const SUPPORTED_VERSION: u32 = 1;
    match protocol_version {
        Some(v) if *v > SUPPORTED_VERSION => {
            eprintln!("[agent] 协议版本不兼容: server={}, client={}, 请升级", v, SUPPORTED_VERSION);
            return Err("protocol_version_mismatch".into());
        }
        _ => {
            eprintln!("[agent] 协议版本协商成功: version={:?}", protocol_version);
            state.transition(StateEvent::HandshakeDone)?;
        }
    }
}
```

**设备信息上报**：`agent_version`、`os_version`、`hostname` 不再作为单独的 WSS `agent_info` 消息发送。这些信息在 WSS 握手阶段通过 HTTP headers 传递：

- `X-KN-Role: kn-agent` — 角色标识（服务端用于消息权限白名单校验）
- `X-KN-Agent-Version` — Agent 版本号
- `X-KN-OS-Version` — macOS 版本
- `X-KN-Hostname` — 主机名

服务端 `connectAgent()` 从 headers 读取后直接写入 `kn_device` 表，无需额外的消息路由或 `DeviceInfoService` 参与（设备信息部分）。

`ClientMessage::AgentInfo` 变体已从 `proto.rs` 中移除。仅 `ProfileList` 消息保留，但其处理方式已变更：不再缓存到 Redis，而是由 `DeviceInfoService` 写入 MySQL `kn_device_profile` 表（delete+insert 覆盖），无 TTL 过期。

同样逻辑需在 iOS 的 `WebSocketClient` 中实现（连接后检查 `connected.protocol_version`，不兼容则提示用户升级 App）。

- [ ] **Step 5: Commit**

```bash
git add agent/src/ws_client.rs agent/src/main.rs
git commit -m "feat(agent): add WebSocket client with heartbeat, reconnect, protocol version check, and device info via WSS headers"
```

---

### Task 6: Session 管理（PTY 创建/销毁）

**Files:**
- Create: `agent/src/session.rs`

- [ ] **Step 1: 实现 SessionManager 骨架**

创建 `agent/src/session.rs`：

```rust
//! Session 管理 — PTY 创建/销毁、多 Session 并发
//!
//! Phase 1 仅实现基本结构，Phase 2 加入 InputMerger / OutputFan-out

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Session {
    pub id: String,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    // Phase 2: PTY handle
}

pub struct SessionManager {
    sessions: Mutex<HashMap<String, Session>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub async fn create(&self, id: String, tool: String, profile: Option<String>, cwd: String, cols: u16, rows: u16) {
        let session = Session {
            id: id.clone(),
            tool,
            profile,
            cwd,
            cols,
            rows,
            created_at: chrono::Utc::now(),
        };
        self.sessions.lock().await.insert(id, session);
    }

    pub async fn remove(&self, id: &str) {
        self.sessions.lock().await.remove(id);
    }

    pub async fn list(&self) -> Vec<String> {
        self.sessions.lock().await.keys().cloned().collect()
    }

    pub async fn count(&self) -> usize {
        self.sessions.lock().await.len()
    }

    pub async fn get(&self, id: &str) -> Option<Session> {
        // 返回 clone（Session 轻量，Phase 2 改引用）
        self.sessions.lock().await.get(id).map(|s| Session {
            id: s.id.clone(),
            tool: s.tool.clone(),
            profile: s.profile.clone(),
            cwd: s.cwd.clone(),
            cols: s.cols,
            rows: s.rows,
            created_at: s.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let mgr = SessionManager::new();
        assert_eq!(mgr.count().await, 0);

        mgr.create("s1".into(), "claude".into(), Some("deepseek".into()), "/tmp".into(), 80, 24).await;
        assert_eq!(mgr.count().await, 1);

        let s = mgr.get("s1").await.unwrap();
        assert_eq!(s.tool, "claude");
        assert_eq!(s.cols, 80);

        mgr.remove("s1").await;
        assert_eq!(mgr.count().await, 0);
    }
}
```

- [ ] **Step 2: 运行测试**

```bash
cd desktop && cargo test --bin kn-agent session::tests
```

- [ ] **Step 3: Commit**

```bash
git add agent/src/session.rs
git commit -m "feat(agent): add session manager (create/list/remove)"
```

---

### Task 7: launchd 安装/卸载

**Files:**
- Create: `agent/src/launchd.rs`

- [ ] **Step 1: 实现 launchd plist 管理**

创建 `agent/src/launchd.rs`：

```rust
//! launchd plist 管理 — 安装、卸载、检查状态
//!
//! plist 路径: ~/Library/LaunchAgents/com.kn.agent.plist

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// 生成 plist 内容
fn plist_content(agent_path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kn.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{agent_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>5</integer>
    <key>StandardOutPath</key>
    <string>{home}/.kn/agent/agent.log</string>
    <key>StandardErrorPath</key>
    <string>{home}/.kn/agent/agent_error.log</string>
</dict>
</plist>"#,
        agent_path = agent_path,
        home = home,
    )
}

fn plist_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!(
        "{}/Library/LaunchAgents/com.kn.agent.plist",
        home
    ))
}

/// 安装 launchd plist 并加载
pub fn install(agent_path: &str) -> Result<(), String> {
    // 确保目录存在
    let agent_dir = PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
    ).join(".kn/agent");
    fs::create_dir_all(&agent_dir)
        .map_err(|e| format!("创建 agent 目录失败: {}", e))?;

    // 写入 plist
    let plist = plist_path();
    fs::write(&plist, plist_content(agent_path))
        .map_err(|e| format!("写入 plist 失败: {}", e))?;

    // 加载到 launchd
    let output = Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&plist)
        .output()
        .map_err(|e| format!("launchctl load 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl load 失败: {}", stderr));
    }

    Ok(())
}

/// 卸载 launchd plist
pub fn uninstall() -> Result<(), String> {
    let plist = plist_path();
    if plist.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", "-w"])
            .arg(&plist)
            .output();
        fs::remove_file(&plist)
            .map_err(|e| format!("删除 plist 失败: {}", e))?;
    }
    Ok(())
}

/// 检查 Agent 是否在运行
pub fn is_running() -> bool {
    Command::new("launchctl")
        .args(["list"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.contains("com.kn.agent"))
        })
        .unwrap_or(false)
}

/// 加载 launchd plist（不卸载旧配置，仅加载）
pub fn load() -> Result<(), String> {
    let plist = plist_path();
    let output = Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&plist)
        .output()
        .map_err(|e| format!("launchctl load 失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl load 失败: {}", stderr));
    }
    Ok(())
}

/// 重启 Agent：卸载 → 加载（用于版本升级后让新二进制生效）
pub fn restart() -> Result<(), String> {
    let plist = plist_path();
    // 卸载
    if plist.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", "-w"])
            .arg(&plist)
            .output();
        // 短暂等待进程退出
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    // 加载
    let output = Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&plist)
        .output()
        .map_err(|e| format!("launchctl load 失败: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl load 失败: {}", stderr));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plist_content_contains_agent_path() {
        let content = plist_content("/Users/test/.kn/agent/kn-agent");
        assert!(content.contains("com.kn.agent"));
        assert!(content.contains("/Users/test/.kn/agent/kn-agent"));
        assert!(content.contains("KeepAlive"));
    }
}
```

- [ ] **Step 2: 在 main.rs 中声明模块**

在 `agent/src/main.rs` 中添加 `mod launchd;`（与其他 `mod` 声明并列）。

- [ ] **Step 3: 运行测试**

```bash
cd desktop && cargo test --bin kn-agent launchd::tests
```

- [ ] **Step 3.5: 日志每日翻转 + 7 天保留**

在 `main.rs` 启动时设置 `tracing-appender` rolling file appender，替代 launchd 的静态日志文件：

```rust
// main.rs
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;

let file_appender = RollingFileAppender::new(
    Rotation::DAILY,
    agent_dir.join("logs"),
    "agent",
);
// 保留 7 天: tracing-appender 默认不自动清理，加清理逻辑
let log_dir = agent_dir.join("logs");
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
        // 删除 7 天前的 agent.*.log 文件
        if let Ok(entries) = std::fs::read_dir(&log_dir) {
            let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with("agent.") {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if modified < cutoff.into() {
                                let _ = std::fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }
        }
    }
});
```

`agent/Cargo.toml` 新增依赖：
```toml
tracing = "0.1"
tracing-subscriber = "0.3"
tracing-appender = "0.2"
```

- [ ] **Step 4: Commit**

```bash
git add agent/src/launchd.rs agent/src/main.rs
git commit -m "feat(agent): add launchd plist install/uninstall/status with rolling log"
```

---

---

### Task 6.5: crash_count 文件持久化 + agent 目录初始化

**Files:**
- Modify: `agent/src/state.rs`
- Modify: `agent/src/main.rs`

- [ ] **Step 1: 增加 crash_count 文件读写**

在 `state.rs` 的 `StateMachine` 中增加持久化方法：

```rust
use std::path::PathBuf;

impl StateMachine {
    fn crash_count_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(format!("{}/.kn/agent/crash_count", home))
    }

    /// 启动时从磁盘读取 crash_count
    pub fn load_crash_count() -> u32 {
        let path = Self::crash_count_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// 每次启动后 +1，写入磁盘（tmp + rename 原子写，防 crash 时写一半）
    pub fn persist_crash_count(count: u32) {
        let path = Self::crash_count_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("tmp");
        let _ = std::fs::write(&tmp, count.to_string());
        let _ = std::fs::rename(&tmp, &path);
    }

    /// crash_count 重置为 0
    pub fn clear_crash_count() {
        let path = Self::crash_count_path();
        let _ = std::fs::remove_file(&path);
    }
}
```

- [ ] **Step 2: main.rs 启动时加载 + 运行 60s 后重置**

```rust
// main.rs 启动逻辑中:
let saved_count = StateMachine::load_crash_count();
let state = Arc::new(StateMachine::new());
state.crash_count.store(saved_count, Ordering::SeqCst);

// 启动时 +1 并写入
let new_count = state.increment_crash();
StateMachine::persist_crash_count(new_count);

if state.in_safe_mode() {
    eprintln!("[agent] SAFE MODE: crash_count={}, 仅维持 WSS + 状态查询", new_count);
    // 进入 safe_mode: 不创建 session，仅心跳 + IPC 查询
}

// 运行 60s 后重置 crash_count（说明启动成功）
let state_clone = state.clone();
tokio::spawn(async move {
    tokio::time::sleep(Duration::from_secs(60)).await;
    state_clone.reset_crash();
    StateMachine::clear_crash_count();
    eprintln!("[agent] 正常运行 60s，crash_count 已重置");
});
```

- [ ] **Step 3: 目录初始化**

在 main.rs 启动时确保 `~/.kn/agent/` 目录存在：

```rust
fn ensure_agent_dirs() -> std::io::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let dirs = vec![
        format!("{}/.kn/agent", home),
        format!("{}/.kn/agent/sessions", home),
    ];
    for d in &dirs {
        std::fs::create_dir_all(d)?;
    }
    Ok(())
}
```

- [ ] **Step 4: 测试**

```bash
cd /Users/zhaojun/workspace/me/shark/kn && cargo test --bin kn-agent state::tests
# 手动测试 crash_count 持久化:
ls -la ~/.kn/agent/crash_count  # Agent 启动后应存在
cat ~/.kn/agent/crash_count      # 应为数字
```

- [ ] **Step 5: Commit**

```bash
git add agent/src/state.rs agent/src/main.rs
git commit -m "feat(agent): crash_count file persistence and agent dir init"
```

---

## Phase 1 完成检查点

Phase 1 完成后，Agent 具备以下能力：
- [x] 采集设备指纹 (IOPlatformUUID)
- [x] 状态机运转 (9 状态 + crash 退避)
- [x] 消息协议序列化/反序列化
- [x] WSS 连接 + 心跳 + 重连
- [x] Session CRUD
- [x] launchd 安装/卸载/检测

**尚未实现（Phase 2 做）**：
- [ ] IPC server (Unix Socket) — Desktop 通信
- [ ] InputMerger + OutputFan-out — 真正的 PTY 读写
- [ ] Shell hook — `ai()` 路由改造
- [ ] 设备绑定完整流程 — `/bind-init` HTTP 调用 + QR 码生成 + HTTP `/bind-result` 短轮询
- [ ] profile_list 上报 + start_session 响应
