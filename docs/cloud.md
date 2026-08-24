# Cloud

Cloud 代码位于相邻仓库 `../kn-cloud`。它是私有服务，由 HTTP API 与 WebSocket 两个 Spring Boot 进程组成，共享 `kn-cloud-common`、MySQL 和 Redis；Nginx 对外提供 HTTPS/WSS。

## 模块与入口

| 模块 | 入口 | 职责 |
| --- | --- | --- |
| `kn-cloud-api` | `/api/v1/*`，开发端口 8080 | 登录、设备绑定、兑换、用户、会话、推送、应用配置与发布信息 |
| `kn-cloud-ws` | `/v1/ws`，开发端口 8081 | 设备/用户连接认证、会话与项目工作台消息中继、心跳、跨节点路由 |
| `kn-cloud-common` | 无 HTTP 入口 | JWT、Redis key、通用响应与异常等无数据库业务依赖的基础设施 |

HTTP 的成功/业务错误使用统一 `{ code, message, data }` 响应；WSS 的两侧协议不同，由 Cloud 转换。服务端的实际控制器、dispatcher、protocol mapper 与测试是接口事实来源。

## 认证和连接

- HTTP 的用户请求使用 JWT；认证白名单以 `AuthFilter` 为准。
- Agent 连接 `/v1/ws` 时携带 `Authorization: Bearer <device_token>`、`X-KN-Role: kn-agent`、`X-KN-Machine-Id` 和 `X-KN-Protocol-Version`。
- iOS 连接同一 WSS 入口，使用用户 access token；Cloud 据角色建立不同的会话上下文。
- Redis 保存在线/心跳、短期绑定和会话协调状态，并通过 Pub/Sub 路由跨节点的 WS 消息；MySQL 保存长期业务数据。

## 本地开发

```bash
cd ../kn-cloud
mvn test
SPRING_PROFILES_ACTIVE=dev mvn -pl kn-cloud-api spring-boot:run
SPRING_PROFILES_ACTIVE=dev mvn -pl kn-cloud-ws spring-boot:run
```

生产部署不由 GitHub Actions 执行。服务器、证书、DMG 发布和回滚以本仓的[发布与上线总手册](../发布与上线总手册.md)为唯一操作手册。
