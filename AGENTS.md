# AGENTS.md

## 文档与事实来源

- 运行时行为以代码和测试为准；架构、Agent、Cloud 和协议的入口是 [docs/README.md](docs/README.md)。
- `CLAUDE.md` 仅是 Claude Code 入口，工程规则只维护在本文件，避免双份规范漂移。
- 发布操作以 [发布与上线总手册.md](发布与上线总手册.md) 为准；不要把凭据或服务器细节复制到其他文档。

## 仓库结构

| 部分 | 位置 | 职责 |
| --- | --- | --- |
| CLI 与 Shell Wrapper | `bin/`、`lib/`、`shell/` | `profile` 命令与 `ai()` 环境注入 |
| 共享 Rust 库 | `common/` | 路径、运行配置、加密与公共类型 |
| macOS Desktop | `desktop/` | Tauri UI、本地 PTY、项目与资源管理 |
| 本地 Agent | `agent/` | launchd 守护、PTY 会话、IPC、Cloud WSS |

默认运行配置根目录是 `~/.kn`。Rust 侧统一通过 `kn_common::path::config_dir()` 获取它：该函数支持绝对路径 `KN_HOME` 和旧的 `CLAUDE_PROFILES_HOME` 覆盖。Python CLI 与 Shell Wrapper 的默认位置仍是 `~/.kn/config.yaml`。

桌面 Debug 构建启动的 Agent 使用隔离根目录 `~/.kn-dev`、launchd 标签 `com.kn.agent.dev`；生产 Agent 使用 `~/.kn`、`com.kn.agent`。不要把 Agent 的开发隔离误写成运行配置拥有两套正式数据。

## 常用校验

```bash
# CLI / Shell
PYTHONPATH=lib python3 -m pytest tests/ -v
bash -n install.sh

# Rust workspace
cargo check -p kn-agent
cargo test -p kn-agent --lib
cargo check -p kn

# Desktop
cd desktop
npm run tauri dev
npx tsc --noEmit
npx vite build
cd src-tauri && cargo check
```

## 工程规范

### 运行配置

- `~/.kn/config.yaml` 是 CLI、Shell Wrapper 和生产 Desktop 共用的数据源。
- Python 写入必须通过 `lib/config.py::write_config`；Rust 运行配置读写使用 `common/src/profile.rs`。
- `kn_common::profile::write_config_file` 负责跨进程 `.config.lock`。Desktop 新增写入路径还必须在外层取得 `crate::with_write_lock`，并保持锁顺序：进程内锁 → 文件锁。
- 写入必须保留三代轮转备份，并采用临时文件、`fsync`、`rename`。不可直接覆盖 `config.yaml`。
- 删除默认 profile 时，Python 和 Rust 都必须把默认项设为剩余 profile 的字母序第一项；无剩余项则设为空字符串。

### 路径、进程与网络

- 获取用户主目录用 `kn_common::path::home_dir()`；获取 KN 配置目录用 `kn_common::path::config_dir()`。不要在业务模块自行拼接 `HOME`。
- 二进制解析用 `kn_common::path::find_binary()`；它包含 macOS 固定路径、登录 shell PATH 和 bare-name 回退。Desktop 的转发入口是 `commands/network.rs`。
- HTTP 使用 `reqwest`，SHA-256 使用 `sha2`；不要为这两类操作启动 `curl`、`shasum` 或 `sha256sum`。
- macOS PTY 使用 `/bin/zsh -i -l`，并确保 `TERM=xterm-256color`。

### Desktop 与 Agent

- 两个 PTY 面板独立。前端的终端输入在 `useTerminal/` 中按 `requestAnimationFrame` 批处理。
- 不要在 `setState` 后读取同步 ref 来判断最新 React state；把依赖最新 state 的副作用置于 updater 内。
- Agent 与 iOS 的边界由 Cloud 适配：iOS 公共消息使用 camelCase，Agent 内部消息以 `agent/src/proto.rs` 和 Cloud mapper/dispatcher 为准。
- 修改绑定、WSS 消息、会话恢复、ACK 或项目交付语义时，同时检查 `../kn-cloud`、`../kn-ios` 和跨仓测试。

### Shell Wrapper

- 唯一源文件是 `shell/ai-profile.sh`；Rust 只能以 `include_str!` 嵌入，不得复制字符串实现。
- `ensure_shell_rc()` 仅在内容变化时覆盖 `~/.kn/shell-rc`，并幂等维护 `.zshrc` 与 `.bashrc`。

## 发布

- 必须从 `main` 打 tag；版本只改根 `Cargo.toml` 的 `[workspace.package] version`。
- 当前 `.github/workflows/build-desktop.yml` 会构建、签名、公证、上传 `release-candidate-v<version>`，并创建或更新同 tag 的 GitHub Release。
- GitHub Actions 不部署 Cloud、Admin 或官网。正式自有发布仍由发布人将 DMG 与 Release Notes 上传 kn-admin 后完成。

完整步骤见 [RELEASE.md](RELEASE.md) 和[发布与上线总手册](发布与上线总手册.md)。
