# 工作流模式挖掘 — 头脑风暴记录

**日期:** 2026-06-05
**主题:** 自动检测用户重复工作流 → 生成 Claude Code Skill / Codex Command / Qoder Workflow
**状态:** 讨论中，待验证核心假设

---

## 1. 问题与目标

### 当前痛点
- AI CLI 工具（Claude Code / Codex / Qoder）对话记录散落各处，无法复用
- 用户经常重复类似的开发工作流，但每次都要重新组织 prompt
- 三个 CLI 的内置 skill/command 机制没有被自动填充的能力

### 目标
自动监控、分析用户的 AI CLI 使用行为，发现重复工作流模式，主动建议生成 Skill/Command 方便复用。

### 核心洞察

> **Skill 的本质是重复工作流，不是重复 Prompt。**

错误方向：
```
"review this code for security issues" → 重复 5 次 → 存成 prompt 模板
```

正确方向：
```
Session A: "Check auth module"      → Read → Grep → Read → Edit → Bash
Session B: "Review JWT validation"  → Read → Grep → Read → Edit → Bash
Session C: "Audit login flow"       → Read → Grep → Read → Bash

→ 同一个工作流: security-review
```

---

## 2. 原始方案（第一版）

```
Transcript → 归一化 → TF-IDF 相似度 → 聚类 → 生成 Skill
```

**能做的：** 发现重复 Prompt
**不能做的：** 发现重复工作流
**问题：** TF-IDF 只能识别词汇层面的相似，「security audit」和「check OWASP vulnerabilities」在语义上很近但词汇层面很远。

---

## 3. 修正后的 4 层架构

```
transcript_watcher         ← 增量扫描 ~/.claude/projects/、~/.codex/projects/
        ↓
session_parser             ← 解析 JSONL，按 session 切分
        ↓
feature_extractor
    ├── prompt_normalized   ← <FILE>/<CODE>/<NUMBER> 替换
    ├── tool_sequence       ← Read/Grep/Edit/Bash 链
    └── file_touch_pattern  ← 操作了哪些目录/文件类型
        ↓
pattern_detector
    ├── prompt_similarity   ← TF-IDF + 余弦 (v1) → Embedding (v2)
    └── behavior_similarity ← 行为链编辑距离 / Jaccard
        ↓
session_cluster            ← 加权融合 prompt + behavior，DBSCAN 聚类
        ↓
skill_candidate            ← 聚类 ≥ N 次触发 → 生成 Pattern 对象
        ↓
skill_generator            ← 调 claude CLI 生成 skill 文件 / 提供模板
```

---

## 4. 各层详细设计

### 4.1 第一层：归一化

做文本替换，消除项目特定信息：

| 原始内容 | 替换后 |
|---------|-------|
| `src/auth/user.go` | `<FILE>` |
| ` ```java ... ``` ` | `<CODE>` |
| `12345` | `<NUMBER>` |

结果：`review src/auth/user.go for security issue` → `review <FILE> for security issue`

### 4.2 第二层：相似度匹配

**v1（MVP）：TF-IDF + 余弦相似度**
- 纯 Rust 实现，零依赖
- 能发现词汇层面相似的 prompt
- 阈值：余弦相似度 > 0.75 → 候选

**v2（增强）：本地 Embedding 模型**
- all-MiniLM-L6-v2 或 bge-small-en
- 模型大小：20-80MB
- 能发现语义相似但词汇不同的 prompt
- 工程挑战：ONNX runtime 跨平台绑定、模型分发、冷启动
- **MVP 不做，但架构留接口**

### 4.3 第三层：行为特征提取（最关键）

从 transcript 提取结构化行为链：

```json
{
  "session_id": "abc123",
  "prompt": "Check the auth module for issues",
  "tool_sequence": ["Read", "Grep", "Read", "Edit", "Bash"],
  "files_touched": ["src/auth/user.go", "src/auth/middleware.go"],
  "shell_commands": ["go test ./auth/..."],
  "duration_seconds": 180
}
```

**行为链比文本靠谱得多** — Claude Code 的 tool use 模式高度结构化，`Read → Grep → Read → Edit → Bash` 极少偶发重复。

### 4.4 第四层：Session 聚类

不按 Prompt 聚类，按 Session 聚类：

```
Session A: Prompt="Review auth module"     Behavior=[Read, Grep, Read, Edit, Bash]
Session B: Prompt="Fix login bug"          Behavior=[Read, Grep, Read, Edit, Bash]
Session C: Prompt="Check permissions"      Behavior=[Read, Grep, Bash]

→ A 和 B 是同一个工作流；C 可能是同一类但更轻量
```

### 4.5 Pattern 中间层（关键架构决策）

不直接生成 Skill，先产出结构化的 `Pattern`：

```json
{
  "name": "security-review",
  "frequency": 12,
  "confidence": 0.85,
  "behavioral_signature": ["Read", "Grep", "Read", "Edit", "Bash"],
  "example_prompts": [
    "Check the auth module for issues",
    "Review JWT validation in login flow",
    "Look at src/middleware for security bugs"
  ],
  "example_sessions": ["sess_001", "sess_012", "sess_034"]
}
```

然后调用户自己的 Claude CLI 生成高质量 Skill：
```bash
claude --print "根据以下使用模式生成一个 Claude Code skill: <Pattern JSON>"
```

---

## 5. 精度风险与缓解策略

### 核心担忧

> 功能做完不准确 → 鸡肋

### 缓解：UX 设计先于算法

**不做：** 自动弹窗推荐（噪音大，用户烦）

**做：** 用户主动查看的「使用回顾」面板

```
┌─────────────────────────────────────────┐
│  📊 本周使用回顾                          │
│                                         │
│  你这周工作了 42 个 session，我们发现      │
│  了 3 个重复模式：                        │
│                                         │
│  🔍 security-review         12 次 ████  │
│     Review auth module                   │
│     Fix login bug                        │
│     Check permission middleware          │
│     → [生成 Skill]                       │
│                                         │
│  📝 generate-commit          8 次 ███    │
│     Write a commit message              │
│     Generate conventional commit         │
│     → [生成 Skill]                       │
│                                         │
│  ⚡ Fix tests                7 次 ██     │
│     （可能是噪声，模式较分散）               │
│     → [忽略]                             │
└─────────────────────────────────────────┘
```

**关键 UX 原则：**
- 不自动弹窗，用户主动打开（或每周 digest）
- 展示原始例子让用户自己判断
- 显示置信度和离散度
- 一键生成 / 一键忽略
- 用户可以标记「不是模式，以后别再推荐」

**即使检测精度只有 60%，用户自己会过滤，10 个建议里有 6 个真的 → 就已经很有价值。**

### 行为链天然更可靠

```
只看 Prompt → 模糊，语义歧义大，语言多样性高
只看 Tool Seq → 量化，结构化，标准化，噪声低
Prompt + Tool Seq → 双重验证，精度远超单一信号
```

---

## 6. 数据源现状

| CLI | Transcript 位置 | 格式 | 可用字段 |
|-----|----------------|------|---------|
| Claude Code | `~/.claude/projects/<project>/*.jsonl` | JSONL | role, content, tool_calls |
| Codex | `~/.codex/projects/<project>/` | JSONL | prompt, tool, patch, command |
| Qoder | 待确认 | — | — |

kn 已有的 `record-usage.py` Hook 可以从 Claude Code 的 Stop/SessionEnd 事件中拿到完整 tool call 序列，数据源已就绪。

---

## 7. 建议的下一步

### 验证实验（投入最小，先回答"值不值得做"）

1. 拿自己的 transcript 历史跑一个脚本（几十行 Python）
2. 提取所有 session 的 tool sequence + 归一化 prompt
3. 手工标注：「哪些 session 是真的重复工作流？」
4. 看最简单的行为链匹配能抓到多少

**决策标准：**
- 手工标注准确率 > 60% → 方向对，继续
- 准确率 < 30% → 放弃，回到 prompt 复用 / 用量分析等其他方向

### 如果验证通过，分阶段实施

**Phase 1（MVP）：**
- 归一化 + TF-IDF + 行为链匹配 + Session 聚类
- 「使用回顾」面板 UX
- 手动确认生成 Skill

**Phase 2（增强）：**
- 本地 Embedding 模型（语义泛化）
- 自动摘要 pattern 描述

**Phase 3（智能生成）：**
- Pattern → 自动调 Claude 生成 Skill
- 增量学习（用户确认/拒绝反馈回训练）

---

## 8. 未解决的问题

- [ ] 相似度阈值调优（TF-IDF 余弦 0.75 是否合适？）
- [ ] 行为链的相似度度量选型（编辑距离 vs Jaccard vs 序列比对？）
- [ ] Qoder 的 transcript 格式和 skill/command 机制待确认
- [ ] Codex Command 的生成格式
- [ ] 多项目模式：同一个工作流跨不同项目的行为链会变吗？
- [ ] 隐私：本地分析保证数据不出去，但用户是否需要"清除分析数据"按钮？
- [ ] tool_sequence 太长时的降维策略
