# KN 文档

本文档集只描述当前已实现的行为。实现、接口或部署方式发生变化时，应在同一改动中更新对应页面；代码、测试和运行配置优先于本文档。

| 文档 | 读者 | 内容 |
| --- | --- | --- |
| [架构](architecture.md) | 所有开发者 | 三仓边界、数据所有权与本地组件关系 |
| [Desktop](desktop.md) | Desktop 开发者 | Tauri 应用、前端、PTY 与本地开发 |
| [Agent](agent.md) | Agent 开发者 | 守护进程、IPC、会话和本地持久化 |
| [Cloud](cloud.md) | Cloud 开发者 | HTTP/WSS 服务、认证和运行边界 |
| [协议](protocol.md) | 跨端开发者 | Cloud 作为协议边界以及变更流程 |
| [发布与上线总手册](../发布与上线总手册.md) | 发布人 | 签名、公证、服务器发布、验收与回滚 |
| [日常发布入口](../RELEASE.md) | 发布人 | 打 tag 生成桌面候选包 |

## 维护约定

- 不保留已完成的方案、阶段计划、审查记录或产品文案作为项目文档；需要追溯时使用 Git 历史。
- 具体接口以声明和测试为准：Desktop 看 `desktop/src-tauri/src`，Agent 看 `agent/src`，Cloud 看 `../kn-cloud`。
- 跨仓改动必须同时检查本仓的 `agent.md`、`cloud.md` 与 `protocol.md`；iOS 的公开协议以 Cloud 的映射层为边界。
- 部署细节集中在《发布与上线总手册》，其他文档只链接它，不复制凭据、服务器或发布步骤。
