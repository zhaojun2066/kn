# 构建 & 发布指南

## 本地构建 (macOS)

### 安装交叉编译目标

```bash
# ARM Mac 构建 Intel 包需要的目标
rustup target add x86_64-apple-darwin
```

### 构建命令

| 命令 | 架构 | 产物位置 |
|------|------|---------|
| `npm run tauri:build:prod` | 当前机器架构 | `src-tauri/target/release/bundle/dmg/` |
| `npm run tauri:build:prod:arm` | Apple Silicon (M1/M2/M3) | 同上 |
| `npm run tauri:build:prod:intel` | Intel Mac (x86_64) | 同上 |
| `npm run tauri:build:debug` | 当前架构, debug 模式 | `src-tauri/target/debug/bundle/` |
| `npm run tauri:build:staging` | 同上 | 同上 |

### 日常开发

```bash
npm run tauri:dev        # 热加载开发模式
```

前端热更新，Rust 修改需 Ctrl+C 重新启动。

### 生产发布流程

```bash
# 1. 更新版本号
# 编辑 src-tauri/tauri.conf.json → "version"

# 2. 确认更新地址配置
cat update/update.prod.json

# 3. 构建两个架构
npm run tauri:build:prod:arm
npm run tauri:build:prod:intel

# 4. 计算 SHA256
shasum -a 256 src-tauri/target/release/bundle/dmg/*.dmg

# 5. 将产物上传到服务器，更新 update.json
```

## CI 构建

项目使用 GitHub Actions 构建 macOS ARM + Intel 两个架构。CI 配置在 `.github/workflows/build-desktop.yml`。

## 签名

### macOS

需要 Apple Developer Program（$99/年）。在 `tauri.conf.json` 中配置：

```json
"bundle": {
  "macOS": {
    "signingIdentity": "Developer ID Application: Your Name (TEAMID)"
  }
}
```

构建后公证：
```bash
xcrun notarytool submit src-tauri/target/release/bundle/dmg/*.dmg \
  --apple-id your@email.com \
  --team-id TEAMID \
  --password @keychain:AC_PASSWORD
```

> Tauri 默认自动添加 adhoc 签名（免费），自己用完全正常。分发给别人时，右键 → 打开即可绕过 Gatekeeper。消除警告才需要付费证书。

## 更新地址配置

```bash
# 生产构建自动使用 update/update.prod.json
npm run tauri:build:prod

# 手动覆盖（本地测试）
echo '{"update_url":"http://localhost:8000/update.json"}' > update/update.json
```

## 版本发布流程

1. 修改 `src-tauri/tauri.conf.json` → `version`
2. 用 GitHub Actions 构建 macOS 包
3. 下载产物，计算 SHA256：
   ```bash
   shasum -a 256 src-tauri/target/release/bundle/dmg/*.dmg
   ```
4. 将 SHA256 填入更新清单（参考 `update/demo.json`）
5. 上传安装包到服务器/CDN
6. 更新服务器上的 `update.json`（`version`、`url`、`sha256`）

## 常见问题

**Q: macOS 打开 DMG 提示"已损坏"？**
未签名的应用被 Gatekeeper 拦截。右键 → 打开，或在系统设置 → 隐私与安全性中点"仍要打开"。

**Q: 更新检测不到新版本？**
确认 `update/update.json` 中的 `update_url` 指向正确服务器，且服务器返回的 `version` > 当前版本。
