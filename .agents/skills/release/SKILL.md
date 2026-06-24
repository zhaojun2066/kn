---
name: release
description: 交互式发布桌面 App —— 合并分支 → bump 版本 → 预览 release notes → 打 tag 推送
---

# Release Skill

交互式引导发布 AI Profile Manager 桌面应用。每一步都会给你确认选项，不会自动执行危险操作。

## 执行流程

严格按以下顺序，每步完成后等待用户确认再继续。

---

### Step 1: 检查状态

列出当前分支、未提交的更改、以及和远程的同步状态：

```bash
git branch --show-current
git status --short
git fetch origin
git log --oneline origin/main..HEAD 2>/dev/null || echo "已同步"
```

如果当前在 main 分支且工作区干净，跳到 Step 2。
如果有未提交更改，提示先提交。
如果有未推送的 commit，列出来供确认。

---

### Step 2: 确认合并分支

如果当前在 main 分支 → 跳过此步。

如果当前在功能分支（如 `fix/xxx`）→ 显示分支名，用 `AskUserQuestion` 确认：

> 当前分支: `fix/xxx`，是否合并到 main？

选项：
1. **合并到 main** — 执行 `git checkout main && git merge <branch>`
2. **跳过合并** — 已经在 main 上有未合并代码时用
3. **指定其他分支** — 让用户输入分支名

执行合并后，再次检查状态确保合并成功。

---

### Step 3: 选择发布模式

读取当前版本：

```bash
grep '"version"' desktop/src-tauri/tauri.conf.json | head -1 | sed 's/.*"version": "\(.*\)".*/\1/'
```

检查最新 release tag：

```bash
git tag --sort=-version:refname 'v*' 2>/dev/null | head -3
```

用 `AskUserQuestion` 让用户选择发布模式：

> 当前版本: 1.0.6 | 最新 release: v1.0.6
> 本次发布要如何处理版本？

选项：
1. **Patch（如 1.0.6 → 1.0.7）** — Bug 修复、小改动
2. **Minor（如 1.0.6 → 1.1.0）** — 新功能、向后兼容
3. **Major（如 1.0.6 → 2.0.0）** — 破坏性变更
4. **自定义版本号** — 用户手动输入
5. **保持不变（重新构建）** — 版本号不变，仅重新构建/发布（如上次构建失败、产物损坏）

---

#### 路径 A：升级版本（选项 1-4）

正常流程，版本号会变化。把选中的版本号存入 `NEW_VERSION`。继续 Step 4。

---

#### 路径 B：保持不变 / 重新构建（选项 5）

当上次构建产物有问题、某个平台构建失败、或需要更新 release notes 时使用。

检查当前 tag 是否已推送到远程：

```bash
git ls-remote --tags origin "v${CURRENT_VERSION}" 2>/dev/null
```

如果远程 tag 存在，用 `AskUserQuestion` 确认：

> 远程 tag `v1.0.6` 已存在。重新构建需要删除它。是否继续？

确认后执行：

```bash
# 删除远程 tag
git push origin :refs/tags/v1.0.6
# 删除本地 tag（如果存在）
git tag -d v1.0.6 2>/dev/null || true
```

然后提示用户：

> ⚠️ 请手动去 GitHub Releases 页面删除旧 Release：
> https://github.com/zhaojun2066/ai-profile-manager/releases
>
> 完成后回复「已删除」继续。

等用户确认后，跳到 Step 5（重新打 tag 并推送，不修改版本号文件，不创建 version bump commit）。

> **注意**：重新构建不会修改版本号文件，不会创建新的 version bump commit。只是重新打 tag + 推送，触发 CI 重新构建发版。

---

### Step 4: 预览 Release Notes

用 git-cliff 预览 release notes（如果安装了的话）：

```bash
# 先尝试用 brew 安装 git-cliff（如果没装）
if ! command -v git-cliff &>/dev/null; then
  echo "📦 安装 git-cliff..."
  brew install git-cliff
fi

# 预览未发布的 commits
git cliff --unreleased
```

**把输出完整展示给用户**，然后确认：

> 以上是自动生成的 release notes 预览。是否满意？

选项：
1. **满意，继续** — 进入下一步
2. **修改** — 允许用户输入自定义的 release notes（会传给 workflow 的 `release_notes_override`）
3. **取消** — 中止发布流程

---

### Step 5: 确认并执行

汇总显示所有即将执行的操作，做最终确认。

#### 路径 A：升级版本

```
═══════════════════════════════════════
  发布确认（新版本）
═══════════════════════════════════════
  Branch:    fix/xxx → merge → main
  Version:   1.0.6 → 1.0.7
  Tag:       v1.0.7
  Commit:    release: v1.0.7
═══════════════════════════════════════
```

用 `AskUserQuestion` 最终确认。

确认后依次执行：

```bash
# 1. 如果还没在 main，切到 main 并 merge（Step 2 中可能已执行）
# 2. 更新版本号
#    - 用 Edit 工具修改 desktop/src-tauri/tauri.conf.json 的 version
#    - 用 Edit 工具修改 desktop/src-tauri/Cargo.toml 的 version
# 3. 提交
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/Cargo.toml
git commit -m "release: v<VERSION>"
# 4. 打 annotated tag
git tag -a v<VERSION> -m "v<VERSION>"
# 5. 推送
git push origin main
git push origin v<VERSION>
```

#### 路径 B：重新构建

```
═══════════════════════════════════════
  发布确认（重新构建）
═══════════════════════════════════════
  Branch:    main
  Version:   1.0.6（不变）
  Tag:       v1.0.6（删除旧 → 重建）
  Commit:    无需 version bump
═══════════════════════════════════════
```

用 `AskUserQuestion` 最终确认。

确认后依次执行：

```bash
# 1. 如果远程旧 tag 还在，删除它（Step 3 路径 B 中可能已执行）
git push origin :refs/tags/v1.0.6 2>/dev/null || true
git tag -d v1.0.6 2>/dev/null || true
# 2. 不修改版本号文件，不创建 version bump commit
# 3. 重新打 annotated tag
git tag -a v1.0.6 -m "v1.0.6"
# 4. 推送（只推 tag，main 没有新 commit 不用推）
git push origin v1.0.6
```

---

### Step 6: 完成提示

推送成功后显示：

```
✅ 发布流程已启动！

📋 查看构建进度:
https://github.com/zhaojun2066/ai-profile-manager/actions

⏱ 预计 20-30 分钟后自动完成:
- macOS ARM + Intel 构建
- Release notes 自动生成
- GitHub Release 自动创建
```

---

## 安全规则

- **绝不跳过版本校验**：版本号必须等于 tag 版本号
- **绝不 force push**：所有 push 不加 `-f`
- **绝不跳过确认**：Step 5 的最终确认必须等用户明确同意
- **绝不删除已有 tag**：如果 `v<VERSION>` tag 已存在，报错让用户手动处理
- **确保在 main 上操作**：打 tag 和 bump 版本必须在 main 分支执行

## 错误处理

- **merge 冲突**：终止流程，让用户手动解决冲突后重新运行 `/release`
- **tag 已存在**：提示用户用 `git tag -d v<VER>` 删除后重试
- **git-cliff 未安装且 brew 安装失败**：跳过预览，直接显示 git log --oneline 作为替代
- **工作区不干净**：列出来让用户决定是先提交还是放弃
