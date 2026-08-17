# 桌面构建

## 本地开发

```bash
npm run tauri:dev
```

## 本地包构建

```bash
npm run tauri:build:prod
npm run tauri:build:prod:arm
npm run tauri:build:prod:intel
```

构建始终使用 `src-tauri/runtime-config.json` 中的自有服务器地址。生产构建前必须将真实 API、WSS 和更新接口地址写入该文件。

## 正式发布

正式分发使用 GitHub Actions 产生签名和公证后的 ARM/Intel 候选包，再由发布人手动上传 DMG 到 kn-admin；Admin 会计算并保存 SHA-256。完整操作、Apple 证书、服务器变量、回滚和真机验收见仓库根目录的 `发布与上线总手册.md`。
