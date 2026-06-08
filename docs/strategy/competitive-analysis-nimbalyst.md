# 竞品分析：Nimbalyst & 产品路线图

> 2026-06-03 | 分析对象：https://nimbalyst.com/

---

## 一、Nimbalyst 是什么

**定位**：本地优先、开源的 AI 开发可视化工作台（原项目名 Crystal，MIT 协议）

**架构**：Electron 桌面应用，macOS / Windows / Linux + iOS 移动端

**核心能力**：

| 模块 | 说明 |
|------|------|
| 7 种内置编辑器 | Markdown（含 AI diff）、代码（Monaco）、UI 原型、Mermaid 图表、Excalidraw 画布、数据模型（Prisma 导出）、CSV 表格 |
| Agent 会话管理 | 并行运行多个 Claude Code / Codex 会话，看板组织（backlog → planning → implementing → complete），会话持久化、可搜索、可恢复 |
| 任务追踪 | `@task` `@bug` `@idea` `@decision` 标签，看板视图，Agent 可自动创建/编辑/完成任务 |
| Git 可视化 | diff、stage、branch 图形化管理，AI 辅助生成 commit message |
| 终端 | 集成 Ghostty 终端 |
| MCP 支持 | 连接 Linear、GitHub、Slack、数据库等 |
| git worktree 支持 | 隔离的并行分支工作目录 |
| iOS 移动端 | 远程监控会话、回复 Agent 问题、审批 diff |

**商业模式**：个人免费（无功能限制、无试用期），团队版（协作功能）即将推出，SOC 2 Type 2 认证

---

## 二、Nimbalyst 的弱点 → 我们的机会

| Nimbalyst 弱点 | 严重程度 | 我们的机会 |
|---------------|---------|-----------|
| **没有 Token 用量 / 费用追踪** |  致命 | **核心差异化方向，做到极致** |
| 不管理 API Key / Profile |  致命 | 我们已做，继续深挖 |
| 学习曲线高、太重 |  高 | 保持轻量，不强迫用户换 IDE |
| 仅支持 Claude Code + Codex |  中 | 我们兼容任意 OpenAI 兼容服务商 |
| 无多语言 |  中 | 我们的中文原生是优势 |
| 无离线模式 |  低 | PTY 终端天然不依赖云端 |
| 无导入导出 |  低 | 我们已有 JSON 导入导出 |
| iOS 独占移动端 |  低 | 暂无必要跟进 |

---

## 三、我们的定位

**不做新 IDE，做 AI 开发的「控制面板」**

```
Nimbalyst = VS Code + Claude Code + Jira + Draw.io  → 太重，换工具成本高
我们      = 1Password for API Keys + 费用仪表盘 + 智能切换  → 轻量，配合现有工具
```

用户已经有了趁手的编辑器（VS Code / Neovim / Cursor / Zed），不需要换。他们缺的是：

1. **钱花哪了？** — API 费用仪表盘（Nimbalyst 完全没有）
2. **key 怎么管？** — 我们已经做了，继续深挖预算告警
3. **怎么更省？** — 智能推荐"这个任务用 DeepSeek 更便宜"

---

## 四、功能路线图

### Phase 1 — 建立差异化（1-2 周）

| 优先级 | 功能 | 说明 | 实现难度 |
|--------|------|------|---------|
| P0 | **Token 用量追踪** | PTY 输出拦截 Claude Code / Codex 的 token usage 日志，存入本地 SQLite | 中 |
| P0 | **费用仪表盘** | 今日/本周/本月消费，按 profile 拆分，自动匹配各供应商定价 | 中 |
| P0 | **项目绑定 `.ai-profile`** | `cd` 进项目自动读取 `.ai-profile` 文件选 profile，比 direnv 轻 100 倍 | 低 |
| P1 | **用量实时状态栏** | 状态栏显示当前 session 的 token 消耗和预估费用 | 低 |

### Phase 2 — 智能控制（2-4 周）

| 优先级 | 功能 | 说明 | 实现难度 |
|--------|------|------|---------|
| P1 | **预算告警** | 每 profile 设月度预算上限，80% toast 提醒，100% 自动停用 | 中 |
| P2 | **省钱建议** | "上月 Claude 花了 $120，同样任务 DeepSeek 只需 $8" | 高 |
| P2 | **用量异常检测** | Token 消耗速率突然飙升 → 弹警告（可能是 key 泄露/被盗刷） | 中 |
| P2 | **Prompt 模板库** | 预设 + 自定义 prompt 模板，`ai claude --prompt review` 快速加载 | 低 |

### Phase 3 — 生态联动（长期）

| 优先级 | 功能 | 说明 | 实现难度 |
|--------|------|------|---------|
| P3 | **Context 快照** | 一键保存当前 Claude Code 会话上下文为存档，跨项目复用 | 高 |
| P3 | **团队 Profile 共享** | API key 加密存储、用量分开统计、权限控制 | 高 |
| P3 | **Prompt 模板市场** | 社区分享常用模板，一键安装 | 中 |

---

## 五、与 Nimbalyst 的正面差异对比

| 维度 | Nimbalyst | 我们 |
|------|-----------|------|
| 本质 | 可视化 AI 开发 IDE | AI 开发的"仪表盘 + 遥控器" |
| 目标用户 | 想在一个工具里完成所有事的开发者 | 已有趁手工具、需要精细化管控的开发者 |
| 侵入性 | 高 — 需要离开现有 IDE | 低 — 配合现有终端/编辑器 |
| API Key 管理 |  无 |  核心能力 |
| 多供应商支持 | Claude Code + Codex | 任意 OpenAI 兼容服务商 |
| 费用追踪 |  无 |  核心差异化 |
| 学习曲线 | 陡峭 | 开箱即用 |
| 语言 | 英文 | 中文原生 |
| 开源 | MIT | 待定 |

---

## 六、决策建议

**现阶段不要做的事：**
-  不要做代码编辑器（干不过 VS Code / Cursor / Nimbalyst）
-  不要做图表/原型工具（偏离核心场景）
-  不要做移动端（目前无需求）
-  不要做团队协作（先吃透个人开发者）

**现阶段要 All-in 的：**
-  **Token 用量 + 费用仪表盘** — 这是 Nimbalyst 完全没覆盖的蓝海
-  **Profile 管理深化** — 预算控制、异常检测、智能推荐
-  **轻量体验** — 保持"安装即用、不换工具"的定位

---

## 七、相关链接

- Nimbalyst 官网：https://nimbalyst.com/
- Nimbalyst GitHub：https://github.com/Nimbalyst/nimbalyst
- Dev.to 对比评测：https://dev.to/stravukarl/best-claude-code-gui-in-2026-5-tools-compared-289i
