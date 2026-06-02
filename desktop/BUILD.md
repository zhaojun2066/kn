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

## 跨平台构建

**Tauri 不支持从 macOS 直接交叉编译 Windows/Linux 包。** 因为 Tauri 依赖各平台的原生系统库：

| 目标平台 | 需要的原生库 | 能否从 macOS 交叉编译 |
|---------|------------|-------------------|
| macOS ARM → macOS Intel | 无需额外库 | ✅ 支持 |
| macOS → Windows | WebView2 + MSVC 运行时 | ❌ 不行 |
| macOS → Linux | webkit2gtk + GTK | ❌ 不行 |

**唯一可行的做法是 GitHub Actions（免费）。** 用 CI 在三台不同 OS 的虚拟机上分别构建。

## GitHub Actions 全平台构建

把以下文件放到 `.github/workflows/build.yml`：

```yaml
name: Build All Platforms
on:
  push:
    tags: ['v*']
  workflow_dispatch:             # 允许手动触发

jobs:
  # ── macOS ARM + Intel ──────────────────────
  macos:
    strategy:
      matrix:
        target:
          - aarch64-apple-darwin
          - x86_64-apple-darwin
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '22' }
      - run: rustup target add ${{ matrix.target }}
      - run: npm ci
      - run: cp update/update.prod.json update/update.json
      - run: npm run tauri build -- --target ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: macos-${{ matrix.target }}
          path: src-tauri/target/${{ matrix.target }}/release/bundle/

  # ── Windows x64 ───────────────────────────
  windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '22' }
      - run: npm ci
      - run: cp update/update.prod.json update/update.json
      - run: npm run tauri build
      - uses: actions/upload-artifact@v4
        with:
          name: windows
          path: src-tauri/target/release/bundle/

  # ── Linux x64 ─────────────────────────────
  linux:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '22' }
      - run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev
      - run: npm ci
      - run: cp update/update.prod.json update/update.json
      - run: npm run tauri build
      - uses: actions/upload-artifact@v4
        with:
          name: linux
          path: src-tauri/target/release/bundle/
```

### 使用方式

1. 把上面的 yaml 文件放好，推送到 GitHub
2. 打开 GitHub 仓库 → **Actions** tab
3. **方式一**：在本地打 tag `git tag v1.0.0 && git push --tags`，自动触发
4. **方式二**：在 Actions 页面点 "Build All Platforms" → "Run workflow" 手动触发
5. 构建完成后，在 Action 详情页下载各平台产物（`*.zip`）

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

### Windows

需要代码签名证书（$200-400/年）。在 `tauri.conf.json` 中配置：

```json
"bundle": {
  "windows": {
    "signCommand": "signtool sign /f cert.pfx /p password /t http://timestamp.server.com %1"
  }
}
```

### Linux

无需签名。

## 更新地址配置

```bash
# 生产构建自动使用 update/update.prod.json
npm run tauri:build:prod

# 手动覆盖（本地测试）
echo '{"update_url":"http://localhost:8000/update.json"}' > update/update.json
```

## 版本发布流程

1. 修改 `src-tauri/tauri.conf.json` → `version`
2. 用 GitHub Actions 构建全平台包
3. 下载产物，计算 SHA256：
   ```bash
   # macOS
   shasum -a 256 src-tauri/target/release/bundle/dmg/*.dmg
   # Linux
   sha256sum *.AppImage
   # Windows (PowerShell)
   Get-FileHash -Algorithm SHA256 *.msi
   ```
4. 将各平台 SHA256 填入更新清单（参考 `update/demo.json`）
5. 上传安装包到服务器/CDN
6. 更新服务器上的 `update.json`（`version`、`url`、`sha256`）

## 常见问题

**Q: macOS 打开 DMG 提示"已损坏"？**
未签名的应用被 Gatekeeper 拦截。右键 → 打开，或在系统设置 → 隐私与安全性中点"仍要打开"。

**Q: Windows SmartScreen 报毒？**
需要代码签名证书，或用户点击"更多信息 → 仍要运行"。

**Q: 更新检测不到新版本？**
确认 `update/update.json` 中的 `update_url` 指向正确服务器，且服务器返回的 `version` > 当前版本。
