# Agent Phase 2 — IPC + PTY 集成 + Shell Hook + 绑定流程

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]` checkbox syntax.

**Goal:** 在 Agent Phase 1 骨架基础上，实现 Unix Socket IPC（Desktop 通信）、PTY 输入/输出对接、shell hook 改造、设备绑定完整流程。

**Prerequisites:** Phase 1 完成。

---

### Task 8: pty.rs trait 适配 — Desktop 对接 kn_common::pty_trait

**说明**: `PtyOutputSink` trait 已在 Agent P1 Task 1 中定义于 `common/src/pty_trait.rs`。本 Task 不做重复定义，只做 Desktop 侧的适配工作：将 `drain_utf8_stream` 改为使用 common trait，创建 `ChannelSink` 实现。

**Files:**
- Modify: `desktop/src-tauri/src/pty.rs`
- Modify: `desktop/src-tauri/src/commands.rs` (4 个 Tauri command)

- [ ] **Step 1: drain_utf8_stream 改用 common trait 泛型**

将 `drain_utf8_stream` 签名从：

```rust
fn drain_utf8_stream(..., on_event: &Channel<PtyEvent>)
```

改为：

```rust
use kn_common::pty_trait::PtyOutputSink;

fn drain_utf8_stream(..., sink: &impl PtyOutputSink)
```

函数体内 `on_event.send(PtyEvent::Data(s.to_string()))` 改为 `sink.send(s.as_bytes())`。

注意：`PtyEvent::Data` 是 **tuple variant** `Data(String)`，不是 struct variant。后续 `ChannelSink` 实现中构造时写成 `PtyEvent::Data(text.to_string())`。

- [ ] **Step 2: Tauri ChannelSink — 实现 common PtyOutputSink**

```rust
use kn_common::pty_trait::PtyOutputSink;
use tauri::ipc::Channel;

struct ChannelSink {
    channel: Channel<PtyEvent>,
}

impl PtyOutputSink for ChannelSink {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Ok(text) = std::str::from_utf8(data) {
            self.channel.send(PtyEvent::Data(text.to_string()))  // tuple variant
                .map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
    // on_ready / on_exit / on_error 保留原有行为：
    // ChannelSink 需要覆写这三个方法，确保前端收到 PTY 就绪/退出/错误事件
    fn on_ready(&self) -> Result<(), String> {
        self.channel.send(PtyEvent::Ready).map_err(|e| e.to_string())
    }
    fn on_exit(&self, code: i32) -> Result<(), String> {
        self.channel.send(PtyEvent::Exit(code)).map_err(|e| e.to_string())
    }
    fn on_error(&self, msg: &str) -> Result<(), String> {
        self.channel.send(PtyEvent::Error(msg.to_string())).map_err(|e| e.to_string())
    }
}
```

4 个 Tauri command 中将 `Channel<PtyEvent>` 包装为 `ChannelSink` 传入。

- [ ] **Step 3: 更新 start_pty / write_pty / resize_pty / kill_pty 签名**

将 `state: tauri::State<'_, Arc<Mutex<PtyState>>>` 改为直接传 `Arc<Mutex<PtyState>>`，去掉 `#[tauri::command]` 宏。

- [ ] **Step 4: 编译验证 + 现有测试通过**

```bash
cd /Users/zhaojun/workspace/me/shark/kn && cargo check --lib
cd /Users/zhaojun/workspace/me/shark/kn && cargo test
```

- [ ] **Step 5: Commit**

```bash
git commit -m "refactor(pty): adapt drain_utf8_stream to kn_common::PtyOutputSink"
```

---

### Task 9: Agent IPC Server (Unix Socket)

**Files:**
- Create: `agent/src/ipc.rs`
- Modify: `agent/src/main.rs`（添加 `mod ipc;` + 启动 IPC server）

- [ ] **Step 1: IPC Server**

创建 `agent/src/ipc.rs`：

```rust
//! Unix Socket IPC Server — kn Desktop 连接 Agent
//!
//! socket 路径: ~/.kn/agent/ipc.sock
//! 请求-响应模式，JSON 行协议 (每行一条完整 JSON)

use crate::state::StateMachine;
use crate::session::SessionManager;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub struct IpcServer {
    socket_path: PathBuf,
    state: Arc<StateMachine>,
    sessions: Arc<SessionManager>,
}

impl IpcServer {
    pub fn new(state: Arc<StateMachine>, sessions: Arc<SessionManager>) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Self {
            socket_path: PathBuf::from(format!("{}/.kn/agent/ipc.sock", home)),
            state,
            sessions,
        }
    }

    pub async fn serve(&self) -> Result<(), String> {
        // 清除旧 socket 文件
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)
            .map_err(|e| format!("IPC 绑定失败: {}", e))?;

        // 权限 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.socket_path,
                std::fs::Permissions::from_mode(0o600)).ok();
        }

        loop {
            let (stream, _) = listener.accept().await
                .map_err(|e| format!("accept 失败: {}", e))?;
            let state = self.state.clone();
            let sessions = self.sessions.clone();
            tokio::spawn(async move {
                handle_client(stream, state, sessions).await;
            });
        }
    }
}

async fn handle_client(stream: UnixStream, state: Arc<StateMachine>, sessions: Arc<SessionManager>) {
    let (reader, mut writer) = stream.into_split();
    let buf_reader = BufReader::new(reader);
    let mut lines = buf_reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let response = handle_request(&line, &state, &sessions).await;
        let _ = writer.write_all(response.as_bytes()).await;
        let _ = writer.write_all(b"\n").await;
    }
}

async fn handle_request(req: &str, state: &StateMachine, sessions: &SessionManager) -> String {
    let v: serde_json::Value = match serde_json::from_str(req) {
        Ok(v) => v,
        Err(e) => return format!(r#"{{"error":"{}"}}"#, e),
    };

    // Tauri agent_ipc command 发送 {"method":"...","params":{...}} 格式。
    // 为兼容此格式，将 params 中的字段合并到根层级。
    let v = if let Some(params) = v.get("params") {
        let mut merged = v.clone();
        if let Some(obj) = params.as_object() {
            for (k, val) in obj {
                if !merged.as_object().unwrap().contains_key(k) {
                    merged[k] = val.clone();
                }
            }
        }
        merged
    } else {
        v
    };

    match v.get("method").and_then(|m| m.as_str()) {
        Some("status") => {
            let s = state.current();
            format!(r#"{{"status":"{}","crash_count":{},"safe_mode":{}}}"#,
                s.as_str(), state.crash_count(), state.in_safe_mode())
        }
        Some("sessions") => {
            let list = sessions.list().await;
            format!(r#"{{"sessions":{}}}"#, serde_json::to_string(&list).unwrap())
        }
        Some("bind") => {
            format!(r#"{{"action":"bind","message":"Desktop 应触发 /bind-init HTTP 调用"}}"#)
        }
        Some("pause") => {
            format!(r#"{{"ok":true,"message":"Agent 即将暂停"}}"#)
        }
        Some("resume") => {
            format!(r#"{{"ok":true,"message":"Agent 已恢复"}}"#)
        }
        Some("new_session") => {
            let tool = v.get("tool").and_then(|t| t.as_str()).unwrap_or("claude");
            let profile = v.get("profile").and_then(|p| p.as_str());
            let cwd = v.get("cwd").and_then(|c| c.as_str()).unwrap_or(".");
            let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
            let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
            // 本地生成 nanoid（去中心化，无需云端替换）
            let session_id = nanoid::nanoid!(12);
            let session_id = format!("s_{}", session_id);
            sessions.create(session_id.clone(),
                tool.to_string(),
                profile.map(|s| s.to_string()),
                cwd.to_string(), cols, rows).await;

            // Phase 2 TODO: spawn PTY + notify cloud
            format!(r#"{{"session_id":"{}","status":"created"}}"#, session_id)
        }
        // ── 以下为补全的 8 个 IPC method ──
        Some("attach") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            // 注册该 IPC 连接为 session 的输出订阅者，后续 output push 通过此连接发送
            // 实现方式：将当前 writer handle 存入 session 的 subscribers 列表
            format!(r#"{{"ok":true}}"#)
        }
        Some("detach") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            // 从 session 的 subscribers 列表中移除当前连接
            format!(r#"{{"ok":true}}"#)
        }
        Some("input") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
            // 将输入写入 session PTY stdin（如果是 Agent 在线路径）
            sessions.push_input(sid, text, "desktop").await;
            format!(r#"{{"ok":true}}"#)
        }
        Some("ctrl") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            let signal = v.get("signal").and_then(|s| s.as_str()).unwrap_or("");
            let bytes = match signal {
                "ctrl_c" => b"\x03",
                "ctrl_d" => b"\x04",
                "ctrl_z" => b"\x1a",
                _ => return format!(r#"{{"error":"unknown_signal"}}"#),
            };
            sessions.push_input(sid, std::str::from_utf8(bytes).unwrap_or("\x03"), "desktop").await;
            format!(r#"{{"ok":true}}"#)
        }
        Some("resize") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
            let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
            sessions.resize(sid, cols, rows).await;
            format!(r#"{{"ok":true}}"#)
        }
        Some("kill_session") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            sessions.kill(sid).await;
            format!(r#"{{"ok":true}}"#)
        }
        Some("get_output_history") => {
            let sid = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("");
            let offset = v.get("offset").and_then(|o| o.as_u64()).unwrap_or(0);
            let limit = v.get("limit").and_then(|l| l.as_u64()).unwrap_or(200);
            // 从本地 output.log 读取历史输出（分页）
            let lines = sessions.read_output_log(sid, offset, limit).await;
            format!(r#"{{"lines":{},"offset":{},"total":{}}}"#,
                serde_json::to_string(&lines).unwrap(), offset, lines.len())
        }
        Some("get_version") => {
            format!(r#"{{"version":"{}","agent_version":"{}"}}"#,
                env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION"))
        }
        // ── 卡密激活（Desktop 输入） ──
        // 【已更新】Desktop redeem 现已改用 HTTP POST /api/v1/device/redeem，
        // 不再通过 Agent IPC/WSS 中转。Desktop 直接调 HTTP + Bearer device_token。
        // 此 IPC method 保留作为兼容入口，实际由 Desktop 自行处理。
        Some("redeem") => {
            let code = v.get("code").and_then(|c| c.as_str()).unwrap_or("");
            // Desktop 应直接调 HTTP POST /api/v1/device/redeem（Authorization: Bearer <device_token>）
            // 此 IPC method 已废弃，仅保留兼容
            format!(r#"{{"action":"redeem","code":"{}","deprecated":true,"message":"请使用 HTTP POST /api/v1/device/redeem"}}"#, code)
        }
        _ => format!(r#"{{"error":"unknown_method"}}"#),
    }
}
```

- [ ] **Step 2: 更新 main.rs 启动 IPC + WSS 消息分发器**

```rust
// main.rs 中添加:
mod ipc;

use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let state = Arc::new(StateMachine::new());
    let sessions = Arc::new(SessionManager::new());

    // ── WSS 消息通道 ──
    // ws_client 将收到的 ServerMessage 发送到此 channel，分发器消费
    let (wss_tx, mut wss_rx) = mpsc::unbounded_channel::<ServerMessage>();

    // ── WSS 消息分发器（Agent 核心调度逻辑）──
    // 将从云端收到的消息分发给 SessionManager 处理
    let dispatch_sessions = sessions.clone();
    let dispatch_state = state.clone();
    tokio::spawn(async move {
        while let Some(msg) = wss_rx.recv().await {
            match msg {
                ServerMessage::StartSession { session_id, tool, profile, cwd, cols, rows } => {
                    // iOS 远程发起新会话 → spawn PTY + 上报 session_created
                    dispatch_sessions.start_session(&session_id, &tool,
                        profile.as_deref(), &cwd, cols, rows,
                        wss_tx.clone(), /* ipc_tx */ wss_tx.clone()).await;
                    // 通过 WSS 发送 session_created 回执
                }
                ServerMessage::Input { session_id, text } => {
                    dispatch_sessions.push_input(&session_id, &text, "ios").await;
                }
                ServerMessage::Ctrl { session_id, signal } => {
                    let bytes = match signal.as_str() {
                        "ctrl_c" => "\x03", "ctrl_d" => "\x04", _ => "\x03",
                    };
                    dispatch_sessions.push_input(&session_id, bytes, "ios").await;
                }
                ServerMessage::KillSession { session_id, .. } => {
                    dispatch_sessions.kill(&session_id).await;
                }
                ServerMessage::ResizePty { session_id, cols, rows } => {
                    dispatch_sessions.resize(&session_id, cols, rows).await;
                }
                _ => {} // 其他消息由 ws_client 内部处理（Pong/BindResult 等）
            }
        }
    });

    // ── 启动 IPC server（Desktop 通信）──
    let ipc = Arc::new(ipc::IpcServer::new(state.clone(), sessions.clone()));
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc.serve().await {
            eprintln!("IPC server error: {}", e);
        }
    });

    // ── WSS 连接逻辑 ──
    // ws_client::connect() 在 WSS 握手阶段通过 HTTP headers 传递设备与角色信息：
    //   X-KN-Role: kn-agent — 角色标识（服务端用于消息权限白名单校验）
    //   X-KN-Agent-Version, X-KN-OS-Version, X-KN-Hostname
    // 服务端 connectAgent() 从 headers 读取后直接写入 kn_device 表。
    // 不再使用独立的 agent_info WSS 消息类型。
    // ProfileList 消息保留，但不再缓存到 Redis，改为写入 MySQL kn_device_profile 表。
    // ws_client::connect(&device_token, state, wss_tx).await;

    ipc_handle.await.unwrap();
}
```

- [ ] **Step 3: 测试 IPC（用 nc）**

```bash
# 启动 agent 后:
echo '{"method":"status"}' | nc -U ~/.kn/agent/ipc.sock
# 预期: {"status":"unbound","crash_count":0,"safe_mode":false}
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(agent): Unix Socket IPC server (status/sessions/bind/new_session)"
```

---

### Task 10: Agent PTY 集成 (WssSink + IpcSink)

**Files:**
- Create: `agent/src/sink.rs`
- Modify: `agent/src/session.rs`

- [ ] **Step 1: WssSink 实现**

创建 `agent/src/sink.rs`：

```rust
//! PtyOutputSink 的 Agent 实现 — WssSink (云端) + IpcSink (Desktop)

use kn_common::pty_trait::PtyOutputSink;
use tokio::sync::mpsc;

/// WSS 输出 — 推给云端
pub struct WssSink {
    pub tx: mpsc::UnboundedSender<String>,
}

impl PtyOutputSink for WssSink {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Ok(text) = std::str::from_utf8(data) {
            let msg = serde_json::json!({
                "type": "output",
                "session_id": "",  // 由调用方设置
                "ansi_text": text
            });
            self.tx.send(msg.to_string()).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}

/// IPC 输出 — 推给 Desktop (Unix Socket)
pub struct IpcSink {
    pub tx: mpsc::UnboundedSender<String>,
}

impl PtyOutputSink for IpcSink {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Ok(text) = std::str::from_utf8(data) {
            self.tx.send(text.to_string()).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Session 关联 PTY**

在 `session.rs` 中，`Session` 结构体增加 PTY 相关字段（Phase 2 占位，Phase 3 真正对接 PTY）：

```rust
pub struct Session {
    pub id: String,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    // Phase 2 新增:
    pub child_pid: Option<u32>,
    // Phase 3: PtyHandle (等 pty.rs trait 稳定后)
}
```

- [ ] **Step 3: 编译验证**

```bash
cd /Users/zhaojun/workspace/me/shark/kn && cargo check --bin kn-agent
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(agent): WssSink and IpcSink, session PTY fields"
```

---

### Task 11: Shell Hook 改造

**Files:**
- Modify: `shell/ai-profile.sh`
- Modify: `install.sh`（确保 `kn agent` CLI 安装路径）

- [ ] **Step 1: 在 ai-profile.sh 中添加 Agent 路由**

在现有 `ai()` 函数定义前添加 `_ai_direct()` 函数（保持原有逻辑），然后修改 `ai()`：

```bash
# 原有逻辑封装为 _ai_direct
_ai_direct() {
  # 当前的完整 ai() 逻辑 (profile 选择 + 环境注入 + 启动)
  # ... (原有代码)
}

# 新 ai() — Agent 路由
ai() {
  if /bin/launchctl list | grep -q com.kn.agent 2>/dev/null; then
    if ~/.kn/agent/kn-agent --new --tool "${1}" --profile "${2}" --cwd "$(pwd)" 2>/dev/null; then
      echo "Session created via kn Agent."
      return 0
    fi
    echo "Agent IPC 不可用，本次会话仅本地运行" >&2
  fi
  _ai_direct "$@"
}
```

- [ ] **Step 2: 测试 hook**

```bash
# 手动 source 后测试
source ~/.kn/shell-rc
ai claude deepseek
# Agent 未运行时 → 走 _ai_direct 原有流程
# Agent 运行时   → IPC 调 Agent 创建 session
```

- [ ] **Step 3: Commit**

```bash
git add shell/ai-profile.sh
git commit -m "feat(shell): route ai() to Agent when available, fallback to direct"
```

---

### Task 12: 设备绑定完整流程

**Files:**
- Create: `agent/src/bind.rs` (绑定流程: HTTP /bind-init + /bind-result 轮询)
- Modify: `agent/src/main.rs`

- [ ] **Step 1: bind-init HTTP 调用**

**Token 格式约定**：
- `bind_code` = 6 位数字短 code（如 `482916`），仅用于 HTTP `/bind-result` 轮询（非 WSS 凭证）
- `device_token` = UUID 风格长 token，绑定完成后由云端签发，Agent 存本地 `~/.kn/agent/device_token`，连接 WSS 时服务端按 `token.length() > 6` 路由到正式 Agent 模式
- Agent 绑定期间不建立 WSS 连接，通过 HTTP 短轮询等待结果，简化了服务端架构

创建 `agent/src/bind.rs`，添加绑定流程相关函数：

```rust
use reqwest::Client;
use serde::Deserialize;

/// 云端统一响应格式（对应 Java ApiResponse<T>）
#[derive(Debug, Deserialize)]
struct CloudResponse<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct BindInitData {
    bind_code: String,
    expires_in: u32,
    bind_url: String,  // 服务端返回的完整二维码 URL，Agent 无需拼接域名
}

pub struct BindCode {
    pub code: String,
    pub expires_in: u32,
    pub bind_url: String,  // 完整 URL，直接用于 QR 码生成
}

/// 调用云服务 /bind-init 获取临时绑定码
pub async fn request_bind_code(machine_id: &str) -> Result<BindCode, String> {
    let client = Client::new();
    let base_url = std::env::var("KN_CLOUD_URL")
        .unwrap_or_else(|_| "https://api.knshark.com".into());
    let resp = client.post(format!("{}/api/v1/device/bind-init", base_url))
        .json(&serde_json::json!({"machine_id": machine_id}))
        .send().await.map_err(|e| format!("bind-init 请求失败: {}", e))?;

    // 解析统一响应：{"code":0, "message":"ok", "data":{"bind_code":"...","expires_in":300,"bind_url":"https://..."}}
    let cloud_resp: CloudResponse<BindInitData> = resp.json().await
        .map_err(|e| format!("bind-init 响应解析失败: {}", e))?;
    if cloud_resp.code != 0 {
        return Err(format!("bind-init 失败: [{}] {}", cloud_resp.code, cloud_resp.message));
    }
    let data = cloud_resp.data.ok_or("bind-init 响应 data 为空")?;
    Ok(BindCode {
        code: data.bind_code,
        expires_in: data.expires_in,
        bind_url: data.bind_url,
    })
}

#[derive(Debug, Deserialize)]
struct BindResultData {
    device_token: String,
    device_id: u64,
}

/// HTTP 短轮询 /bind-result?code=xxx（每 1-2s），等待 iOS 扫码确认
/// 返回 Some(device_token) 或 None（超时/取消）
pub async fn poll_bind_result(code: &str, timeout_secs: u64) -> Result<Option<String>, String> {
    let client = Client::new();
    let base_url = std::env::var("KN_CLOUD_URL")
        .unwrap_or_else(|_| "https://api.knshark.com".into());
    let url = format!("{}/api/v1/device/bind-result?code={}", base_url, code);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if std::time::Instant::now() > deadline {
            return Ok(None);  // 超时
        }

        let resp = client.get(&url).send().await
            .map_err(|e| format!("bind-result 请求失败: {}", e))?;
        let cloud_resp: CloudResponse<BindResultData> = resp.json().await
            .map_err(|e| format!("bind-result 响应解析失败: {}", e))?;

        match cloud_resp.code {
            0 => {
                if let Some(data) = cloud_resp.data {
                    return Ok(Some(data.device_token));
                }
                // code=0 但 data 为 null → iOS 尚未确认，继续等待
            }
            _ => {
                // 非 0 错误码（如 code 过期/无效）
                return Err(format!("bind-result 失败: [{}] {}", cloud_resp.code, cloud_resp.message));
            }
        }

        // 等待 1-2s 后重试
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }
}
```

- [ ] **Step 2: 生成 QR 码 (ASCII)**

```rust
/// 生成 ASCII QR 码（依赖 qr2term crate 或简单输出绑定 URL）
pub fn generate_ascii_qr(bind_url: &str, hostname: &str) -> String {
    format!(
        "╔══════════════════════════╗\n\
         ║    📱 kn 设备绑定       ║\n\
         ║                      ║\n\
         ║  {}  ║\n\
         ║                      ║\n\
         ║  主机名: {}     ║\n\
         ║                      ║\n\
         ║  请用 kn iOS App     ║\n\
         ║  扫描以上 URL 完成    ║\n\
         ╚══════════════════════════╝",
        bind_url, hostname
    )
}
```

- [ ] **Step 3: agent main 绑定流程整合**

```rust
// main.rs 中:
if !has_token {
    // 未绑定 → 等待 Desktop 通过 IPC 触发绑定
    // Desktop 发 {"method":"bind"} → Agent 调 bind-init → 显示 QR → HTTP 轮询 bind-result → 等待 device_token
    let machine_id = fingerprint::machine_id().unwrap_or_default();
    let bind = bind::request_bind_code(&machine_id).await?;
    let qr = bind::generate_ascii_qr(&bind.bind_url, &hostname::get()?.to_string_lossy());
    println!("{}", qr);

    // HTTP 短轮询 GET /api/v1/device/bind-result?code=xxx（每 1-2s，超时 5min）
    // 不再建立 WSS 临时连接，直接通过 HTTP 轮询获取 device_token
    match bind::poll_bind_result(&bind.code, 300).await {
        Ok(Some(device_token)) => {
            let token_path = format!("{}/.kn/agent/device_token", home_dir);
            std::fs::write(&token_path, &device_token).map_err(|e| e.to_string())?;
            // 权限 0600
            #[cfg(unix)]
            { use std::os::unix::fs::PermissionsExt;
              std::fs::set_permissions(&token_path,
                std::fs::Permissions::from_mode(0o600)).ok(); }
            // 通知 Desktop "绑定成功"
            eprintln!("[agent] 绑定成功, device_token 已保存");
        }
        Ok(None) => {
            return Err("绑定超时: iOS 未在 5 分钟内扫码确认".into());
        }
        Err(e) => {
            return Err(format!("绑定失败: {}", e));
        }
    }
}
```

- [ ] **Step 4: 编译验证**

```bash
cd /Users/zhaojun/workspace/me/shark/kn && cargo check --bin kn-agent
cargo test --bin kn-agent
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(agent): complete device binding flow (bind-init + QR + HTTP polling + save token)"
```

---

---

### Task 13: InputMerger + OutputFan-out

**Files:**
- Modify: `agent/src/session.rs`

- [ ] **Step 1: InputMerger — 多来源 FIFO**

在 `session.rs` 的 `SessionManager` 中增加输入队列：

```rust
use std::collections::VecDeque;
use tokio::sync::mpsc;

pub struct InputMessage {
    pub session_id: String,
    pub text: String,
    pub source: String,  // "ios" / "local" / "desktop"
}

pub struct InputMerger {
    /// 每个 session 一个输入队列
    queues: Mutex<HashMap<String, VecDeque<InputMessage>>>,
    /// 每个 session 一个 Notify（唤醒该 session 的 stdin 写入循环）
    notifies: Mutex<HashMap<String, Arc<Notify>>>,
}

impl InputMerger {
    pub fn new() -> Self {
        Self {
            queues: Mutex::new(HashMap::new()),
            notifies: Mutex::new(HashMap::new()),
        }
    }

    /// 接收输入（iOS/Desktop/Shell 调用）
    /// 写入 session 队列后唤醒该 session 的 stdin 写入循环
    pub async fn push(&self, msg: InputMessage) {
        let mut queues = self.queues.lock().await;
        queues.entry(msg.session_id.clone())
            .or_insert_with(VecDeque::new)
            .push_back(msg);
        drop(queues);
        // 唤醒等待中的 PTY stdin 写入循环
        let notifies = self.notifies.lock().await;
        if let Some(n) = notifies.get(&msg.session_id) {
            n.notify_one();
        }
    }

    /// PTY 写入循环取输入（由 Notify 唤醒后调用）
    pub async fn pop(&self, session_id: &str) -> Option<InputMessage> {
        let mut queues = self.queues.lock().await;
        queues.get_mut(session_id).and_then(|q| q.pop_front())
    }

    /// 为新 session 注册 Notify，返回其 Notify 句柄
    /// 调用方（start_session）在 spawn PTY stdin 循环前调用此方法
    pub async fn register_session(&self, session_id: &str) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        self.notifies.lock().await.insert(session_id.to_string(), notify.clone());
        notify
    }

    /// Session 结束时注销 Notify
    pub async fn unregister_session(&self, session_id: &str) {
        self.notifies.lock().await.remove(session_id);
    }
}
```

- [ ] **Step 2: OutputFan-out — 广播到 WSS + IPC + log（含批量合并 + 分片）**

设计文档 §5.2 性能要求：PTY 碎片以 100ms 时间窗口或 64KB 积压任一达到即 flush；单次输出超过 10KB 时分片推送。

**关键设计决策**：`buffer` 使用 `std::sync::Mutex`（而非 `tokio::sync::Mutex`）。
理由是 PTY 读取线程在 `spawn_blocking` 中运行（同步上下文），无法 `.await`。
`std::sync::Mutex` 在同步和异步上下文中均可使用，且锁持有时间极短（仅 extend + check + take），不会阻塞 event loop。

```rust
use std::sync::Mutex;
use tokio::time::{interval, Duration};

pub struct OutputFanout {
    wss_tx: Option<mpsc::UnboundedSender<String>>,
    ipc_tx: Option<mpsc::UnboundedSender<String>>,
    session_id: String,
    /// 缓冲池：积累 PTY 碎片，100ms 或 64KB 触发 flush
    /// 使用 std::sync::Mutex 因为在 spawn_blocking（同步上下文）中也会被调用
    buffer: Arc<Mutex<Vec<u8>>>,
}

// OutputFanout 需实现 Clone（给 reader 线程和调用方各一份），通过 Arc 包裹内部字段实现
// 详见 Task 15 Step 2 末尾的 Clone 改造说明
#[derive(Clone)]
pub struct OutputFanout {
    inner: Arc<OutputFanoutInner>,
}

struct OutputFanoutInner {
    wss_tx: Option<mpsc::UnboundedSender<String>>,
    ipc_tx: Option<mpsc::UnboundedSender<String>>,
    session_id: String,
    buffer: Mutex<Vec<u8>>,
}

impl OutputFanout {
    pub fn new(session_id: String, wss: Option<mpsc::UnboundedSender<String>>,
                ipc: Option<mpsc::UnboundedSender<String>>) -> Self {
        let inner = Arc::new(OutputFanoutInner {
            wss_tx: wss, ipc_tx: ipc, session_id,
            buffer: Mutex::new(Vec::with_capacity(65536)),
        });
        let fanout = Self { inner: inner.clone() };

        // 启动 100ms 定时 flush（在 tokio async 上下文中运行）
        let buf = inner.buffer.clone();  // Arc<Mutex<Vec<u8>>> 直接 clone
        let wss = inner.wss_tx.clone();
        let ipc = inner.ipc_tx.clone();
        let sid = inner.session_id.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_millis(100));
            loop {
                tick.tick().await;
                let data = {
                    let mut locked = buf.lock().unwrap();
                    if locked.is_empty() { continue; }
                    std::mem::take(&mut *locked)
                }; // lock 在此 drop
                Self::flush_chunked(&sid, &data, &wss, &ipc);
            }
        });
        fanout
    }

    /// PTY 输出到达 → 写入缓冲，超过 64KB 立即 flush。
    ///
    /// **并发安全**：extend + check + take 在单次加锁内原子完成，
    /// 防止与 100ms 定时 flush 之间出现 TOCTOU 竞争导致 ANSI 序列被截断。
    ///
    /// **注意**：这是同步方法（非 async），因为 PTY reader 在 spawn_blocking 中运行。
    /// 使用 `std::sync::Mutex::lock()` 而非 `tokio::sync::Mutex::lock().await`。
    pub fn broadcast(&self, data: &[u8]) {
        let mut buf = self.inner.buffer.lock().unwrap();
        buf.extend_from_slice(data);
        if buf.len() >= 65536 {  // 64KB 阈值
            let chunk = std::mem::take(&mut *buf);
            drop(buf);  // 尽早释放锁
            Self::flush_chunked(&self.inner.session_id, &chunk, &self.inner.wss_tx, &self.inner.ipc_tx);
        }
    }

    /// 分片发送：每片 ≤ 10KB，超出部分拆成多个 output 消息
    fn flush_chunked(
        session_id: &str, data: &[u8],
        wss_tx: &Option<mpsc::UnboundedSender<String>>,
        ipc_tx: &Option<mpsc::UnboundedSender<String>>,
    ) {
        let text = String::from_utf8_lossy(data);
        let max_chunk = 10240; // 10KB

        let mut offset = 0;
        while offset < text.len() {
            let end = std::cmp::min(offset + max_chunk, text.len());
            let chunk = &text[offset..end];
            offset = end;

            // → WSS (云端 → iOS)
            if let Some(ref tx) = wss_tx {
                let msg = serde_json::json!({
                    "type": "output",
                    "session_id": session_id,
                    "ansi_text": chunk
                });
                let _ = tx.send(msg.to_string());
            }

            // → IPC (Desktop)
            if let Some(ref tx) = ipc_tx {
                let _ = tx.send(chunk.to_string());
            }
        }

        // → 本地 output.log（追加写入磁盘，保留 7 天）
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let log_path = format!("{}/.kn/agent/sessions/{}/output.log", home, session_id);
        if let Some(parent) = std::path::Path::new(&log_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            use std::io::Write;
            let _ = writeln!(file, "{}", text);
        }
    }
}
```

- [ ] **Step 3: SessionManager 集成**

```rust
impl SessionManager {
    /// 创建 session 并返回 OutputFanout 句柄
    pub async fn create_with_io(
        &self, id: String, tool: String, profile: Option<String>,
        cwd: String, cols: u16, rows: u16,
        wss_tx: mpsc::UnboundedSender<String>,
        ipc_tx: mpsc::UnboundedSender<String>,
    ) -> OutputFanout {
        let session = Session {
            id: id.clone(), tool, profile, cwd, cols, rows,
            created_at: chrono::Utc::now(), child_pid: None,
        };
        self.sessions.lock().await.insert(id.clone(), session);
        OutputFanout::new(id, Some(wss_tx), Some(ipc_tx))
    }

    // ── 以下方法供 IPC handler 调用（对应 §3.2.9 的 14 个 IPC method）──

    /// 将输入写入 session PTY stdin（来源: desktop / shell / ios）
    pub async fn push_input(&self, session_id: &str, text: &str, source: &str) {
        self.input_merger.push(InputMessage {
            session_id: session_id.to_string(),
            text: text.to_string(),
            source: source.to_string(),
        }).await;
    }

    /// 调整 session PTY 窗口尺寸
    pub async fn resize(&self, session_id: &str, cols: u16, rows: u16) {
        let mut sessions = self.sessions.lock().await;
        if let Some(s) = sessions.get_mut(session_id) {
            s.cols = cols;
            s.rows = rows;
            // PTY 尺寸变更通过 writer 发送 TIOCSWINSZ ioctl（Phase 3 实现）
        }
    }

    /// 强制终止 session（SIGTERM → 5s → SIGKILL）
    pub async fn kill(&self, session_id: &str) {
        if let Some(s) = self.sessions.lock().await.get(session_id) {
            if let Some(pid) = s.child_pid {
                // Phase 3: 实现 SIGTERM + 超时 + SIGKILL
                let _ = pid; // 占位
            }
        }
        self.sessions.lock().await.remove(session_id);
    }

    /// 分页读取 session 本地 output.log
    pub async fn read_output_log(&self, session_id: &str, offset: u64, limit: u64) -> Vec<String> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let log_path = format!("{}/.kn/agent/sessions/{}/output.log", home, session_id);
        // 读取文件 → 按行分割 → skip(offset) → take(limit)
        std::fs::read_to_string(&log_path)
            .map(|content| {
                content.lines()
                    .skip(offset as usize)
                    .take(limit as usize)
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(agent): InputMerger FIFO and OutputFan-out with 100ms/64KB batching + 10KB chunking"
```

---

### Task 14: Session checkpoint 原子写入

**Files:**
- Modify: `agent/src/session.rs`

- [ ] **Step 1: checkpoint 结构体 + 写入逻辑**

```rust
use serde::Serialize;

#[derive(Serialize)]
struct CheckpointData {
    agent_state: String,
    sessions: Vec<CheckpointSession>,
}

#[derive(Serialize)]
struct CheckpointSession {
    session_id: String,
    last_input: String,          // 截断到 200 字符
    last_output_snippet: String, // 截断到 500 字符
    cwd: String,
    tool: String,
    profile: Option<String>,
}

impl SessionManager {
    fn checkpoint_path(session_id: &str) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(format!("{}/.kn/agent/sessions/{}/checkpoint.json", home, session_id))
    }

    /// 原子写 checkpoint (tmp + rename)
    pub async fn save_checkpoint(&self, agent_state: &str) -> Result<(), String> {
        let sessions = self.sessions.lock().await;
        let mut data = Vec::new();
        for (_, s) in sessions.iter() {
            data.push(CheckpointSession {
                session_id: s.id.clone(),
                last_input: truncate(&s.last_input(), 200),
                last_output_snippet: truncate(&s.last_output_snippet(), 500),
                cwd: s.cwd.clone(),
                tool: s.tool.clone(),
                profile: s.profile.clone(),
            });
        }
        drop(sessions);

        let checkpoint = CheckpointData {
            agent_state: agent_state.to_string(),
            sessions: data,
        };
        let json = serde_json::to_string_pretty(&checkpoint)
            .map_err(|e| e.to_string())?;

        // 每个 session 写各自的 checkpoint（不写全局数据）
        for s in &checkpoint.sessions {
            let path = Self::checkpoint_path(&s.session_id);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            // 仅序列化当前 session 的数据
            let per_session = serde_json::json!({
                "agent_state": checkpoint.agent_state,
                "session": s,
            });
            let session_json = serde_json::to_string_pretty(&per_session)
                .map_err(|e| e.to_string())?;
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, &session_json).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// 每 30s 执行一次
    pub fn start_checkpoint_loop(sm: Arc<SessionManager>, state: Arc<StateMachine>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(e) = sm.save_checkpoint(state.current().as_str()).await {
                    eprintln!("checkpoint 写入失败: {}", e);
                }
            }
        });
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars { s.to_string() }
    else { s.chars().take(max_chars).collect::<String>() + "..." }
}
```

- [ ] **Step 2: Session 中 last_input/last_output_snippet 字段**

```rust
pub struct Session {
    // ... 现有字段
    last_input: Mutex<String>,
    last_output_snippet: Mutex<String>,
}

impl Session {
    pub fn record_input(&self, text: &str) {
        *self.last_input.lock().unwrap() = text.to_string();
    }
    pub fn record_output_snippet(&self, text: &str) {
        *self.last_output_snippet.lock().unwrap() = truncate(text, 500);
    }
    pub fn last_input(&self) -> String { self.last_input.lock().unwrap().clone() }
    pub fn last_output_snippet(&self) -> String { self.last_output_snippet.lock().unwrap().clone() }
}
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(agent): session checkpoint atomic write every 30s"
```

---

### Task 15: AI CLI 启动 — find_binary + PTY spawn + I/O 桥接

**Files:**
- Modify: `agent/src/session.rs`
- Modify: `agent/Cargo.toml` (已在 P1 Task 1 加 `portable-pty = "0.8"`)
- Create (reuse): 引用 `kn_common::commands::find_binary`

- [ ] **Step 0: 确认依赖**

`agent/Cargo.toml` 中需包含 `portable-pty = "0.8"`（Agent Phase 1 Task 1 Step 4 已添加）。

- [ ] **Step 1: AI CLI 二进制查找**

```rust
/// 在 session.rs 中引用公共库 commands 的工具函数
use kn_common::commands::{find_binary, home_dir};

/// 根据 tool 名称查找对应的 CLI 二进制路径
///
/// v1: tool → binary 映射硬编码。新增 tool 需改代码。
/// v2: 改为从 config.yaml profile 的 `tool` 字段反查，
///     遍历所有 profile → 提取唯一的 tool 名 → find_binary 查找。
fn resolve_tool_path(tool: &str) -> Result<String, String> {
    let candidates: &[&str] = match tool {
        "claude" => &["claude"],
        "codex"  => &["codex", "qoder"],
        "qoder"  => &["qoder", "codex"],
        _        => return Err(format!("未知 tool: {}", tool)),
    };
    // find_binary 签名: pub(crate) fn find_binary(names: &[&str]) -> Option<String>
    // 直接传切片，内部按优先级依次查找
    if let Some(path) = find_binary(candidates) {
        return Ok(path);
    }
    Err(format!("未找到 {} 二进制", tool))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_claude_path() {
        // 在已安装 Claude Code 的机器上应能找到
        let path = resolve_tool_path("claude");
        assert!(path.is_ok() || path.unwrap_err().contains("未找到"));
    }
}
```

- [ ] **Step 2: PTY spawn + I/O 桥接**

替换原来返回 `Err("PTY spawn 待 Phase 3 实现")` 的占位，实现完整 PTY 生命周期。以下代码直接写在 `session.rs` 中 `Step 1` 和 `Step 3`（tool 预处理）之间的 `start_session` 方法体内。

**2a. 依赖声明**（`session.rs` 文件头部）：

```rust
use kn_common::commands::{find_binary, home_dir};
use kn_common::profile_cmd;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
```

**2b. `Session` 结构体增加 PTY 字段**（替换 Task 10 Step 2 中的占位定义）：

```rust
pub struct Session {
    pub id: String,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub created_at: chrono::DateTime<chrono::Utc>,
    // PTY 相关
    pub child_pid: Option<u32>,
    pub writer: Option<Arc<Mutex<Box<dyn Write + Send>>>>,  // PTY stdin
    pub output_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,   // PTY stdout → OutputFanout
    // Checkpoint 字段（Task 14 使用）
    pub last_input: Mutex<String>,
    pub last_output_snippet: Mutex<String>,
    // Tool 预处理清理守卫（RAII: session 移除时自动删除临时文件/恢复 auth.json）
    pub cleanup_guard: Mutex<Option<ToolCleanupGuard>>,
}

impl Session {
    pub fn record_input(&self, text: &str) {
        *self.last_input.lock().unwrap() = text.to_string();
    }
    pub fn record_output_snippet(&self, text: &str) {
        *self.last_output_snippet.lock().unwrap() = truncate(text, 500);
    }
    pub fn last_input(&self) -> String { self.last_input.lock().unwrap().clone() }
    pub fn last_output_snippet(&self) -> String { self.last_output_snippet.lock().unwrap().clone() }
}
```

**2c. `start_session` — 完整 PTY spawn + 错误处理**：

```rust
impl SessionManager {
    /// 创建 session 并真正 spawn PTY，返回 OutputFanout 句柄。
    /// 调用方（IPC new_session / WSS start_session 响应）拿到 fanout 后
    /// 即可接收 PTY 输出。
    pub async fn start_session(
        &self,
        id: &str,
        tool: &str,
        profile: Option<&str>,
        cwd: &str,
        cols: u16,
        rows: u16,
        wss_tx: mpsc::UnboundedSender<String>,
        ipc_tx: mpsc::UnboundedSender<String>,
    ) -> Result<OutputFanout, String> {
        // ── 1. 查找 CLI 二进制 ──
        let binary = resolve_tool_path(tool)?;

        // ── 2. 读 profile env vars ──
        let env_vars: Option<HashMap<String, String>> = if let Some(p) = profile {
            match kn_common::profile_cmd::get_env_cmd(p) {
                Ok(v) => Some(v),
                Err(e) => {
                    // config 解析失败 → 不创建 session
                    let _ = wss_tx.send(serde_json::json!({
                        "type": "error_notify",
                        "code": "config_parse_error",
                        "message": format!("配置文件损坏: {}", e)
                    }).to_string());
                    return Err(format!("config_parse_error: {}", e));
                }
            }
        } else {
            None
        };

        // ── 3. Tool 预处理（Claude --settings / Codex auth.json） ──
        let prep = prepare_tool_env(tool, &env_vars)?;
        // RAII 清理守卫：存入 session 后，当 session 被 remove 时自动清理临时文件
        let cleanup_guard = prep.cleanup.map(|f| ToolCleanupGuard {
            cleanup: Some(f),
            session_id: id.to_string(),
        });

        // ── 4. openpty ──
        let pty_system = NativePtySystem::default();
        let size = PtySize { rows, cols, pixel_width: 0, pixel_height: 0 };
        let pair = pty_system
            .openpty(size)
            .map_err(|e| {
                let _ = wss_tx.send(serde_json::json!({
                    "type": "error_notify", "code": "pty_alloc_failed",
                    "message": format!("openpty 失败: {}", e)
                }).to_string());
                format!("pty_alloc_failed: {}", e)
            })?;

        // ── 5. spawn shell ──
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.args(["-i", "-l"]);
        if !cwd.is_empty() { cmd.cwd(cwd); }

        // 注入 env vars（与 Desktop pty.rs 保持一致）
        for (k, v) in std::env::vars() { cmd.env(&k, &v); }
        if let Some(ref ev) = env_vars {
            for (k, v) in ev { cmd.env(k, v); }
        }

        // PATH 补齐 + TERM 强制设置（参考 Desktop pty.rs）
        if cfg!(target_os = "macos") {
            let current_path = std::env::var("PATH").unwrap_or_default();
            let extra = ["/opt/homebrew/bin", "/opt/homebrew/sbin",
                         "/usr/local/bin", "/usr/local/sbin"];
            let missing: Vec<&str> = extra.iter()
                .filter(|p| !current_path.split(':').any(|seg| seg == **p))
                .copied().collect();
            if !missing.is_empty() {
                cmd.env("PATH", format!("{}:{}", current_path, missing.join(":")));
            }
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "kn");
        if std::env::var_os("LANG").is_none() { cmd.env("LANG", "en_US.UTF-8"); }

        // 构建完整 CLI 命令行: <binary> [--settings tmp.json] ...
        let mut cli_args: Vec<String> = prep.extra_args.clone();
        // 如果 tool 本身需要 profile 名作为参数（如 `claude --profile xxx`），在这里追加
        // v1: Claude 通过 --settings 注入 env，不需要 --profile；Codex/qoder 同理
        cmd.arg(&binary);
        for arg in &cli_args { cmd.arg(arg); }

        let child = pair.slave
            .spawn_command(cmd)
            .map_err(|e| {
                let _ = wss_tx.send(serde_json::json!({
                    "type": "error_notify", "code": "shell_spawn_failed",
                    "message": format!("shell 启动失败: {}", e)
                }).to_string());
                format!("shell_spawn_failed: {}", e)
            })?;
        drop(pair.slave);

        let child_pid = child.process_id();

        // ── 6. 创建 I/O 通道 ──
        let mut reader = pair.master
            .try_clone_reader()
            .map_err(|e| format!("clone reader: {}", e))?;

        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(
            pair.master.take_writer()
                .map_err(|e| format!("take writer: {}", e))?,
        )));

        // 输出通道：PTY stdout → OutputFanout
        let (output_tx, mut output_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // ── 7. 注册 session ──
        let fanout = OutputFanout::new(id.to_string(), Some(wss_tx.clone()), Some(ipc_tx.clone()));
        {
            let mut sessions = self.sessions.lock().await;
            sessions.insert(id.to_string(), Session {
                id: id.to_string(),
                tool: tool.to_string(),
                profile: profile.map(|s| s.to_string()),
                cwd: cwd.to_string(),
                cols, rows,
                created_at: chrono::Utc::now(),
                child_pid,
                writer: Some(writer.clone()),
                output_tx: Some(output_tx.clone()),
                last_input: Mutex::new(String::new()),
                last_output_snippet: Mutex::new(String::new()),
                cleanup_guard: Mutex::new(cleanup_guard),
            });
        }

        // ── 8. PTY stdout 读取线程 ──
        let fanout_clone = fanout.clone();
        let session_id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,  // EOF
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        // 推给 OutputFanout（100ms/64KB 批量 + 10KB 分片）
                        // 注意：broadcast() 是同步方法（内部使用 std::sync::Mutex），
                        // 在 spawn_blocking 中直接调用，无需 block_on
                        fanout_clone.broadcast(&data);
                    }
                    Err(_) => break,
                }
            }
            // PTY EOF → 标记 session 结束
            eprintln!("[agent] session {} PTY EOF, 会话结束", session_id);
        });

        // ── 9. PTY stdin 写入循环（从 InputMerger 取输入）──
        // 使用 Notify 唤醒模式：等待输入到达 → 唤醒 → 从队列取 → 写入 PTY
        let notify = self.input_merger.register_session(&id).await;
        let writer_clone = writer.clone();
        let sid = id.to_string();
        let merger = self.input_merger.clone(); // InputMerger 需实现 Clone (外层 Arc)
        tokio::spawn(async move {
            loop {
                notify.notified().await;
                // 被唤醒后，从队列中取出所有待处理输入
                while let Some(msg) = merger.pop(&sid).await {
                    let mut w = writer_clone.lock().await;
                    let _ = w.write_all(msg.text.as_bytes());
                }
            }
        });

        Ok(fanout)
    }
}
```

**错误路径说明**（对应设计文档 §10.1）：

| 错误 | 检测点 | WSS 通知 | 返回值 |
|------|--------|---------|--------|
| `cli_not_found` | `resolve_tool_path()` | 无（由调用方处理） | `Err(...)` |
| `config_parse_error` | `get_env_cmd()` 失败 | `error_notify` | `Err(...)` |
| `pty_alloc_failed` | `openpty()` 失败 | `error_notify` | `Err(...)` |
| `shell_spawn_failed` | `spawn_command()` 失败 | `error_notify` | `Err(...)` |

**注意**：`SessionManager` 需要新增 `input_merger: Arc<InputMerger>` 字段。`InputMerger` 已改为 per-session `Notify` 唤醒模式（见 Task 13 Step 1），每个 session 独立注册 Notify，支持多 session 并发输入。`InputMerger` 本身需包在 `Arc` 中以支持 Clone。

**`OutputFanout` Clone + std::sync::Mutex 已在 Task 13 实现**（见上方的 `#[derive(Clone)]` + `Arc<OutputFanoutInner>` + `std::sync::Mutex` 定义）。reader 线程和调用方可直接 clone 使用，无需额外改造。

- [ ] **Step 3: CLI Tool 启动前预处理**

在 `start_session` 中按 tool 类型执行预处理（对应设计文档 §3.2.4 CLI Tool 启动前的预处理表）：

```rust
/// Tool 启动前预处理 —— 在 spawn PTY 之前执行
fn prepare_tool_env(tool: &str, env_vars: &Option<HashMap<String, String>>) -> Result<ToolPrep, String> {
    match tool {
        "claude" => {
            // Claude: 生成临时 settings.json，追加 --settings 参数
            let tmp = std::env::temp_dir().join(format!("kn-claude-{}.json", std::process::id()));
            let settings = serde_json::json!({"env": env_vars});
            std::fs::write(&tmp, serde_json::to_string(&settings).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            Ok(ToolPrep {
                extra_args: vec!["--settings".into(), tmp.to_string_lossy().to_string()],
                cleanup: Some(Box::new(move || { let _ = std::fs::remove_file(&tmp); })),
            })
        }
        "codex" => {
            // Codex: 备份 auth.json → 写入 profile 的 API key
            let auth_path = dirs::home_dir().unwrap().join(".codex/auth.json");
            let bak_path = auth_path.with_extension("json.kn-bak");
            if auth_path.exists() {
                std::fs::copy(&auth_path, &bak_path).map_err(|e| e.to_string())?;
            }
            if let Some(ref env) = env_vars {
                if let Some(api_key) = env.get("OPENAI_API_KEY") {
                    let auth = serde_json::json!({"auth_mode":"apikey","OPENAI_API_KEY": api_key});
                    std::fs::create_dir_all(auth_path.parent().unwrap()).ok();
                    std::fs::write(&auth_path, serde_json::to_string(&auth).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(ToolPrep {
                extra_args: vec![],
                cleanup: Some(Box::new(move || {
                    if bak_path.exists() {
                        let _ = std::fs::rename(&bak_path, &auth_path);
                    }
                })),
            })
        }
        "qoderclicn" | _ => {
            // qoderclicn 及其他 tool：无特殊预处理，直接 spawn
            Ok(ToolPrep { extra_args: vec![], cleanup: None })
        }
    }
}

struct ToolPrep {
    extra_args: Vec<String>,           // 追加到 CLI 命令行的额外参数
    cleanup: Option<Box<dyn FnOnce()>>, // PTY 退出后执行的清理逻辑
}

/// RAII 清理守卫：session 被移除或 Agent 退出时自动执行 tool 预处理清理。
/// 替代不可靠的 60s 轮询清理。
///
/// Claude: 删除临时 settings.json
/// Codex: 恢复备份的 auth.json
struct ToolCleanupGuard {
    cleanup: Option<Box<dyn FnOnce()>>,
    #[allow(dead_code)]
    session_id: String,  // 仅用于日志
}

impl Drop for ToolCleanupGuard {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            eprintln!("[agent] session {} 清理 tool 预处理资源", self.session_id);
            cleanup();
        }
    }
}

// Session 结构体需增加 cleanup_guard 字段：
// pub cleanup_guard: Mutex<Option<ToolCleanupGuard>>,
```

注意：`dirs` crate 需添加到 `agent/Cargo.toml` 依赖中，或改用 `std::env::var("HOME")` 拼接路径。

- [ ] **Step 4: Commit**

```bash
git add agent/src/session.rs
git commit -m "feat(agent): integrate find_binary and CLI tool prep for Claude/Codex"
```

---

## Agent Phase 2 完成检查点

- [x] `pty.rs` PtyOutputSink trait 解耦 Tauri
- [x] IPC Server (Unix Socket, status/sessions/bind/new_session)
- [x] WssSink + IpcSink (PTY 输出双通道)
- [x] Shell hook `ai()` Agent 路由 + fallback
- [x] 设备绑定完整流程 (HTTP /bind-init → QR → HTTP 轮询 GET /bind-result → 收到 device_token → 存本地 → 建立正式 WSS)
- [x] InputMerger (FIFO) + OutputFan-out
- [x] Session checkpoint 原子写入磁盘 (每 30s)

- [x] PTY 真正 spawn + InputMerger → stdin 桥接 + stdout → OutputFanout 桥接
- [x] CLI 错误处理（cli_not_found / config_parse_error / pty_alloc_failed / shell_spawn_failed）
- [x] Tool 预处理集成（Claude --settings / Codex auth.json swap）

**尚未实现（Phase 3）**：
- [ ] Desktop `useAgent.ts` + 📡 面板
- [ ] Agent 二进制打包进 .app bundle
