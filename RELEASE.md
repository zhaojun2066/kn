# KN 桌面端发布

GitHub Actions 只负责构建、Developer ID 签名、公证和生成 `release-candidate-v<version>`。它不会创建 GitHub Release 或上传自有服务器。

完整的服务器、Apple Developer 账号、证书、签名、发布和回滚步骤见 [发布与上线总手册.md](发布与上线总手册.md)。本文件只保留日常入口。

## 发布前

1. 在 `main` 合并所有改动，只修改根目录 `Cargo.toml` 的 `[workspace.package] version`。Agent、桌面 App、官网构建和 CI 都从这里读取版本。
2. 确认 `desktop/src-tauri/runtime-config.json` 已包含生产 `release_api_url`、`cloud_ws_url` 和 `cloud_http_url`。
3. 执行桌面检查：`cargo test -p kn-agent --lib`，`cd desktop && npx tsc --noEmit && npx vite build`，`cargo check -p kn`。
4. 确认 Cloud、kn-admin 和官网将由人工部署到自有服务器；不要为它们配置 GitHub Actions 部署密钥。

## 触发候选包

```bash
git checkout main
git pull origin main
git tag -a v1.2.0 -m "v1.2.0"
git push origin main
git push origin v1.2.0
```

在 Actions 的 `Build Desktop App` 下载 `release-candidate-v1.2.0`。随后按总手册完成：真机验收、将两个 DMG 和 Release Notes 上传 kn-admin、发布、API/官网验证和 24 小时观察。DMG 哈希由 Admin 自动计算。

## 手动构建候选包

Actions 页面可手动运行 `Build Desktop App`，但输入版本必须与仓库版本一致。手动构建仍只产出候选包，不改变服务器公开版本。
