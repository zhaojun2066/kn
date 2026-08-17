# kn 打包分发 — macOS & iOS

> 适用：`kn` (macOS Desktop) + `kn-ios` (iOS App) | 最后更新：2026-06-17

## 一、总览

| | kn Desktop (macOS) | kn-ios |
|---|---|---|
| 框架 | Tauri v2 | SwiftUI |
| 分发渠道 | GitHub Releases (.dmg) | App Store / TestFlight |
| 签名证书 | Developer ID Application | iOS Distribution |
| 公证 | Apple Notary Service | 不需要（App Store 自动公证） |
| CI | `.github/workflows/build-desktop.yml` | 手动 Archive + Upload |
| 安装方式 | 用户下载 .dmg 拖入 /Applications | App Store 下载 |

---

## 二、kn Desktop — macOS 分发

### 2.1 需要的证书

在 [Apple Developer → Certificates](https://developer.apple.com/account/resources/certificates/list) 创建：

| 证书 | 用途 | 数量 |
|------|------|------|
| **Developer ID Application** | 签名 .app 内所有可执行文件（kn、kn-agent 等） | 1 个 |
| **Developer ID Installer**（可选） | 签名 .pkg 安装包（如果不用 .dmg 用 .pkg） | 0-1 个 |

> Tauri 默认打包为 `.dmg`，仅需 **Developer ID Application** 一个证书。

### 2.2 公证凭证

Apple 公证需要 API 凭证（推荐 App Store Connect API Key，比 app-specific password 更安全）：

```
App Store Connect → Users and Access → Integrations → Keys
  → + → 填名称 "kn-notary"
  → 选 Admin 权限
  → Download .p8（只能一次！）
  → 记下 Issuer ID、Key ID
```

### 2.3 CI 需要的 GitHub Secrets

| Secret | 值 | 说明 |
|--------|-----|------|
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` | 签名身份 |
| `APPLE_CERTIFICATE` | Base64 编码的 .p12 证书文件 | 证书内容 |
| `APPLE_CERTIFICATE_PASSWORD` | .p12 导出时设的密码 | 证书密码 |
| `APPLE_TEAM_ID` | 10 位 | 与 APNs Team ID 相同 |
| `APPLE_NOTARY_KEY` | p8 文件内容（单行） | 公证用，与 APNs 是不同的 key |
| `APPLE_NOTARY_KEY_ID` | 10 位 | 公证 Key ID |
| `APPLE_NOTARY_ISSUER_ID` | UUID 格式 | App Store Connect → Keys 页面顶部 |

### 2.4 导出证书为 CI 可用格式

```bash
# 1. 在 Keychain Access 中找到 Developer ID Application 证书
# 2. 右键 → Export → 选 .p12 格式 → 设密码
# 3. Base64 编码
base64 -i ~/Desktop/kn-signing.p12 | tr -d '\n'
# 把输出贴到 GitHub Secret APPLE_CERTIFICATE
```

### 2.4.1 本地签名构建

本地构建不需要导出 `.p12` 或设置 GitHub Secrets；只要当前 Mac 的“登录”钥匙串已安装带私钥的 `Developer ID Application` 证书即可。

先确认签名身份：

```bash
security find-identity -v -p codesigning
```

然后在 `desktop` 目录带上签名身份构建：

```bash
APPLE_SIGNING_IDENTITY='Developer ID Application: jun zhao (8237PLJ8M5)' \
npm run tauri:build:prod
```

本地签名和公证的完整操作请见 [macOS 本地构建、签名与公证](macOS-本地构建签名与公证.md)。对外发布前仍须完成 Apple Notary Service 公证与 stapler 装订。

### 2.5 当前 CI 状态

已有 `.github/workflows/build-desktop.yml`：
- 触发：tag push `v*`
- 构建：macOS ARM + Intel (universal binary)
- 签名：用 `APPLE_CERTIFICATE` + `APPLE_SIGNING_IDENTITY`
- 公证：`notarytool submit` → staple

需要补充：
- `build-agent.sh` 步骤（先编译 kn-agent → 拷到 resources/）
- `tauri.conf.json` 中 `bundle.resources` 声明

---

## 三、kn-ios — App Store 分发

### 3.1 在 Apple Developer 准备

```
① Certificates, Identifiers & Profiles
   → Identifiers → + → App IDs
     Bundle ID: dev.kn.ios（或你选的）
     ☑️ Push Notifications（APNs 推送需要）

② Certificates → + → iOS Distribution (App Store and Ad Hoc)
   或直接用 Xcode 自动管理（推荐）

③ Profiles → + → App Store
   → 选上面的 App ID + Distribution Certificate
   → Download → 双击安装
```

### 3.2 在 App Store Connect 准备

```
① App Store Connect → Apps → + → New App
   Platform: iOS
   Name: kn
   Bundle ID: dev.kn.ios
   SKU: dev.kn.ios

② 填写基本信息后，状态变为 "Prepare for Submission"

③ 如需 TestFlight 测试：
   App Information → 左侧 TestFlight → 无需额外配置
   上传 build 后自动出现在 TestFlight 中
```

### 3.3 Xcode 项目配置

```
① Xcode → 项目 → Signing & Capabilities
   ☑️ Automatically manage signing
   Team: 选你的 Apple ID
   Bundle Identifier: dev.kn.ios（与上面一致）

② + Capability → Push Notifications（APNs 需要）

③ + Capability → Background Modes（如果需要后台保活）
   不勾也 OK，iOS Phase 1 不强制

④ Info.plist 锁竖屏：
   UISupportedInterfaceOrientations → iPhone:
     ☑️ Portrait（只勾这一个）
```

### 3.4 构建与上传

```bash
# Xcode 操作：
① Product → Scheme → Edit Scheme → Run → Build Configuration → Release
② Product → Archive
③ Organizer 窗口出现后 → Distribute App → App Store Connect → Upload
④ 上传完成后到 App Store Connect 等处理（通常几分钟）
⑤ TestFlight 页出现新 build → 添加测试用户 → 开始测试
```

### 3.5 TestFlight 内测

免费，无需审核。流程：
```
App Store Connect → TestFlight → 选 build → 
  Internal Testing → + 添加测试员（用 Apple ID 邮箱）
  → 测试员在 iPhone 上装 TestFlight App 即可下载
```

### 3.6 CI 自动化（可后续配置）

iOS 打包通过 Xcode Cloud 或 GitHub Actions + `xcodebuild`：

```bash
# 命令行构建（不依赖 Xcode GUI）
xcodebuild archive \
  -workspace kn-ios.xcworkspace \
  -scheme kn-ios \
  -archivePath ./build/kn-ios.xcarchive \
  -destination "generic/platform=iOS"

# 上传到 App Store Connect
xcodebuild -exportArchive \
  -archivePath ./build/kn-ios.xcarchive \
  -exportPath ./build \
  -exportOptionsPlist ExportOptions.plist \
  -allowProvisioningUpdates
```

需要 GitHub Secrets：
| Secret | 值 |
|--------|-----|
| `APPSTORE_CONNECT_KEY` | App Store Connect API Key p8 内容 |
| `APPSTORE_CONNECT_KEY_ID` | Key ID |
| `APPSTORE_CONNECT_ISSUER_ID` | Issuer ID |
| `IOS_DISTRIBUTION_CERTIFICATE` | .p12 Base64 |
| `IOS_DISTRIBUTION_CERTIFICATE_PASSWORD` | .p12 密码 |
| `IOS_PROVISIONING_PROFILE` | .mobileprovision Base64 |

> iOS CI 复杂度远高于 macOS。建议初期手动 Archive → Upload，稳定后再建 CI。

---

## 四、你现在需要做的事情

### macOS Desktop 打包

| 步骤 | 状态 |
|------|------|
| 创建 Developer ID Application 证书 | 🔲 |
| 导出 .p12 + Base64 编码 | 🔲 |
| 创建公证 API Key（App Store Connect → Keys） | 🔲 |
| GitHub Secrets 填 `APPLE_CERTIFICATE` 等 | 🔲 |
| 待 CI 触发后验证 .dmg 可安装 | 🔲 |

### iOS App 分发

| 步骤 | 状态 |
|------|------|
| 注册 App ID（`dev.kn.ios`） | 🔲 |
| 创建 iOS Distribution Certificate | 🔲 |
| 创建 App Store Provisioning Profile | 🔲 |
| App Store Connect 创建 App 记录 | 🔲 |
| Xcode 项目配置 Signing + Push Notifications | 🔲 |
| Archive → Upload → TestFlight 验证 | 🔲 |

---

---

## 四-A、iOS 登录验证码流程

kn-cloud v0.1.1 起，登录接口增加了**算术验证码**机制。iOS App 需适配。

### 触发条件：失败 ≥3 次

```
POST /api/v1/auth/login → response code = 1005 (CAPTCHA_REQUIRED)
```

### API 流程

```
① POST /api/v1/auth/captcha  （无需 JWT）
   → { "captchaId": "a1b2c3d4", "question": "3+5=?",
       "imageBase64": "iVBOR...", "expiresIn": 60 }

② POST /api/v1/auth/login
   Body: { "email": "...", "password": "...",
           "captchaId": "...", "captchaAnswer": "..." }
```

### 错误码

| 错误码 | 含义 |
|--------|------|
| `1005` | CAPTCHA_REQUIRED — 需先获取验证码 |
| `1006` | CAPTCHA_INVALID — 答案错误或 60s 过期 |
| `429` | RATE_LIMITED — 失败≥5次，锁定 15 分钟 |

### iOS 端实现要点

- `imageBase64` 为标准 PNG（130×48），`UIImage(data: Data(base64Encoded:))` 解码
- 验证码用完即毁，答案错误需重新 `POST /captcha` 获取新验证码
- 验证码 60 秒过期，客户端应显示倒计时

---

## 五、证书清单汇总

| 用途 | 证书类型 | 所属 |
|------|---------|------|
| macOS 签名 | Developer ID Application | kn Desktop |
| macOS 公证 | App Store Connect API Key（notary） | kn Desktop |
| APNs 推送 | APNs p8 Key | kn-cloud |
| iOS 签名 | iOS Distribution | kn-ios |
| iOS 上传 | App Store Connect API Key（upload） | kn-ios |

> **注意**：公证 API Key 和 APNs p8 Key 是两把不同的 key，虽然都在 Apple Developer 后台创建但用途互不通用。
