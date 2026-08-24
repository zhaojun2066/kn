# Desktop

Desktop 位于 `desktop/`，是 macOS 专用的 Tauri v2 应用。React/TypeScript 前端在 `desktop/src/`，Rust 后端在 `desktop/src-tauri/`。

## 运行边界

```text
React UI ── Tauri invoke/event ── Rust commands ── PTY / filesystem / Agent IPC
                                      │
                                      └── <config_root>/config.yaml、projects.json
```

- `commands/`：平台、网络、文件、系统扫描、运行配置和发布相关 Tauri command。
- `profile_cmd.rs`：与 Python CLI 兼容地读写运行配置；必须同时使用进程内锁和 `.config.lock` 文件锁。
- `pty.rs`：本地终端，使用 `/bin/zsh -i -l` 和 `TERM=xterm-256color`。
- `agent_manager.rs`、`agent_runtime.rs`：安装、启动及健康检查 `kn-agent`。
- `project_manager.rs`：管理 `<config_root>/projects.json`。
- `skill_manager/`、`hook_manager.rs`：管理本机 AI CLI 的扩展资源与 hooks。

前端的 `useTerminal/` 管理终端状态、PTY 生命周期、会话同步和输入批处理。不要在 `setState` 后通过同步 ref 推断刚更新的 state；将依赖最新状态的副作用置于 state updater 内。

## 本地开发与校验

```bash
cd desktop
npm run tauri dev
npx tsc --noEmit
npx vite build
cd src-tauri && cargo check
```

`src-tauri/runtime-config.json` 提供桌面端的发布 API、Cloud WSS 与 Cloud HTTP 地址。Debug 构建优先查找 `runtime-config.dev.json`，并允许 `http://` / `ws://` 本地地址；Release 仅接受 `https://` / `wss://`。勿将凭据写入这些文件。

桌面候选包的构建、签名、公证和发布请看[发布与上线总手册](../发布与上线总手册.md)。
