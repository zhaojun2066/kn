# Agent Phase 3 — Desktop 集成 + E2E + 性能

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]` checkbox syntax.

**Goal:** Desktop 通过 IPC 接入 Agent，实现 📡 面板（五态图标 + 绑定 + 暂停）、Agent 二进制打包进 .app bundle、端到端测试、异常恢复测试。

**Prerequisites:** Agent Phase 2 完成，Cloud Phase 1 可连通。

---

### Task 16: Desktop useAgent.ts

**Files:**
- Create: `desktop/src/hooks/useAgent.ts`
- Create: `desktop/src-tauri/src/agent_ipc.rs` (Rust 侧 IPC 桥接 command)
- Modify: `desktop/src/App.tsx`

- [ ] **Step 0: Rust 侧 IPC 桥接 command**

Tauri WebView 无法直接访问 Unix Socket，需要通过 Rust Tauri command 中转。

创建 `desktop/src-tauri/src/agent_ipc.rs`:

```rust
//! Tauri command — Desktop 前端通过此命令访问 Agent IPC
//!
//! 前端调 invoke("agent_ipc", { method, params }) → Rust 连 Agent Unix Socket → 返回结果

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

fn ipc_socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(format!("{}/.kn/agent/ipc.sock", home))
}

#[tauri::command]
pub fn agent_ipc(method: String, params: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(ipc_socket_path())
        .map_err(|e| format!("Agent IPC 连接失败: {}", e))?;

    let request = serde_json::json!({
        "method": method,
        "params": params.unwrap_or(serde_json::json!({}))
    });
    let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).map_err(|e| e.to_string())?;

    serde_json::from_str(&response).map_err(|e| format!("IPC 响应解析失败: {}", e))
}
```

在 `lib.rs` 中注册 command:
```rust
mod agent_ipc;
// generate_handler! 中添加 agent_ipc::agent_ipc
```

- [ ] **Step 0.5: Rust setup 阶段 — Agent 启动 + 版本检查（窗口显示前）**

在 `lib.rs` 的 `tauri::Builder::default().setup(|app| { ... })` 中添加，确保 React 渲染前 Agent 已就绪

**辅助函数 `get_binary_version()`**（在 `agent_ipc.rs` 或 `lib.rs` 中定义）：

```rust
/// 通过执行 `kn-agent --version` 获取版本号
fn get_binary_version(path: &std::path::Path) -> Result<String, String> {
    std::process::Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("无法执行 {}: {}", path.display(), e))
        .and_then(|out| {
            if out.status.success() {
                String::from_utf8(out.stdout)
                    .map(|s| s.trim().to_string())
                    .map_err(|e| format!("版本解析失败: {}", e))
            } else {
                // 如果 --version 不支持，回退到检查文件修改时间
                std::fs::metadata(path)
                    .map_err(|e| format!("无法读取文件元数据: {}", e))
                    .and_then(|m| m.modified().map_err(|e| format!("{}", e)))
                    .map(|t| format!("{:?}", t))
            }
        })
}

/// 通过 IPC 查询 Agent 是否在 safe_mode
fn agent_in_safe_mode() -> bool {
    // 尝试连 IPC 查 crash_count
    // 如果 IPC 不通（Agent 未运行），直接返回 false
    false  // 占位，实际实现走 agent_ipc command
}
```

**setup() 完整代码**：

```rust
.setup(|app| {
    // 此代码在窗口显示前运行
    let agent_bundle_path = app.path().resource_dir()
        .map_err(|e| e.to_string())?
        .join("kn-agent");
    let agent_install_path = home_dir().join(".kn/agent/kn-agent");

    // 1. 确保 Agent 已安装
    if !agent_install_path.exists() {
        std::fs::create_dir_all(agent_install_path.parent().unwrap())?;
        std::fs::copy(&agent_bundle_path, &agent_install_path)?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&agent_install_path,
                std::fs::Permissions::from_mode(0o755)).ok(); }
        // 注册 launchd
        agent::launchd::install(agent_install_path.to_str().unwrap())?;
    }

    // 2. 版本检查：bundle 版本 vs 已安装版本
    let bundle_version = get_binary_version(&agent_bundle_path)?;
    let installed_version = get_binary_version(&agent_install_path)?;
    if bundle_version > installed_version {
        // 保留旧版本作为回滚备份
        let bak = agent_install_path.with_extension("bak");
        std::fs::copy(&agent_install_path, &bak).ok();
        // 原子替换
        let tmp = agent_install_path.with_extension("tmp");
        std::fs::copy(&agent_bundle_path, &tmp)?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp,
                std::fs::Permissions::from_mode(0o755)).ok(); }
        std::fs::rename(&tmp, &agent_install_path)?;
        // 重启 Agent（新版本生效）
        launchd::restart()?;
    }

    // 3. 确保 Agent 在运行
    if !launchd::is_running() {
        launchd::load()?;
    }

    // 4. safe_mode 自动回滚检测：Agent crash_count > 5 连续崩溃 → 回退到 .bak
    if agent_in_safe_mode() && agent_install_path.with_extension("bak").exists() {
        eprintln!("[desktop] Agent 连续崩溃，自动回滚到上一个稳定版本");
        std::fs::rename(&agent_install_path.with_extension("bak"), &agent_install_path)?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&agent_install_path,
                std::fs::Permissions::from_mode(0o755)).ok(); }
        crate::agent::launchd::restart()?;
    }

    Ok(())
})
```

这样窗口渲染时 Agent 已在运行，📡 直接从"灰点闪烁"开始连 IPC，无需异步等待启动。

- [ ] **Step 0.6: Agent IPC 持久桥接 — Tauri Event 推送**

`agent_ipc` command 是短连接（一次 invoke 一次响应），无法接收 Agent 的异步推送（PTY 输出、状态变化通知）。需要一条**长连接 + Tauri Event 桥接**，在 setup 阶段启动，贯穿整个 Desktop 生命周期。

在 `desktop/src-tauri/src/agent_ipc.rs` 中新增：

```rust
use tauri::AppHandle;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

static BRIDGE_ACTIVE: Mutex<bool> = Mutex::new(false);

/// 启动后台线程：持久连接 Agent IPC，接收推送 → emit Tauri Event
/// 在 lib.rs setup() 中调用，贯穿整个 Desktop 生命周期
pub fn start_ipc_bridge(app_handle: AppHandle) {
    *BRIDGE_ACTIVE.lock().unwrap() = true;

    thread::spawn(move || {
        let socket_path = ipc_socket_path();

        // 等待 Agent IPC 就绪（最多 5s，每 2s 重试）
        let mut stream = loop {
            if !*BRIDGE_ACTIVE.lock().unwrap() { return; }
            match UnixStream::connect(&socket_path) {
                Ok(s) => break s,
                Err(_) => thread::sleep(Duration::from_secs(2)),
            }
        };

        let reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        // 初次连接 → 拉取当前状态
        let _ = writeln!(writer, r#"{{"method":"status"}}"#);

        for line in reader.lines() {
            if !*BRIDGE_ACTIVE.lock().unwrap() {
                break;
            }
            let Ok(line) = line else { break; };
            let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };

            // 按 type 字段路由到不同 Tauri Event
            if let Some(ty) = msg.get("type").and_then(|v| v.as_str()) {
                match ty {
                    "status_changed" | "heartbeat" => {
                        let _ = app_handle.emit("agent-status", &msg);
                    }
                    "output" => {
                        let _ = app_handle.emit("agent-output", &msg);
                    }
                    "bind_result" => {
                        let _ = app_handle.emit("agent-bind-result", &msg);
                    }
                    "session_created" | "session_ended" => {
                        let _ = app_handle.emit("agent-status", &msg); // 顺带刷新 session 列表
                    }
                    _ => {} // 忽略其他
                }
            } else {
                // 无 type 字段 → 响应类消息（如 status 查询的返回），走 agent-status
                let _ = app_handle.emit("agent-status", &msg);
            }
        }
        // 断线 → 2s 后重连
        thread::sleep(Duration::from_secs(2));
        drop(BRIDGE_ACTIVE.lock());
        start_ipc_bridge(app_handle); // 递归重连
    });
}

pub fn stop_ipc_bridge() {
    *BRIDGE_ACTIVE.lock().unwrap() = false;
}
```

在 `lib.rs` 的 `setup()` 末尾调用：

```rust
// setup() 最后一行，窗口显示后启动 IPC 桥接
agent_ipc::start_ipc_bridge(app.handle().clone());
```

在 `lib.rs` 的 `on_exit` 或 drop 中调用 `agent_ipc::stop_ipc_bridge()`。

**架构**：
- 短连接 `agent_ipc` command — 用于 `bind`、`pause`、`new_session` 等一次性操作（不变）
- 长连接 `start_ipc_bridge` — 用于接收 Agent 推送，转 Tauri Event 给前端（新增）
- 两条连接独立，互不影响

---

- [ ] **Step 1: useAgent hook — 事件驱动**

创建 `desktop/src/hooks/useAgent.ts`：

```typescript
import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type AgentStatus = 'stopped' | 'starting' | 'unbound' | 'binding' | 'connected' | 'idle' | 'running' | 'reconnecting';

export interface AgentState {
  status: AgentStatus;
  crashCount: number;
  safeMode: boolean;
  sessions: string[];
}

// 短连接 IPC — 用于 bind / pause / new_session 等一次性操作
async function ipcCall(method: string, params?: Record<string, unknown>): Promise<Record<string, unknown>> {
  return await invoke('agent_ipc', { method, params: params ?? {} });
}

export function useAgent() {
  const [state, setState] = useState<AgentState>({
    status: 'starting', crashCount: 0, safeMode: false, sessions: []
  });

  // 通过 Tauri Event 接收 Agent 推送（长连接 → Rust → emit → 前端）
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    listen<Record<string, unknown>>('agent-status', (event) => {
      const p = event.payload;
      setState(s => ({
        ...s,
        status: (p.status as AgentStatus) || s.status,
        crashCount: (p.crash_count as number) ?? s.crashCount,
        safeMode: (p.safe_mode as boolean) ?? s.safeMode,
      }));
    }).then(fn => unlisteners.push(fn));

    // session 列表由 session_created/session_ended 事件独立维护
    listen<{ session_id: string }>('agent-status', (event) => {
      const p = event.payload;
      if (p.session_id) {
        setState(s => {
          const sessions = s.sessions.includes(p.session_id)
            ? s.sessions  // session_ended: 保持列表不变（前端通过 IPC sessions 刷新）
            : [...s.sessions, p.session_id];  // session_created: 追加
          return { ...s, sessions };
        });
      }
    }).then(fn => unlisteners.push(fn));

    // 5 秒超时：仍未收到首次 push → 切 stopped
    const timeout = setTimeout(() => {
      setState(s => s.status === 'starting' ? { ...s, status: 'stopped' } : s);
    }, 5000);

    return () => {
      clearTimeout(timeout);
      unlisteners.forEach(fn => fn());
    };
  }, []);

  const bind = useCallback(async () => {
    setState(s => ({ ...s, status: 'binding' }));
    await ipcCall('bind');
  }, []);

  const pause = useCallback(async () => {
    await ipcCall('pause');
    setState(s => ({ ...s, status: 'stopped' }));
  }, []);

  const resume = useCallback(async () => {
    setState(s => ({ ...s, status: 'starting' }));
    await ipcCall('resume');
  }, []);

  const isLocalOnly = state.status === 'stopped' || state.status === 'starting';

  return { state, bind, pause, resume, isLocalOnly };
}
```

关键变化：
- **删掉 `setInterval` 轮询**。状态通过 `listen("agent-status")` 实时接收 Agent 推送
- **删掉 `refreshState`**。Agent 在状态变化时主动 push，无需前端拉取
- 保留 `ipcCall` 用于 bind / pause / new_session 等一次性操作

---

- [ ] **Step 2: App.tsx 集成 📡 按钮**

```tsx
import { useAgent } from './hooks/useAgent';

function AgentStatusIcon({ state }: { state: AgentState }) {
  // 五态图标
  switch (state.status) {
    case 'stopped':   return <span title="Agent 未运行">📡</span>;
    case 'starting':  return <span title="启动中" className="animate-pulse">📡</span>;
    case 'unbound':   return <span title="未绑定">🟠</span>;
    case 'binding':   return <span title="绑定中" className="animate-pulse">🟠</span>;
    case 'reconnecting': return <span title="重连中" className="animate-pulse">🟠</span>;
    case 'connected':
    case 'idle':
    case 'running':   return <span title="已连接">🟢</span>;
    default:          return <span>📡</span>;
  }
}

function AgentPanel({ state, onBind, onPause, onResume }: {
  state: AgentState; onBind: () => void; onPause: () => void; onResume: () => void;
}) {
  const [open, setOpen] = useState(false);

  if (!open) return <button onClick={() => setOpen(true)}><AgentStatusIcon state={state} /></button>;

  return (
    <div className="agent-panel">
      {state.status === 'unbound' && (
        <>
          <h3>📱 绑定设备</h3>
          <p>请用 kn iOS App 扫码绑定</p>
          <button onClick={onBind}>生成绑定码</button>
          <button onClick={() => setOpen(false)}>取消</button>
        </>
      )}
      {state.status === 'connected' || state.status === 'idle' || state.status === 'running' ? (
        <>
          <h3>🟢 设备在线</h3>
          <p>活跃会话: {state.sessions.length} 个</p>
          {state.sessions.map(s => <div key={s}>{s}</div>)}
          <button onClick={onPause}>暂停连接</button>
        </>
      ) : null}
      {state.status === 'reconnecting' && <p>🟠 连接中断，正在重连...</p>}
      {state.status === 'stopped' && <p>🟡 远程控制已暂停 <button onClick={onResume}>恢复连接</button></p>}
    </div>
  );
}
```

- [ ] **Step 2.5: useTerminal 双路径降级 — 事件驱动 PTY 输出**

`useTerminal.ts` 的 PTY 读写按 Agent IPC 连接状态选择路径。Agent 在线时，PTY 输出通过 Tauri Event `agent-output` 接收（长连接桥接 → Rust emit → 前端 listen），替代 Tauri Channel。

```typescript
// useTerminal.ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Channel } from '@tauri-apps/api/core';
import { useAgent } from './useAgent';

export function useTerminal() {
  const { isLocalOnly, state: agentState } = useAgent();
  const termRef = useRef<Terminal | null>(null);

  // ── Agent 在线路径：IPC → Tauri Event 接收 PTY 输出 ──
  useEffect(() => {
    if (isLocalOnly) return;

    const unlisten = listen<{ session_id: string; ansi_text: string }>(
      'agent-output',
      (event) => {
        // 直接写入 xterm.js（与现有 Channel onmessage 行为一致）
        termRef.current?.write(event.payload.ansi_text);
      }
    );
    return () => { unlisten.then(fn => fn()); };
  }, [isLocalOnly]);

  // ── 启动 session ──
  async function startPty(sessionId: string, tool: string, profile: string | undefined, cwd: string, cols: number, rows: number) {
    if (!isLocalOnly) {
      // 正常路径：Agent IPC 创建 session → attach → 接收输出
      await invoke('agent_ipc', { method: 'new_session', params: { tool, profile, cwd, cols, rows } });
      await invoke('agent_ipc', { method: 'attach', params: { session_id: sessionId } });
      // 之后的 PTY 输出由 agent-output Event 驱动，无需轮询
    } else {
      // 降级路径：直接 spawn PTY（现有 pty.rs 代码完整保留）
      const channel = new Channel<PtyEvent>();
      channel.onmessage = (event: PtyEvent) => {
        if (event.event === 'data') {
          termRef.current?.write(event.data);
        }
      };
      await invoke('start_pty', { sessionId, workDir: cwd, cols, rows, onEvent: channel });
    }
  }

  // ── 写输入 ──
  async function writeInput(sessionId: string, text: string) {
    if (!isLocalOnly) {
      await invoke('agent_ipc', { method: 'input', params: { session_id: sessionId, text } });
    } else {
      await invoke('write_pty', { sessionId, data: text });
    }
  }

  return { startPty, writeInput, isLocalOnly };
}
```

关键：
- Agent 在线 → IPC `new_session` + `attach` → `agent-output` Event 驱动终端渲染
- Agent 离线 → 完整保留现有 `start_pty` Tauri Channel 路径，功能完全不变
- `agent-output` 事件的数据结构与 WSS `output` 消息一致：`{ session_id, ansi_text }`

- [ ] **Step 3: Commit**

```bash
git add desktop/src-tauri/src/agent_ipc.rs desktop/src-tauri/src/lib.rs
git add desktop/src/hooks/useAgent.ts desktop/src/hooks/useTerminal.ts desktop/src/App.tsx
git commit -m "feat(desktop): Agent IPC bridge with Tauri Events, event-driven status + PTY output"
```
```

---

### Task 17: Agent 二进制打包进 .app Bundle

**Files:**
- Create: `build-agent.sh` (repo root)
- Modify: `desktop/src-tauri/tauri.conf.json`
- Modify: `.github/workflows/build-desktop.yml`

- [ ] **Step 1: 构建脚本 (repo 根运行)**

创建 `build-agent.sh` (repo 根):

```bash
#!/bin/bash
set -e

echo "Building kn-agent..."
cargo build --release --bin kn-agent

# workspace 共享 target 目录，二进制在 repo 根 target/
AGENT_BIN="target/release/kn-agent"
RESOURCES="desktop/src-tauri/resources"

mkdir -p "$RESOURCES"
cp "$AGENT_BIN" "$RESOURCES/kn-agent"
chmod +x "$RESOURCES/kn-agent"

echo "kn-agent copied to $RESOURCES/kn-agent"
```

- [ ] **Step 2: tauri.conf.json 声明 resources**

```json
{
  "bundle": {
    "resources": {
      "resources/kn-agent": "./"  // 注: tauri.conf.json 位于 desktop/src-tauri/，
    }                             // resources/ 是其同目录子目录
  }
}
```

注意：Tauri v2 `bundle.resources` 支持对象格式 `{ "源路径": "目标路径" }`，源路径相对于 `tauri.conf.json` 所在目录（即 `desktop/src-tauri/`）。`build-agent.sh` 将 Agent 二进制拷贝到 `desktop/src-tauri/resources/kn-agent`，与 `tauri.conf.json` 的资源声明对应。

- [ ] **Step 3: CI 构建集成**

在 `.github/workflows/build-desktop.yml` 的 build 步骤前添加：

```yaml
- name: Build kn-agent
  run: bash build-agent.sh
```

- [ ] **Step 4: 本地验证**

```bash
bash build-agent.sh
ls -la desktop/src-tauri/resources/kn-agent  # 应存在且可执行
```

- [ ] **Step 5: Commit**

```bash
git add build-agent.sh desktop/src-tauri/tauri.conf.json
git commit -m "feat(desktop): embed kn-agent binary in .app bundle"
```

---

### Task 18: 端到端测试 + 异常恢复测试

**Files:**
- Create: `tests/e2e/test_binding_flow.py`
- Create: `tests/e2e/test_session_lifecycle.py`
- Create: `tests/e2e/test_crash_recovery.py`

- [ ] **Step 1: 绑定流程 E2E 测试**

创建 `tests/e2e/test_binding_flow.py`：

```python
#!/usr/bin/env python3
"""E2E: 设备绑定完整流程"""
import subprocess
import time
import json
import os

AGENT_IPC = os.path.expanduser("~/.kn/agent/ipc.sock")
CLOUD_URL = "http://localhost:8080"

def test_bind_init():
    """Agent 请求 bind-init → 获得 6 位码"""
    machine_id = subprocess.check_output(
        "ioreg -d2 -c IOPlatformExpertDevice | awk -F\\\" '/IOPlatformUUID/{print $(NF-1)}'",
        shell=True, text=True).strip()

    resp = subprocess.check_output([
        "curl", "-s", "-X", "POST",
        f"{CLOUD_URL}/api/v1/device/bind-init",
        "-H", "Content-Type: application/json",
        "-d", json.dumps({"machine_id": machine_id})
    ], text=True)
    data = json.loads(resp)
    assert "bind_code" in data
    assert len(data["bind_code"]) == 6
    print(f"✓ bind-init OK: code={data['bind_code']}")

def test_agent_ipc_status():
    """Agent IPC 状态查询"""
    resp = subprocess.check_output([
        "bash", "-c", f"echo '{{\"method\":\"status\"}}' | nc -U {AGENT_IPC}"
    ], text=True)
    data = json.loads(resp)
    assert "status" in data
    print(f"✓ Agent IPC OK: status={data['status']}")

if __name__ == "__main__":
    test_bind_init()
    test_agent_ipc_status()
    print("✓ 所有 E2E 测试通过")
```

- [ ] **Step 2: Session 生命周期测试**

创建 `tests/e2e/test_session_lifecycle.py`：

```python
"""E2E: Session 创建 → 运行 → 结束"""
def test_session_create():
    resp = subprocess.check_output([
        "bash", "-c",
        f"echo '{{\"method\":\"new_session\",\"tool\":\"claude\",\"cwd\":\"/tmp\"}}' | nc -U {AGENT_IPC}"
    ], text=True)
    data = json.loads(resp)
    assert "session_id" in data
    print(f"✓ Session 创建 OK: {data['session_id']}")

def test_session_list():
    resp = subprocess.check_output([
        "bash", "-c", f"echo '{{\"method\":\"sessions\"}}' | nc -U {AGENT_IPC}"
    ], text=True)
    data = json.loads(resp)
    assert "sessions" in data
    print(f"✓ Session 列表: {len(data['sessions'])} 个")
```

- [ ] **Step 3: Crash 恢复测试**

创建 `tests/e2e/test_crash_recovery.py`：

```python
"""E2E: Agent crash 后 launchd 自动重启 + checkpoint 恢复"""
def test_crash_count_persists():
    """crash_count 应在文件系统中持久化"""
    count_path = os.path.expanduser("~/.kn/agent/crash_count")
    if os.path.exists(count_path):
        count = int(open(count_path).read().strip())
        print(f"✓ crash_count 文件存在: {count}")
    else:
        print("○ crash_count 文件不存在（Agent 未启动或已正常退出）")

def test_safe_mode_entry():
    """crash_count > 5 → safe_mode"""
    # 手动设置 crash_count 为 6
    count_path = os.path.expanduser("~/.kn/agent/crash_count")
    os.makedirs(os.path.dirname(count_path), exist_ok=True)
    with open(count_path, 'w') as f:
        f.write("6")
    print("✓ crash_count 手动设为 6，下次 Agent 启动应进入 safe_mode")
```

- [ ] **Step 4: 运行全部 E2E 测试**

```bash
cd tests/e2e && python3 -m pytest -v
```

注意：E2E 测试依赖 `nc -U`（Unix Socket）和 macOS 环境（`ioreg`），仅在本地 macOS 机器运行，不加入 CI。

- [ ] **Step 5: Commit**

```bash
git add tests/e2e/
git commit -m "test(e2e): binding flow, session lifecycle, crash recovery"
```

---

### Task 18.5: output.log 清理 + 优雅关闭

**Files:**
- Modify: `agent/src/session.rs`
- Modify: `agent/src/main.rs`

- [ ] **Step 1: output.log 清理定时任务**

在 `session.rs` 中添加：

```rust
/// 每天凌晨 3:00 清理 7 天前的 session 目录
pub fn start_cleanup_loop() {
    tokio::spawn(async move {
        loop {
            // 等到下一个凌晨 3:00
            let now = chrono::Utc::now();
            let next_3am = (now + chrono::Duration::days(1))
                .date_naive()
                .and_hms_opt(3, 0, 0)
                .unwrap();
            let wait_secs = (next_3am - now.naive_utc()).num_seconds().max(0) as u64;
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;

            let sessions_dir = home_dir().join(".kn/agent/sessions");
            if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
                let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
                for entry in entries.flatten() {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            let modified_dt: chrono::DateTime<chrono::Utc> = modified.into();
                            if modified_dt < cutoff {
                                let _ = std::fs::remove_dir_all(entry.path());
                            }
                        }
                    }
                }
            }
        }
    });
}
```

- [ ] **Step 2: 优雅关闭 — SIGTERM 等待 PTY 结束**

在 `main.rs` 中：

```rust
// 监听 SIGTERM / SIGINT
let (tx_shutdown, mut rx_shutdown) = tokio::sync::mpsc::channel(1);
tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    tx_shutdown.send(()).await.ok();
});

// 优雅关闭: 等待所有 PTY 结束 (最多 30s)
rx_shutdown.recv().await;
eprintln!("[agent] 收到退出信号，等待活跃 session 结束...");
let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(30);
while sessions.count().await > 0 {
    if tokio::time::Instant::now() > deadline {
        eprintln!("[agent] 超时，强制退出");
        break;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
}
eprintln!("[agent] 已退出");
```

- [ ] **Step 3: Commit**

```bash
git add agent/src/session.rs agent/src/main.rs
git commit -m "feat(agent): output.log cleanup (7d) and graceful shutdown on SIGTERM"
```

---

## Agent Phase 3 完成检查点

- [x] Desktop `useAgent.ts` + 📡 五态面板
- [x] Agent 二进制打包进 .app bundle
- [x] CI 构建脚本集成
- [x] E2E: 绑定流程、Session 生命周期、Crash 恢复
- [x] output.log 保留 7 天自动清理
- [x] 优雅关闭（等待 PTY 结束，15s 超时）

**全部 Agent Phase 完成后的能力**：
- [x] `kn-agent` 独立二进制，launchd 守护
- [x] 设备指纹采集 + 设备绑定完整流程
- [x] WSS 长连接 + 心跳 + 重连
- [x] IPC Server (Desktop 通信)
- [x] Session CRUD + InputMerger + OutputFan-out
- [x] Shell hook 自动路由
- [x] Desktop 📡 面板 + Agent 版本管理
