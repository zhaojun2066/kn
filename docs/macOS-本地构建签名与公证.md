# macOS Desktop 本地构建、签名与公证

适用仓库：`kn`。桌面端通过 Tauri 打包为 DMG，供 Mac App Store 以外的用户安装。

## 目标

发布给外部用户的 DMG 必须完成以下两件事：

1. 使用 `Developer ID Application` 签名。
2. 提交 Apple Notary Service 公证并装订（staple）。

只有签名但未公证的应用，仍可能被 macOS Gatekeeper 拦截。

## 前置条件

### 1. 安装签名证书

在 Apple Developer 创建并下载 `Developer ID Application` 证书，双击 `.cer` 导入“钥匙串访问”的“登录”钥匙串。

在终端确认私钥和证书配对成功：

```bash
security find-identity -v -p codesigning
```

输出应包含类似内容：

```text
Developer ID Application: jun zhao (8237PLJ8M5)
```

如果证书无法展开看到私钥，或上面的命令未列出它，说明创建 CSR 的私钥不在当前 Mac，不能用于签名。

### 2. 构建依赖

在项目根目录准备 Rust、Node.js 和依赖；桌面构建还需要随包提供 `kn-agent`：

```bash
cd /Users/zhaojun/workspace/me/shark/kn
cargo build --release --bin kn-agent
mkdir -p desktop/src-tauri/resources
cp target/release/kn-agent desktop/src-tauri/resources/kn-agent
chmod +x desktop/src-tauri/resources/kn-agent
cd desktop
npm ci
```

## 本地签名构建

在 `desktop` 目录执行：

```bash
APPLE_SIGNING_IDENTITY='Developer ID Application: jun zhao (8237PLJ8M5)' \
npm run tauri:build:prod
```

该身份只在当前命令有效，不会写入代码库或全局 Shell 配置。

DMG 产物通常位于：

```text
desktop/src-tauri/target/release/bundle/dmg/
```

构建后可检查签名：

```bash
codesign --verify --deep --strict --verbose=2 \
  "desktop/src-tauri/target/release/bundle/macos/kn.app"
```

## 本地公证（发布前必须完成）

在 App Store Connect 创建一个用于公证的 API Key：

```text
App Store Connect → Users and Access → Integrations → Keys → +
```

保存以下内容：

- 下载的 `.p8` 私钥文件（只能下载一次；勿提交至 Git）。
- Key ID。
- Issuer ID。

提交 DMG 公证并等待结果：

```bash
xcrun notarytool submit \
  "desktop/src-tauri/target/release/bundle/dmg/kn_*.dmg" \
  --key "/安全路径/AuthKey_你的KeyID.p8" \
  --key-id "你的 Key ID" \
  --issuer "你的 Issuer ID" \
  --wait
```

公证成功后，装订公证票据：

```bash
xcrun stapler staple \
  "desktop/src-tauri/target/release/bundle/dmg/kn_*.dmg"
```

最后验证 Gatekeeper：

```bash
spctl -a -vvv -t open \
  "desktop/src-tauri/target/release/bundle/dmg/kn_*.dmg"
```

预期看到 `accepted`，并显示 `source=Notarized Developer ID`。

## GitHub Actions 与本地构建的区别

本地构建直接使用当前 Mac“登录”钥匙串中的证书，不需要导出 `.p12`。

GitHub Actions 运行在临时机器上，才需要将 `.p12` 和公证 API Key 配置为仓库 Secrets：

- `APPLE_SIGNING_IDENTITY`
- `APPLE_TEAM_ID`
- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_NOTARY_KEY`
- `APPLE_NOTARY_KEY_ID`
- `APPLE_NOTARY_ISSUER_ID`

这些敏感信息禁止写进仓库、`tauri.conf.json`、`.env` 或文档示例的实际值。
