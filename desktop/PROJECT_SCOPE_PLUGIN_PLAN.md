# 项目级插件安装 & Skill 导入 — 改造方案

## 目标

两个入口，行为不同：

| 入口 | Marketplace 注册 | Plugin 安装 | Skill 导入 |
|------|:--:|:--:|:--:|
| **ResourceDrawer（全局抽屉）** | 全局 | 用户级 ✅ 不改 | 用户级 ✅ 不改 |
| **ProjectWorkspace → Resource Tab** | 全局 | **项目级** ← 改造 | **项目级** ← 改造 |

关键原则：**Marketplace 永远全局注册一次，Plugin/Skill 的安装范围由「从哪个入口操作」决定。**

---

## 1. 现状分析

### 1.1 数据流

```
ProjectWorkspace (Resource tab)
  ├── ResourceList                    ← 展示当前项目的 skills/plugins/agents
  │   └── onOpenMarketplace()         → 弹市场弹窗
  ├── ResourceDetail
  └── MarketplaceBrowser              ← 弹窗组件（App.tsx 统一持有）
       ├── handleInstall()            → invoke("install_plugin", {cli, name, marketplace})
       │                                Rust: claude plugin install xxx --scope user  ❌ 写死 user
       ├── handleInstallSkill()       → invoke("install_standalone_skill", {cli, sourcePath})
       │                                Rust: 复制到 ~/.claude/skills/ 或 ~/.codex/skills/  ❌ 硬编码用户级
       ├── handleAddMarketplace()     → invoke("add_marketplace", {cli, source})
       │                                ✅ 全局操作，不改
       └── handleAddRecommended()     → invoke("add_marketplace", ...)
                                        ✅ 同上
```

### 1.2 当前问题

| Rust 命令 | 硬编码行为 |
|-----------|-----------|
| `install_plugin` (Claude) | `--scope user` 写死 |
| `install_plugin` (Codex) | `plugin add` 无 scope，只写 `~/.codex/config.toml` |
| `install_standalone_skill` (Claude) | 复制到 `~/.claude/skills/` |
| `install_standalone_skill` (Codex/Qoder) | 复制到 `~/.codex/skills/` 或 `~/.qoder-cn/skills/` |

Marketplace 管理命令（`add_marketplace`、`remove_marketplace`）不受影响 — 它们永远是全局操作。

---

## 2. 改造方案

### 2.1 核心思路

**Marketplace = 全局，Plugin/Skill 安装 = 项目内**。

当用户从项目 Resource 内打开市场弹窗时，需要把"当前项目"的上下文传下去，让安装操作写到项目目录。

### 2.2 前端改造

#### 2.2.1 MarketplaceBrowser — 增加 projectPath 参数

```typescript
// MarketplaceBrowser.tsx
interface MarketplaceBrowserProps {
  open: boolean;
  onClose: () => void;
  onInstalled: () => void;
  projectPath?: string | null;  // ← 新增：当前项目路径
}
```

- 从 **App.tsx** 打开时（全局 Resource 抽屉）→ `projectPath = null`，行为不变
- 从 **ProjectWorkspace** 打开时 → `projectPath = activeProject.path`

#### 2.2.2 handleInstall — 传入 projectPath

```typescript
const handleInstall = async (p: MarketplacePluginEntry) => {
  // ...
  await invoke("install_plugin", {
    cli: p.cli,
    name: p.name,
    marketplace: p.marketplace,
    projectPath: projectPath ?? null,  // ← 新增参数
  });
};
```

#### 2.2.3 handleInstallSkill — 传入 projectPath

```typescript
const handleInstallSkill = async (overwrite = false) => {
  // ...
  await invoke("install_standalone_skill", {
    cli: skillCli,
    sourcePath: skillSource,
    overwrite,
    projectPath: projectPath ?? null,  // ← 新增参数
  });
};
```

#### 2.2.4 调用链改造

```
App.tsx
  └─ MarketplaceBrowser projectPath={null}     ← 全局抽屉，行为不变

ProjectWorkspace
  └─ MarketplaceBrowser projectPath={project.path}  ← 项目内，安装到项目
```

`ProjectWorkspace` 中需要自己持有 `marketplaceOpen` 状态（而不是复用 App.tsx 的全局 MarketplaceBrowser），或者 App.tsx 传递 `projectPath`。

**推荐方案**：ProjectWorkspace 自己持有 MarketplaceBrowser 实例（就像当前已有 HookStore 一样），传入 `projectPath={project.path}`。

---

### 2.3 Rust 后端改造

#### 2.3.1 install_plugin — 支持 project scope

```rust
// skill_manager/mod.rs
#[tauri::command]
pub async fn install_plugin(
    app_handle: tauri::AppHandle,
    cli: String,
    name: String,
    marketplace: String,
    project_path: Option<String>,  // ← 新增
) -> Result<String, String> {
    // ...
    match cli.as_str() {
        "claude" => {
            let mut args = vec!["plugin", "install", &full];
            match project_path.as_deref() {
                Some(_) => { args.push("--scope"); args.push("project"); }
                None    => { args.push("--scope"); args.push("user"); }
            }
            // => claude plugin install xxx@xxx --scope project
        }
        "codex" => {
            // 步骤 1: 用户级缓存（不变）
            run_cli_plugin_action("codex", &["plugin", "add", &full], ...)?;
            // 步骤 2: 如果有 project_path，写项目级 .codex/config.toml
            if let Some(ref proj) = project_path {
                write_codex_project_plugin_enabled(proj, &name, &marketplace, true)?;
            }
        }
        _ => ...
    }
}
```

#### 2.3.2 install_standalone_skill — 支持 project scope

```rust
#[tauri::command]
pub fn install_standalone_skill(
    cli: String,
    source_path: String,
    overwrite: Option<bool>,
    project_path: Option<String>,  // ← 新增
) -> Result<String, String> {
    match cli.as_str() {
        "claude" => {
            let dest_dir = match project_path {
                Some(ref proj) => PathBuf::from(proj).join(".claude").join("skills"),
                None => claude_skills_dir().ok_or("...")?,
            };
            install_claude_skill_to_dir(src, &dest_dir, overwrite)
        }
        "codex" => {
            let dest_dir = match project_path {
                Some(ref proj) => PathBuf::from(proj).join(".codex").join("skills"),
                None => codex_skills_dir().ok_or("...")?,
            };
            install_codex_style_skill_to_dir(src, &dest_dir, overwrite)
        }
        "qoder" => {
            // Qoder: project-level = .qoder/  (not .qoder-cn/)
            let dest_dir = match project_path {
                Some(ref proj) => PathBuf::from(proj).join(".qoder").join("skills"),
                None => qoder_skills_dir().ok_or("...")?,
            };
            install_codex_style_skill_to_dir(src, &dest_dir, overwrite)
        }
    }
}
```

#### 2.3.3 新增 helper: write_codex_project_plugin_enabled

Codex 没有 `plugin add --scope project` 命令，需要手动写项目 `.codex/config.toml`：

```rust
fn write_codex_project_plugin_enabled(
    project_path: &str,
    plugin_name: &str,
    marketplace: &str,
    enabled: bool,
) -> Result<(), String> {
    let config_path = Path::new(project_path).join(".codex").join("config.toml");
    
    // 如果项目还没有 .codex/ 目录，创建它
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    
    // 读取已有内容（可能文件不存在）
    let mut config: toml::Value = if config_path.exists() {
        let text = fs::read_to_string(&config_path).map_err(|e| format!("读取配置失败: {}", e))?;
        toml::from_str(&text).unwrap_or(toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    
    // 设置 [plugins."name@marketplace"].enabled = true/false
    let key = format!("{plugin_name}@{marketplace}");
    config["plugins"] = config.get("plugins").cloned().unwrap_or(toml::Value::Table(toml::map::Map::new()));
    config["plugins"][&key]["enabled"] = toml::Value::Boolean(enabled);
    
    // 原子写入
    let content = toml::to_string_pretty(&config).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&config_path, content).map_err(|e| format!("写入配置失败: {}", e))?;
    Ok(())
}
```

#### 2.3.4 现有 scan_all 项目级 plugin 扫描 — 补充

`scan_all(project_root)` 已经在扫项目级 skills 和 commands，但没有扫项目级 plugins。需要补充：

```rust
// 在 scan_all 中 project_root 分支增加：

// Claude: 扫 <project>/.claude/settings.json 的 enabledPlugins
if let Some(root) = project_root {
    let proj_settings = root.join(".claude").join("settings.json");
    if let Ok(text) = fs::read_to_string(&proj_settings) {
        if let Ok(settings) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(enabled) = settings.get("enabledPlugins") {
                // 与用户级 claude plugin list 的输出合并显示
            }
        }
    }
}

// Codex: 扫 <project>/.codex/config.toml 的 [plugins]
if let Some(root) = project_root {
    let proj_config = root.join(".codex").join("config.toml");
    if let Ok(text) = fs::read_to_string(&proj_config) {
        if let Ok(config) = toml::from_str::<CodexConfig>(&text) {
            // 合并到 plugin list
        }
    }
}
```

---

## 3. 文件改动清单

### 前端

| 文件 | 改动 |
|------|------|
| `MarketplaceBrowser.tsx` | 新增 `projectPath?: string` prop；install_plugin 和 install_standalone_skill 调用时传入 projectPath |
| `ProjectWorkspace.tsx` | 自己持有 `MarketplaceBrowser` 实例，传 `projectPath={project.path}` |
| `App.tsx` | 全局 `MarketplaceBrowser` 不传 projectPath（保持 null） |

### Rust 后端

| 文件 | 改动 |
|------|------|
| `skill_manager/mod.rs` | `install_plugin` 增加 `project_path` 参数；Claude 改 `--scope`；Codex 额外写项目 config.toml |
| `skill_manager/mod.rs` | `install_standalone_skill` 增加 `project_path` 参数，目标目录改为项目级 |
| `skill_manager/mod.rs` | 新增 `write_codex_project_plugin_enabled` helper |
| `skill_manager/mod.rs` | `scan_all` 补充项目级 plugin 扫描（Claude settings.json + Codex config.toml） |
| `skill_manager/paths.rs` | 可能需要加项目级路径 helper（如 `claude_project_settings`） |
| `lib.rs` | 注册命令签名更新（如果有破坏性变更） |

---

## 4. 不需要改的

| 组件 | 原因 |
|------|------|
| `toolbar → "市场"按钮 → MarketplaceBrowser` | 全局入口，行为不变 |
| `ResourceDrawer → 资源 Tab → MarketplaceBrowser` | 全局抽屉，插件安装到用户级，不改 |
| `ResourceDrawer → install_plugin / install_standalone_skill` | 无 projectPath，行为完全不变 |
| `add_marketplace` / `remove_marketplace` | Marketplace 管理永远是全局操作 |
| `ResourceList` | 不感知安装目标，只管展示数据 |

---

## 5. 项目级 Plugin 的更新 & 删除

### 5.1 更新 (Update)

插件的**实体文件**在用户级缓存（`~/.codex/plugins/cache/` 或 `~/.claude/`），所以更新操作实际上和 scope 无关——更新的是源文件：

| CLI | 更新命令 | 说明 |
|-----|---------|------|
| Claude | `claude plugin update <name>` | 无 scope 区分，更新插件源 |
| Codex | 暂无单独的 plugin update 命令 | 需要通过 marketplace upgrade + 重新 install 或手动替换缓存 |

**桌面 App**：沿用现有的 `update_plugin` 和 `check_updates` 逻辑，不需要区分 scope。

### 5.2 删除 (Uninstall)

项目级删除 = 移除项目对插件的引用，**不删除用户级缓存的插件文件**：

| CLI | 操作 | 效果 |
|-----|------|------|
| Claude | `claude plugin uninstall <name> --scope project` | 从 `<project>/.claude/settings.json` 移除 enabledPlugins 条目 |
| Codex | 编辑 `<project>/.codex/config.toml`，删除 `[plugins."name@marketplace"]` 块 | Codex 不再在此项目中启用该插件 |

**注意**：如果用户也想从用户级完全删除插件（包括缓存），应该在全局 Resource 抽屉中操作。

---

## 6. scan_all 项目级 Plugin 扫描

当前 `scan_all(project_root)` 在 `project_root` 存在时：
- ✅ 扫项目级 skills（Claude / Codex / Qoder）
- ✅ 扫项目级 commands（Claude）
- ❌ **不扫**项目级 plugins

需要补充（在 `skill_manager/mod.rs` 的 `scan_all` 函数 `project_root` 分支中）：

```rust
if let Some(root) = project_root {
    // （现有：skills + commands 扫描）

    // ── 新增：项目级 Plugin 扫描 ──

    // Claude: <project>/.claude/settings.json → enabledPlugins
    let proj_claude_plugins = scan_claude_project_plugins(root);
    plugins.extend(proj_claude_plugins);

    // Codex: <project>/.codex/config.toml → [plugins]
    let proj_codex_plugins = scan_codex_project_plugins(root);
    plugins.extend(proj_codex_plugins);
}
```

`scan_claude_project_plugins` 实现思路：
1. 读 `<project>/.claude/settings.json`
2. 取 `enabledPlugins` 对象（格式：`{"xxx@marketplace": true}`）
3. 对每个 key，调用 `claude plugin list` 获取版本/状态信息
4. 返回 `Vec<PluginEntry>`，标记 `source = "project"`

`scan_codex_project_plugins` 实现思路：
1. 读 `<project>/.codex/config.toml`
2. 解析 `[plugins."name@marketplace"]` 段
3. 返回 `Vec<PluginEntry>`，标记 `source = "project"`

---

## 7. ProjectWorkspace 改造细节

当前 ProjectWorkspace **没有**自己的 MarketplaceBrowser，靠 props `onOpenMarketplace` 冒泡到 App.tsx 统一弹。

改造后：

```tsx
// ProjectWorkspace.tsx
const [marketplaceOpen, setMarketplaceOpen] = useState(false);

// ... render
<MarketplaceBrowser
  open={marketplaceOpen}
  onClose={() => setMarketplaceOpen(false)}
  onInstalled={handleMarketplaceInstalled}
  projectPath={project.path}  // ← 关键
/>
```

同时 ResourceList 内的 `onOpenMarketplace` 回调改为 `() => setMarketplaceOpen(true)`。

App.tsx 全局的 MarketplaceBrowser 保持不变（无 projectPath）。

---

## 8. 各 CLI 安装行为总结

| CLI | 安装位置（项目级） | 实现方式 |
|-----|-------------------|---------|
| Claude | `<project>/.claude/settings.json` → `enabledPlugins` | `claude plugin install xxx --scope project` |
| Claude (skill) | `<project>/.claude/skills/<name>.md` | 文件复制 |
| Codex | `<project>/.codex/config.toml` → `[plugins."name@marketplace"] enabled=true` | 直接写 TOML（无 CLI 命令） |
| Codex (skill) | `<project>/.codex/skills/<name>/SKILL.md` | 文件复制（目录结构） |
| Qoder | `<project>/.qoder/`（项目级用 `.qoder` 不是 `.qoder-cn`） | 同上 |
| Qoder (skill) | `<project>/.qoder/skills/<name>/SKILL.md` | 文件复制 |

### 插件移除（项目级）

| CLI | 操作 |
|-----|------|
| Claude | `claude plugin uninstall xxx --scope project` |
| Codex | 从 `<project>/.codex/config.toml` 删除 `[plugins."xxx"]` 块 |

---

## 9. 注意事项

1. **Codex 的 enable 状态**：`codex plugin add` 把 plugin 缓存到 `~/.codex/plugins/cache/`，同时写 `~/.codex/config.toml`。项目级只写 `.codex/config.toml` 的 `[plugins]` 引用，不需要重复缓存 — Codex 本身就会合并两层 config。

2. **Qoder 项目路径**：项目级 `<project>/.qoder/`，用户级 `~/.qoder-cn/`。这是两个不同的目录名，skill 安装时注意区分。

3. **扫描合并**：项目级 scan 的结果需要和用户级合并，同一个插件可能在两个 scope 都"已安装"，ResourceList 展示时需要区分。

4. **兼容性**：`projectPath` 参数用 `Option<String>`，传 `null` 时行为与现在完全一致，不影响全局抽屉的使用。

5. **撤销操作**：项目级安装的撤销比用户级简单 — 删文件/改配置即可，不需要复杂的事务回滚。

---

## 10. 最终对照

| | ResourceDrawer（全局） | ProjectWorkspace（项目内） |
|---|---|---|
| 打开市场弹窗 | 工具栏 / 资源抽屉按钮 | Resource Tab 内按钮 |
| Marketplace 注册 | 全局 ✅ | 全局 ✅（同一个） |
| Plugin 安装 | 用户级（不改） | **项目级** ← 改造 |
| Skill 导入 | 用户级（不改） | **项目级** ← 改造 |
| Plugin 更新 | 用户级 | 用户级（更新实体文件） |
| Plugin 删除 | 用户级（删缓存） | **项目级**（只删引用） |
| Scan 数据源 | 用户级目录 | 用户级 + 项目级（合并） |

**核心改造点**（5处）：

| # | 文件 | 改什么 |
|---|------|--------|
| 1 | `MarketplaceBrowser.tsx` | 新增 `projectPath` prop |
| 2 | `ProjectWorkspace.tsx` | 自己持有 MarketplaceBrowser，传入 projectPath |
| 3 | `mod.rs:install_plugin` | 加 `project_path` 参数，Claude 改 scope，Codex 写项目 config |
| 4 | `mod.rs:install_standalone_skill` | 加 `project_path` 参数，目标目录切到项目 |
| 5 | `mod.rs:scan_all` | 补项目级 plugin 扫描（Claude settings.json + Codex config.toml） |

---

## 11. 验证方案

实施完成后，逐项验收：

### 11.1 回归验证 — 全局抽屉不受影响

| # | 验证项 | 步骤 | 预期 |
|---|--------|------|------|
| 1 | 全局 Resource 抽屉的市场 | 工具栏 → Resource → 市场按钮 → 安装一个插件 | 插件装到用户级（`claude plugin list` 显示 Scope: user；`~/.codex/config.toml` 中新增 `[plugins]` 条目） |
| 2 | 全局 Skill 导入 | Resource 抽屉 → 市场弹窗 → Skill 导入 | Skill 文件出现在 `~/.claude/skills/` 或 `~/.codex/skills/` |
| 3 | 全局抽屉扫描不变 | 打开全局 Resource 抽屉 | plugins/skills/agents 列表与改造前一致 |

### 11.2 核心验证 — 项目级 Plugin 安装

| # | CLI | 验证项 | 步骤 | 预期 |
|---|-----|--------|------|------|
| 4 | Claude | 项目级安装 | ProjectWorkspace → Resource Tab → 市场 → 装一个插件 | `claude plugin list` 中该插件出现 Scope: project；`<project>/.claude/settings.json` 的 `enabledPlugins` 包含该插件 |
| 5 | Claude | 项目级删除 | 在项目 Resource 中删除该插件 | `claude plugin list` 中 Scope: project 条目消失；`<project>/.claude/settings.json` 该条目被移除；**用户级不受影响** |
| 6 | Codex | 项目级安装 | 同上 | `<project>/.codex/config.toml` 中出现 `[plugins."name@marketplace"] enabled = true`；插件实体仍在 `~/.codex/plugins/cache/` |
| 7 | Codex | 项目级删除 | 在项目 Resource 中删除该插件 | `<project>/.codex/config.toml` 中该条目被移除；**用户级 `~/.codex/config.toml` 不受影响** |

### 11.3 核心验证 — 项目级 Skill 导入

| # | CLI | 验证项 | 步骤 | 预期 |
|---|-----|--------|------|------|
| 8 | Claude | 项目级 skill 导入 | ProjectWorkspace → Resource Tab → 市场 → Skill 导入 | Skill 文件出现在 `<project>/.claude/skills/<name>.md` |
| 9 | Codex | 项目级 skill 导入 | 同上 | Skill 出现在 `<project>/.codex/skills/<name>/SKILL.md` |
| 10 | Qoder | 项目级 skill 导入 | 同上 | Skill 出现在 `<project>/.qoder/skills/<name>/SKILL.md`（注意不是 `.qoder-cn`） |

### 11.4 核心验证 — 项目级 Plugin 显示

| # | 验证项 | 步骤 | 预期 |
|---|--------|------|------|
| 11 | ProjectWorkspace Resource Tab 展示 Plugins | 打开某项目的 Resource Tab | Plugins 分组出现，列出该项目的插件（source: project） |
| 12 | 与用户级插件不混淆 | 同一个插件同时在用户级和项目级安装 | Resource Tab 中项目级插件独立显示，全局抽屉中显示用户级 |
| 13 | 更新功能 | 点击检查更新 | 项目级插件也能正常检查更新（更新的是实体文件，scope 无关） |

### 11.5 边界情况

| # | 验证项 | 步骤 | 预期 |
|---|--------|------|------|
| 14 | 项目无 `.claude/` 目录 | 新项目第一次装插件 | 自动创建 `<project>/.claude/` 目录和 `settings.json` |
| 15 | 项目无 `.codex/` 目录 | 新项目第一次装 Codex 插件 | 自动创建 `<project>/.codex/` 目录和 `config.toml` |
| 16 | 重复安装同一插件 | 项目已有该插件，再次安装 | 提示已存在或更新，不报错 |
| 17 | 全局抽屉仍能用 | 改造后打开全局 Resource 抽屉 | 所有功能正常，marketplace 浏览/安装均到用户级 |
| 18 | TypeScript 编译 | `npx tsc --noEmit` | 无新增类型错误 |
| 19 | 现有测试 | `npx vitest run` | 已有测试全部通过 |
