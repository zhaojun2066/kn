# kn 远程控制 — 执行指南

> 7 个 plan + Phase 4 收尾、3 个 repo、~49 个 task。按依赖顺序执行。

## 执行顺序总览

```
┌─────────────────────────────────────────────────────────┐
│ 第 1 轮: 三端骨架并行 (可同时开工)                         │
│                                                         │
│  Agent P1 ──── Cloud P1 ──── iOS P1                     │
│  (kn repo)    (kn-cloud)    (kn-ios)                    │
│  8 tasks      10 tasks       9 tasks                    │
│                                                         │
│  产出: Agent 能启动 + Cloud 能注册/绑定 + iOS 能登录/终端    │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────┐
│ 第 2 轮: 核心连通 (Agent P2 先做，其余依赖它)               │
│                                                         │
│  Agent P2 ──┬── Cloud P2                                │
│  (kn repo)  │  (kn-cloud)                               │
│  8 tasks    │  5 tasks (9/9.5/9.6/10/11)                │
│             │                                           │
│             ├── iOS P2                                   │
│             │  (kn-ios)                                  │
│             │  3 tasks                                  │
│             │                                           │
│  产出: 三端打通 — Desktop↔Agent↔Cloud↔iOS 全链路          │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────┐
│ 第 3 轮: Desktop 集成 + 测试                              │
│                                                         │
│  Agent P3                                               │
│  (kn repo)                                              │
│  4 tasks (Task 16, 17, 18, 18.5)                        │
│                                                         │
│  产出: Desktop 📡 面板 + Agent 打包 + E2E 测试             │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────┐
│ 第 4 轮: 收尾 — Python CLI 兼容 + 部署检查                  │
│                                                         │
│  Phase 4                                                │
│  (kn repo)                                              │
│  2 tasks (Task 19, 20)                                  │
│                                                         │
│  产出: Python CLI 可读加密 config + 生产部署 checklist      │
└─────────────────────────────────────────────────────────┘
```

## 各阶段 Skill 使用指南

每个 Phase 启动时，使用以下 skill 组合：

### 1. 启动 Phase 前 — 准备工作

```
/using-git-worktrees   # 创建隔离 worktree（可选，建议用）
```

创建 worktree 分支：
- Agent P1-P3: `feat/agent-phase<N>-<描述>`
- Cloud P1-P2: 在 `kn-cloud` repo 的 `main` 分支
- iOS P1-P2: 在 `kn-ios` repo 的 `main` 分支

### 2. 执行 Task 时 — 开发工作流

```
/superpowers:subagent-driven-development   # 推荐: 每个 Task 一个 subagent
```

或按传统方式逐 task 手动执行。每个 Task 内步骤自带 checkbox 和 commit message。

### 3. Task 完成后 — 代码审查

```
/code-review   # 每完成 2-3 个 Task 做一次 review
```

### 4. Phase 完成后 — 收尾

```
/superpowers:verification-before-completion   # 验证 Phase 成果
```

---

## Agent Phase 1（kn repo，8 tasks: Task 1-7 + Task 6.5）

**入口 plan**：`docs/superpowers/plans/2026-06-16-agent-phase1.md`

**Prerequisites**：当前 `feat/remote-control-design` 分支

**Skill**：`/superpowers:subagent-driven-development`

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 1 | 独立: Cargo workspace + kn-common + 二进制骨架 |
| Round 2 | Task 2 + 3 + 4 | 并行: fingerprint验证 + state + proto（Task 2 仅引用 common） |
| Round 3 | Task 5 + 6 | WSS client + session（依赖 proto） |
| Round 4 | Task 6.5 + 7 | crash 持久化 + launchd |

**完成标志**：`cargo build --bin kn-agent && cargo test --bin kn-agent` 全绿

---

## Cloud Phase 1（kn-cloud repo，10 tasks: Task 1-7 + 2.5 + 4.5 + 6.5）

**入口 plan**：`docs/superpowers/plans/2026-06-16-cloud-phase1.md`

**Prerequisites**：GitHub 创建私有 repo `kn-cloud`，clone 到本地

**Skill**：`/superpowers:subagent-driven-development`

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 1 | Maven 多模块项目 |
| Round 2 | Task 2 | DB Entity + Mapper + init.sql |
| Round 3 | Task 3 | 用户模块 (注册/登录/JWT) |
| Round 4 | Task 4 | 设备绑定 (bind-init/confirm/list/unbind) |
| Round 5 | Task 5 + 6 | WSS 中继 + Nginx/限流/并发检测/消息持久化 |

**完成标志**：
```bash
curl -X POST localhost:8080/api/v1/auth/register -d '{"email":"t@kn.dev","password":"x"}'
# → access_token
curl localhost:8081/ws  # WebSocket 200 upgrade
```

---

## iOS Phase 1（kn-ios repo，9 tasks: Task 1-8 + 5.5）

**入口 plan**：`docs/superpowers/plans/2026-06-16-ios-phase1.md`

**Prerequisites**：GitHub 创建私有 repo `kn-ios`，Xcode 新建项目

**Skill**：`/superpowers:subagent-driven-development`（Xcode 操作为主，subagent 辅助 Swift 代码）

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 1 | Xcode 项目 + 目录 |
| Round 2 | Task 2 + 3 | Keychain + API + WSS client（无 UI） |
| Round 3 | Task 4 | 登录/注册 UI |
| Round 4 | Task 5 + 5.5 | TerminalView + 屏幕适配 |
| Round 5 | Task 6 + 7 + 8 | 设备绑定 + TabView + 直通模式 + APNs |

**完成标志**：Simulator 上跑通登录 → 终端连接 WSS → 发送输入 → 收到 output

---

## Agent Phase 2（kn repo，8 tasks）

**入口 plan**：`docs/superpowers/plans/2026-06-16-agent-phase2.md`

**Prerequisites**：Agent P1 完成，Cloud P1 WSS 可连通

**Skill**：`/superpowers:subagent-driven-development`

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 8 | pty.rs trait 抽取（影响现有代码） |
| Round 2 | Task 9 | IPC Server |
| Round 3 | Task 10 + 13 | WssSink/IpcSink + InputMerger/OutputFan-out |
| Round 4 | Task 11 + 14 | Shell hook + checkpoint |
| Round 5 | Task 12 + 15 | 绑定流程 + find_binary |

**完成标志**：`ai claude xyz` → Agent IPC 创建 session → Cloud WSS 收到 session_created

---

## Cloud Phase 2（kn-cloud repo，5 tasks: Task 9/9.5/9.6/10/11）

**入口 plan**：`docs/superpowers/plans/2026-06-16-cloud-phase2.md`

**Prerequisites**：Cloud P1 完成

**Skill**：`/superpowers:subagent-driven-development`

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 9 + 9.5 | 会员到期/缓冲期 + 消息保留清理 |
| Round 2 | Task 10 | APNs 推送 |
| Round 3 | Task 11 | 卡密生成 |

---

## iOS Phase 2（kn-ios repo，3 tasks）

**入口 plan**：`docs/superpowers/plans/2026-06-16-ios-phase2.md`

**Prerequisites**：iOS P1 完成，Cloud P2 APNs 可用

**Skill**：`/superpowers:subagent-driven-development`

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 9 | 推送通知处理 |
| Round 2 | Task 10 | Sessions 历史 |
| Round 3 | Task 11 | 语音输入 |

---

## Agent Phase 3（kn repo，4 tasks）

**入口 plan**：`docs/superpowers/plans/2026-06-16-agent-phase3.md`

**Prerequisites**：Agent P2 + Cloud P1 完成

**Skill**：`/superpowers:subagent-driven-development`

**执行**：

| 回合 | Tasks | 说明 |
|------|-------|------|
| Round 1 | Task 16 | Desktop useAgent + 📡 面板 |
| Round 2 | Task 17 | Agent 二进制打包 + CI |
| Round 3 | Task 18 | E2E 测试 |
| Round 4 | Task 18.5 | output.log 清理 + 优雅关闭 |

**完成标志**：Desktop 📡 绿点 + `bash build-agent.sh` 成功 + `pytest tests/e2e/` 全绿

---

## Phase 4: 收尾 + 兼容（无独立 plan 文件）

Agent/Cloud/iOS Phase 1-3 完成后，以下两项需在合并 main 前完成：

### Task 19: Python CLI 兼容加密 config

**设计文档 §6.5 + 附录 §11 Phase 4**

`config_crypto.rs` (kn-common) 加密 env var value 后，`bin/profile` CLI 需能正确读取：

- [ ] **Step 1**: `lib/config.py` 集成 AES-256-GCM 解密
  - 检测 value 前缀 `kn:v1:` → 通过 `security` 模块调 macOS Keychain 获取主密钥
  - 解密后返回原始 value
  - 无此前缀的旧明文 value 正常读取（向前兼容）
  - 新增 `pycryptodome` 依赖（Python AES-GCM 实现）
- [ ] **Step 2**: `bin/profile list -j` 验证：加密后的 env var 应能正确展示原始值
- [ ] **Step 3**: 单元测试：`tests/test_config_crypto.py` — 加密封装 + 解密 + 明文兼容

### Task 20: 生产部署前安全检查清单

- [ ] iOS Certificate pinning 启用 (ios-phase2 已标记为后续)
- [ ] `kn-cloud` 生产环境变量配置完毕（`/opt/kn-cloud/kn-cloud.env`）
- [ ] APNs p8 key 部署 + 推送验证
- [ ] Nginx TLS 证书配置 + Let's Encrypt 自动续期
- [ ] DB 备份策略
- [ ] Agent crash 告警（至少日志监控）

---

## 本地开发环境变量

三端接入同一云服务时，需统一覆盖默认 URL 指向本地。以下为所有需要配置的环境变量：

| 组件 | 变量 | 默认值 | 本地开发值 | 配置方式 |
|------|------|--------|-----------|---------|
| Agent | `KN_CLOUD_URL` | `wss://api.shark.kim` | `ws://localhost:8081` | shell `export` 或 `launchctl setenv` |
| Agent HTTP | (同上) | `https://api.shark.kim` | `http://localhost:8080` | `KN_CLOUD_URL` 同时控制 HTTP 和 WSS；Agent 内部自动替换 `wss://` ↔ `https://` |
| iOS App | `KN_API_BASE_URL` | `https://api.shark.kim` | `http://localhost:8080` | Xcode Scheme → Run → Arguments → Environment Variables；或 `Info.plist` 中改默认值 |
| Cloud (Spring) | `SPRING_PROFILES_ACTIVE` | `prod` | `dev` | systemd `EnvironmentFile` (prod) / shell `export` (dev) |
| Cloud DB | `DB_USER` / `DB_PASS` | (env var) | `root` / `12345678` | dev: `application-dev.yml` 内置；prod: 从 `kn-cloud.env` 注入 |
| Cloud JWT | `kn.jwt.secret` | `JWT_SECRET` env var | 硬编码 `dev-secret-...` | dev 用内置默认值；prod 从 env 注入 |
| Cloud Redis | `REDIS_HOST` / `REDIS_PASS` | (env var) | `localhost:6379` 无密码 | dev 硬编码；prod 从 env 注入 |
| Cloud Redeem | `REDEEM_AES_KEY` | (env var) | (同 prod) | 本地测试也需设置，`openssl rand -base64 32` |

**Agent 本地启动**（不使用 launchd）：

```bash
# 前提: MySQL (localhost:3306, root/12345678) + Redis (localhost:6379, 无密码) 已运行
# 首次需导入表结构: mysql -u root -p12345678 < kn-cloud/deploy/init.sql

# 终端 1: 启动 API (dev profile)
cd kn-cloud
export SPRING_PROFILES_ACTIVE=dev
mvn -pl kn-cloud-api spring-boot:run   # → :8080

# 终端 2: 启动 WS (dev profile)
cd kn-cloud
export SPRING_PROFILES_ACTIVE=dev
mvn -pl kn-cloud-ws spring-boot:run     # → :8081

# 终端 3: 启动 Agent
export KN_CLOUD_URL=http://localhost:8081
cd kn && cargo run --bin kn-agent
```

**dev vs prod 环境对照**：

| 配置项 | dev (`application-dev.yml`) | prod (`application-prod.yml`) |
|--------|---------------------------|------------------------------|
| DB | `localhost:3306`, user `root` | `${DB_HOST}`, `${DB_USER}` from env |
| Redis | `localhost:6379`, no password | `${REDIS_HOST}`, password from env |
| JWT secret | 硬编码 `dev-secret-...` | `${JWT_SECRET}` from env |
| APNs | (无，推送不工作) | env var 注入 |
| 日志级别 | DEBUG | INFO |
| SSL | HTTP (明文) | HTTPS (Nginx 终止 TLS) |

**iOS 本地调试**：Xcode → Edit Scheme → Run → Arguments → Environment Variables → 添加 `KN_API_BASE_URL` = `http://<your-mac-ip>:8080`（因为 Simulator 中 `localhost` 指向虚拟机的 localhost，不是 Mac）。

## 关键约束

| 规则 | 说明 |
|------|------|
| Agent P1 + Cloud P1 + iOS P1 **可并行** | 三个 repo 互不依赖 |
| Agent P2 **必须先做** | Cloud P2 和 iOS P2 依赖 Agent→Cloud 连通 |
| 每完成 2-3 个 Task 做一次 `/code-review` | 防止积累问题 |
| 每个 Phase 完成做 `/superpowers:verification-before-completion` | 确认 Phase 产物可用 |
| Commit 频率 | 每个 Task 结束 commit，不要跨 Task |

## 快速启动

```bash
# 1. Agent P1 (当前 repo)
cd /Users/zhaojun/workspace/me/shark/kn
git checkout -b feat/agent-phase1
# 打开 docs/superpowers/plans/2026-06-16-agent-phase1.md 开始

# 2. Cloud P1 (新建私有 repo)
# GitHub → New private repo → kn-cloud
git clone git@github.com:zhaojun2066/kn-cloud.git
# 打开 docs/superpowers/plans/2026-06-16-cloud-phase1.md 开始

# 3. iOS P1 (新建私有 repo)
# GitHub → New private repo → kn-ios
# Xcode → New Project → 打开 docs/superpowers/plans/2026-06-16-ios-phase1.md 开始
```
