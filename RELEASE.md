# Release Guide

AI Profile Manager 的发布流程完全自动化：推送 Git tag 即可触发 CI 构建所有平台、生成 release notes、创建 GitHub Release。

## 前置条件

- 仓库 push 权限
- Git 2.0+
- 本地已 clone 仓库并切换到 main 分支

## 发布前检查清单

- [ ] `desktop/` 编译通过：`cd desktop && npx tsc --noEmit && npx vite build && cd src-tauri && cargo check`
- [ ] 手动冒烟测试过你手头平台的功能（macOS 直接 `npm run tauri dev` 跑一下）
- [ ] `CLAUDE.md` / `README.md` 如果有架构变化已同步更新
- [ ] 确认本次要发布的所有 commit 已经 push 到 main

## 发布步骤

完整的发布流程分为三个阶段：**功能分支开发 → 合并到 main → 打 tag 发布**。

### 阶段一：功能分支开发（日常）

```bash
# 从 main 拉功能分支
git checkout main
git pull origin main
git checkout -b feat/my-feature

# ... 开发、commit（遵循 conventional commits）...
git commit -m "feat: add new feature X"
git commit -m "fix: resolve bug Y"

# 推送功能分支
git push origin feat/my-feature
```

### 阶段二：合并到 main

```bash
# 切回 main 并拉取最新
git checkout main
git pull origin main

# 合并功能分支
git merge feat/my-feature

# 或者用 PR（推荐）：在 GitHub 网页上创建 PR → review → merge

# 推送到远程 main
git push origin main
```

### 阶段三：打 tag 发布

> **注意**：以下步骤必须在 **main 分支**上执行，不要在功能分支上打 release tag。

```bash
# 确保当前在 main 分支且已是最新
git checkout main
git pull origin main
```

#### 1. 更新版本号

同步修改两个文件：

**`desktop/src-tauri/tauri.conf.json`**:
```json
"version": "1.0.7"
```

**`desktop/src-tauri/Cargo.toml`**:
```toml
version = "1.0.7"
```

#### 2. 提交版本升级

```bash
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/Cargo.toml
git commit -m "release: v1.0.7"
```

#### 3. 打 annotated tag

```bash
git tag -a v1.0.7 -m "v1.0.7"
```

> **重要**：必须用 `-a` 打 annotated tag（含元信息），不要用轻量 tag。Tag 名必须以 `v` 开头，版本号三段式 semver（`v1.0.7`）。

#### 4. 推送

```bash
git push origin main
git push origin v1.0.7
```

推送 tag 即触发 CI。在 https://github.com/zhaojun2066/ai-profile-manager/actions 查看 `Build Desktop App` workflow 的实时进度。

#### 5. 验证发布

CI 完成后（约 20-30 分钟），检查：

- [ ] [GitHub Releases 页面](https://github.com/zhaojun2066/ai-profile-manager/releases) 出现新版本
- [ ] Release notes 正确显示了本版本的 commit 变更
- [ ] `download.json` 可访问：`curl -sL https://github.com/zhaojun2066/ai-profile-manager/releases/latest/download/download.json`
- [ ] 桌面 App 内「检查更新」能拉取到新版本

### 完整示例：一次真实的发布

假设当前版本是 `1.0.6`，你在修一个 bug：

```bash
# ── 阶段一：开发 ──
git checkout -b fix/hook-crash main
# ... 改代码 ...
git add .
git commit -m "fix(hook): prevent crash on empty hook config"
git push origin fix/hook-crash

# ── 阶段二：合并（用 PR 或直接 merge）──
git checkout main
git pull origin main
git merge fix/hook-crash
git push origin main
git branch -d fix/hook-crash  # 清理本地分支

# ── 阶段三：发布 ──
# 编辑 desktop/src-tauri/tauri.conf.json  → "version": "1.0.7"
# 编辑 desktop/src-tauri/Cargo.toml       → version = "1.0.7"
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/Cargo.toml
git commit -m "release: v1.0.7"
git tag -a v1.0.7 -m "v1.0.7"
git push origin main
git push origin v1.0.7

# 然后去 https://github.com/zhaojun2066/ai-profile-manager/actions 等结果
```

## 自动化流程说明

推送 tag 后，CI 按以下步骤自动执行：

```
┌─────────────┐     ┌──────────────────┐     ┌──────────────────────┐
│  validate   │ ──▶ │  build (4 jobs)   │ ──▶ │       release         │
│  版本校验    │     │  macOS ARM/Intel  │     │  生成 release notes   │
│  tag == json │     │  Windows x64/ARM64│     │  生成 download.json   │
│  新 > 旧     │     │  (并行构建 ~15min) │     │  创建 GitHub Release  │
└─────────────┘     └──────────────────┘     └──────────────────────┘
```

### 版本校验（validate job）

1. 读取 `tauri.conf.json` 的 `version` 字段
2. 确认 tag 版本 == 配置文件版本（防止忘记同步）
3. 确认新版本 > 上一个 release tag（防止版本号倒退）

校验失败会直接终止，不浪费 CI 时间构建。

### Release Notes 生成

使用 [git-cliff](https://git-cliff.org) 自动解析上次 release 以来的所有 commit，按 conventional commit 类型分组：

| Commit 前缀 | 分组 |
|------------|------|
| `feat:` / `feat(scope):` | 🚀 Features |
| `fix:` / `fix(scope):` | 🐛 Bug Fixes |
| `refactor:` | 🔧 Refactoring |
| `perf:` | ⚡ Performance |
| `docs:` | 📚 Documentation |
| `style:` | 🎨 Styling |
| `test:` | ✅ Testing |
| `chore:` / `ci:` / `build:` | 🧹 Miscellaneous |
| 其他 | 📦 Other |

每条 commit 会带上 7 位短 hash 和 GitHub 链接。Release notes 底部有完整的版本对比链接（`v1.0.6...v1.0.7`）。

### download.json 清单

为 Tauri 自动更新生成的清单文件，结构：

```json
{
  "version": "1.0.7",
  "notes": "## [1.0.7] - 2026-06-09\n### 🚀 Features\n...",
  "platforms": {
    "darwin-aarch64": {
      "url": "https://github.com/.../releases/download/v1.0.7/AI.Profile.Manager_1.0.7_aarch64.dmg",
      "sha256": "abc123..."
    },
    "darwin-x86_64": { "url": "...", "sha256": "..." },
    "windows-x86_64": { "url": "...", "sha256": "..." },
    "windows-arm64": { "url": "...", "sha256": "..." }
  }
}
```

### 手动触发（备用）

如果需要在没有 tag 的情况下手动发布（比如 CI 失败重跑），可以去 Actions → Build Desktop App → Run workflow，填入 version 参数。

也可以填入 `release_notes_override` 来覆盖自动生成的 release notes（比如 hotfix 只改了一行代码，不想显示全部 commit 历史）。

## 本地预览 Release Notes

在打 tag 之前，可以先预览 release notes 会是什么样的：

```bash
# 安装 git-cliff（macOS）
brew install git-cliff

# 预览自上次 tag 以来的所有 commit
git cliff --unreleased

# 或者只看最近 10 条 commit
git cliff --unreleased | head -30
```

## 版本号规范

采用 [Semantic Versioning](https://semver.org) 的三段式：

```
MAJOR.MINOR.PATCH
  │     │     └─ 补丁：bug 修复、小调整
  │     └─────── 次要：新功能、向后兼容的变更
  └───────────── 主要：破坏性变更（目前一直在 1.x）
```

- 目前处于早期开发阶段，MAJOR 保持为 `1`
- 新增功能 → bump MINOR（`1.0.6` → `1.1.0`）
- Bug 修复 / 小改动 → bump PATCH（`1.0.6` → `1.0.7`）
- 重大架构变更 → bump MAJOR（`1.x.x` → `2.0.0`）

## Troubleshooting

### 版本校验失败："version is not newer than latest release"

忘记 bump 版本号，或者版本号没有增大。检查 `tauri.conf.json` 的 version 是否确实比上一个 release tag 大。可以用 `git tag --sort=-version:refname 'v*'` 查看所有已有 tag。

### 版本校验失败："tauri.conf.json version does not match tag"

`tauri.conf.json` 里的版本号和 tag 名不一致。比如 tag 是 `v1.0.7` 但配置文件里还是 `1.0.6`。

### Release notes 是空的

如果 git-cliff 生成空输出（比如只有一个 release commit），会自动 fallback 到 "Release vX.Y.Z"。

长期来看，保持 conventional commit 的书写习惯可以让 release notes 更可读：
- ✅ `feat(agent): add dependency graph builder`
- ✅ `fix(hook): resolve ID collision in hook copy`
- ❌ `update code`
- ❌ `stuff`

### 某个平台构建失败

去 Actions 日志里看具体是哪个平台失败了。常见原因：
- **依赖更新导致编译失败**：Cargo.lock 变化、Tauri 版本升级
- **Windows 特定问题**：路径分隔符、PowerShell 语法
- **macOS Intel 交叉编译**：aarch64 机器上 cross-compile 到 x86_64

如果只有某个平台失败且紧急需要发布，可以临时注释掉 `build-desktop.yml` matrix 中的那个平台。

### 重新发布同一个版本

如果 `v1.0.7` 的 release 已经存在但需要重发（比如构建产物有问题），可以：

1. 修复代码并 commit
2. 删除旧的 tag 和 release：
   ```bash
   git tag -d v1.0.7
   git push origin :refs/tags/v1.0.7
   # 然后在 GitHub 网页上手动删除 Release
   ```
3. 重新打 tag 并推送：
   ```bash
   git tag -a v1.0.7 -m "v1.0.7"
   git push origin v1.0.7
   ```

### 手动触发时没有填 version 参数

如果 `workflow_dispatch` 没填 version，CI 会自动从 `tauri.conf.json` 读取。确保配置文件里的版本号和你想发布的版本一致。
