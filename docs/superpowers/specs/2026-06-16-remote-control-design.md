# kn 远程控制 & 用户系统 — 设计文档

> 状态: 设计审查中 | 日期: 2026-06-16

## 目录

1. [概述与目标](#1-概述与目标)
2. [总体架构](#2-总体架构)
3. [组件设计](#3-组件设计)
   - [3.1 云服务 (Java)](#31-云服务-java) — 初期双服务（HTTP + WS），URL 前缀分层，未来拆微服务
   - [3.2 kn Agent (Rust)](#32-kn-agent-rust) — 定位、模块结构、状态机、代码复用、launchd、shell hook
   - [3.3 iOS App (SwiftUI)](#33-ios-app-swiftui) — 架构、Chat/Terminal 模式、键盘工具栏、后台策略
   - [3.4 kn Desktop 适配](#34-kn-desktop-适配)
4. [通信协议](#4-通信协议)
5. [可靠性设计](#5-可靠性设计)
6. [安全设计](#6-安全设计) — 设备绑定、JWT 双 Token、命令白名单、设备防共享（指纹 + 冷却 + 并发检测）
7. [ANSI 解析与 Chat 模式](#7-ansi-解析与-chat-模式)
8. [并发与多客户端](#8-并发与多客户端)
9. [存储与持久化](#9-存储与持久化)
10. [异常场景处理矩阵](#10-异常场景处理矩阵)
11. [实施路线图](#11-实施路线图)

---

## 1. 概述与目标

### 1.1 产品定位

让用户通过手机随时随地操控电脑上的 Claude Code / Codex / Qoder CLI 工具。

**手机是遥控器，不是主力工具。** 发指令、看进度、确认结果、紧急处理——手机上搞定。真正写代码、重构、调试——回到电脑前。

### 1.2 核心需求

| 需求 | 说明 |
|------|------|
| 用户系统 | 注册、登录、会员体系（仅登录用户可远程控制） |
| 远程交互 | 用户在手机上发送消息，AI 在电脑上执行，输出实时回传 |
| 执行状态 | 手机端可看到 AI 的实时输出（进度、diff、错误等） |
| 多设备 | 按会员等级限制绑定设备数，设备指纹防共享，解绑冷却期 |
| 开机自启 | Agent 随系统启动，被杀自动重启 |

### 1.3 技术选型

| 层 | 技术 | 理由 |
|----|------|------|
| 云服务 | Java + Spring Boot + MyBatis Plus + MySQL + Redis | 用户熟悉，生态成熟 |
| 桌面 Agent | Rust (独立二进制，launchd 守护) | 复用 kn 现有 profile/PTY 代码，体积小，启动快 |
| iOS App | SwiftUI (原生) | 最佳性能和用户体验 |
| 通信 | WebSocket over TLS (WSS) + HTTPS | 双向实时通信 |
| 推送 | APNs (Apple Push Notification service) | iOS 后台唤醒 |

---

## 2. 总体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                        用户设备                                   │
│                                                                   │
│  ┌──────────────┐    ┌──────────────────────┐                     │
│  │ iOS App      │    │  macOS 主机            │                    │
│  │ (SwiftUI)    │    │                       │                    │
│  │              │    │  ┌─────────────────┐  │                    │
│  │ Terminal 模式│    │  │ kn Agent (Rust) │  │  ← launchd 守护   │
│  │ 设备管理     │    │  │ daemon          │  │    开机自启        │
│  │ APNs 推送    │    │  │                 │  │    KeepAlive 重启  │
│  └──────┬───────┘    │  │ Session Manager │  │                    │
│         │            │  │ PTY Pool        │  │                    │
│         │            │  │ WS Client       │  │                    │
│         │            │  │ IPC Server      │  │                    │
│         │            │  └────────┬────────┘  │                    │
│         │            │           │            │                    │
│         │            │  ┌────────┴────────┐  │                    │
│         │            │  │ Claude / Codex  │  │  ← AI CLI 工具     │
│         │            │  │ / Qoder CLI     │  │    由 Agent 启动   │
│         │            │  └─────────────────┘  │                    │
│         │            │                       │                    │
│         │            │  ┌─────────────────┐  │                    │
│         │            │  │ kn Desktop App  │  │  ← 可选 GUI 面板   │
│         │            │  │ (Tauri)         │  │    可接入 Agent    │
│         │            │  │                 │  │    或独立运行      │
│         │            │  └────────┬────────┘  │                    │
│         │            └───────────┼───────────┘                    │
│         │                        │                                │
│         │       HTTPS/WSS        │       WSS                      │
│         └──────────┬─────────────┴──────────┘                     │
│                    │                                               │
└────────────────────┼───────────────────────────────────────────────┘
                     │
         ┌───────────┴────────────┐
         │   云服务 (Java)         │
         │                        │
         │  ┌──────────────────┐  │
         │  │ kn-cloud-api     │  │  ← HTTP 服务 (Spring Boot)
         │  │ ┌──────────────┐ │  │    单体，URL 前缀分层
         │  │ │ /auth/*      │ │  │
         │  │ │ /user/*      │ │  │
         │  │ │ /device/*    │ │  │
         │  │ │ /session/*   │ │  │
         │  │ │ /push/*      │ │  │
         │  │ └──────────────┘ │  │
         │  └────────┬─────────┘  │
         │           │            │
         │  ┌────────┴─────────┐  │
         │  │ kn-cloud-ws      │  │  ← WebSocket 服务 (独立进程)
         │  │ /ws              │  │    连接管理/消息中继/心跳
         │  └────────┬─────────┘  │
         │           │            │
         │  ┌────────┴─────────┐  │
         │  │ MySQL + Redis     │  │  ← 两个服务共享
         │  └──────────────────┘  │
         └────────────────────────┘
```

### 2.1 核心数据流

```
1. kn Agent 启动 → 用 device_token 登录云端 WSS → 保持长连接
2. iOS App 登录 → 获取 JWT → 连接云端 WSS
3. 用户发消息 → iOS → WSS → 云服务路由 → Agent → PTY → AI CLI
4. PTY 输出 → Agent → WSS → 云服务 → iOS (流式推送)
5. AI 执行完成 / 需要确认 → 云服务 → APNs → iOS 推送通知
```

### 2.2 代码仓库划分

三个独立 repo，按开源策略分开：

```
github.com/zhaojun2066/
├── kn/                # 公开  — CLI (Python) + Desktop (Tauri) + Site + Agent (Rust)
├── kn-cloud/          # 私有  — Java 云服务 (kn-cloud-api + kn-cloud-ws + kn-cloud-common)
└── kn-ios/            # 私有  — SwiftUI App
```

| repo | 可见性 | 内容 |
|------|--------|------|
| `kn` | 公开 | `bin/`, `lib/`, `desktop/`, `site/`, `shell/`, `docs/`（含本设计文档）, Agent 代码 |
| `kn-cloud` | 私有 | Spring Boot 服务、鉴权逻辑、限流策略、DB schema、APNs 推送实现 |
| `kn-ios` | 私有 | SwiftUI 终端、WSS 客户端、Keychain、APNs 注册 |

**跨 repo 协作**：
- 设计文档在公开 `kn/docs/`，不含密钥/定价等敏感信息，是三方协作的唯一权威入口
- Agent 连接云服务的唯一凭证是 `wss://api.knshark.com` 域名 + 协议格式（设计文档中公开定义）
- `kn-cloud` 和 `kn-ios` 各自内部可有私有的实现细节文档

---

## 3. 组件设计

### 3.1 云服务 (Java)

#### 3.1.1 模块划分

**初期架构（当前阶段）**：两个独立服务，在私有 repo `kn-cloud` 中，按需拆分。

```
kn-cloud/                       # 私有 repo
├── kn-cloud-common/           # 共享模块（model, mapper, util）
│
├── kn-cloud-api/              # HTTP 服务（Spring Boot 单体，模块化内部结构）
│   ├── AuthModule/            ← /api/v1/auth/*   (controller→service→mapper)
│   ├── UserModule/            ← /api/v1/user/*   用户信息、会员、密码
│   ├── DeviceModule/          ← /api/v1/device/* 设备列表、解绑、profiles
│   ├── SessionModule/         ← /api/v1/session/*  会话历史、消息查询
│   └── PushModule/            ← /api/v1/push/*   APNs 注册触发
│
│   关键 REST 端点：
│   POST   /api/v1/auth/register           注册（公开）
│   POST   /api/v1/auth/login              登录（公开）
│   POST   /api/v1/auth/refresh            刷新 access_token + 轮换 refresh_token（公开）
│   POST   /api/v1/device/bind-init        绑定初始化：Agent 请求临时 code（公开 + 限流）
│   POST   /api/v1/device/bind-confirm     iOS 扫码确认绑定（需 JWT）
│   GET    /api/v1/device/list             iOS 获取用户设备列表（需 JWT）
│   POST   /api/v1/device/unbind           iOS 解绑设备（需 JWT）
│   GET    /api/v1/device/{id}/profiles    iOS 获取设备 profile 列表（需 JWT，不含 API Key）
│   POST   /api/v1/user/redeem            iOS 端输入卡密激活会员（需 JWT）
│   GET    /api/v1/session/list            会话历史（需 JWT）
│   GET    /api/v1/session/{id}/messages   会话消息（需 JWT）
│
│   未来拆分：AuthModule → auth-service.jar，Nginx location /api/v1/auth/* → upstream auth
│
└── kn-cloud-ws/               # WebSocket 服务（独立进程）
    ├── 连接管理 + 鉴权
    ├── 消息路由（iOS ↔ Agent）
    ├── 心跳检测
    ├── 离线消息缓存 (Redis)
    └── /ws/*                   ← 单一 WebSocket endpoint
```

**URL 前缀规划**：初期 HTTP 服务按前缀区分，未来量上来后，每个前缀独立部署为微服务。

```
             初期                                 未来
┌──────────────────────────────┐    ┌──────────────────────────────┐
│ Nginx (reverse proxy)        │    │ Nginx (reverse proxy)        │
│                              │    │                              │
│ /api/v1/*  → kn-cloud-api    │    │ /api/v1/auth/*    → auth-svc │
│ /ws/*      → kn-cloud-ws     │    │ /api/v1/user/*    → user-svc │
└──────────────────────────────┘    │ /api/v1/device/*  → device-svc│
                                    │ /api/v1/session/* → session-svc│
┌──────────────────────────────┐    │ /api/v1/push/*    → push-svc  │
│ kn-cloud-api (单体)           │    │ /ws/*             → ws-svc    │
│                              │    └──────────────────────────────┘
│ AuthFilter (OncePerRequest)  │
│  ├─ /auth/*   → 放行         │
│  ├─ 其他所有   → JWT 校验     │
│  └─ /device/* → + 会员检查   │
│                              │
│ controller → service → mapper│
└──────────────────────────────┘
```

**关键约定**：
- HTTP 和 WebSocket 从一开始就分两个进程，互不影响
- **Nginx 做反向代理**，不引入 Spring Cloud ws_node，配置简洁
- **鉴权用 Spring Filter**（`OncePerRequestFilter`），根据 URL 前缀执行不同策略，不依赖网关
- HTTP 内部按**模块**组织，每个模块内再分层（controller → service → mapper），模块间通过 service 接口调用。这比纯 package 分层更重一些，但未来拆微服务时只需把对应模块目录提成独立 jar + Nginx upstream
- WebSocket 服务不依赖 HTTP 服务，直接查 Redis/MySQL
- 共享的 model、mapper、工具类抽到 `kn-cloud-common` 模块
- 未来拆分时，只需把对应 package 提出来变成独立 jar + Nginx `location` 指向新 upstream 即可

**WebSocket 多实例路由**：

初期单实例部署，但路由层从一开始就支持水平扩展。核心机制：每个 WS 实例启动时生成唯一 `ws_node_id`（如 `ws-1`、`ws-2`），通过 Redis Pub/Sub 实现跨实例消息中继：

```
iOS 连接 ws-1 ──→ 发消息给 Agent ──→ 查 Redis ws:device:{device_id} → ws_node_id = ws-2
  → 消息发布到 Redis Pub/Sub channel ws:relay:ws-2
  → ws-2 订阅自己的 channel → 收到消息 → 本地 WebSocketSession 投递
```

- 单实例（v1）：`ws_node_id = ws-1`，ws_node 订阅 `ws:relay:ws-1`，发消息到自己的 channel，自己收到自己投递——零开销
- 多实例（v2）：ws_node 除了订阅自己的 channel，还订阅 `ws:relay:all` 作为广播通道。Nginx `/ws` location 按 `ip_hash` 做 sticky session
- iOS 和 Agent 可能连到不同实例，Redis Pub/Sub 保证了无论谁连到哪个实例，消息都能送达

**AuthFilter 设计**：

```java
// 在 kn-cloud-api 中，单一 Filter，按 URL 前缀执行不同策略
@Component
public class AuthFilter extends OncePerRequestFilter {

    // 无需鉴权的路径
    private static final List<String> PUBLIC_PATHS = List.of(
        "/api/v1/auth/register",
        "/api/v1/auth/login",
        "/api/v1/auth/refresh",
        "/api/v1/device/bind-init"      // Agent 请求绑定码（限流由 Nginx 控制）
    );

    @Override
    protected void doFilterInternal(HttpServletRequest request,
                                    HttpServletResponse response,
                                    FilterChain chain) {
        String path = request.getRequestURI();

        // 1. 公开路径 → 直接放行
        if (matchesPublic(path)) {
            chain.doFilter(request, response);
            return;
        }

        // 2. 其他所有路径 → JWT 校验
        String token = extractToken(request);
        if (token == null || !jwtService.validate(token)) {
            response.setStatus(401);
            return;
        }

        // 3. 将会员/设备信息写入 request context
        UserContext ctx = jwtService.parse(token);
        request.setAttribute("userContext", ctx);

        // 4. 特定路径额外检查
        if (path.startsWith("/api/v1/device/bind")) {
            // 设备绑定 → 检查会员等级 + 设备数上限
            if (!deviceService.canBind(ctx.getUserId())) {
                response.setStatus(403);
                writeJson(response, DeviceError.deviceLimitReached());
                return;
            }
        }

        chain.doFilter(request, response);
    }
}
```
- 未来拆分时，只需把对应 package 提出来变成独立 jar + 配 Nginx 路由即可

**REST API 版本策略**：采用 URL 前缀版本号（`/api/v1/`）。WSS 协议有独立的 `protocol_version` 协商（§4.2），但 REST API 无类似机制。约束如下：

- breaking change（删除/修改已有字段语义）→ 升级前缀为 `/api/v2/`，同时保留 `/api/v1/` 至少一个发布周期
- 新增端点 / 新增可选字段 → 不升级版本号，向前兼容
- 客户端升级前，服务端需同时支持新旧两套 API

#### 3.1.2 会员等级与权限

初期只按**会员等级**控制设备数，不按运行时长计费。后期可扩展更细粒度的时长套餐。

| 等级 | 最多绑定设备 | 同时在线会话 | 付费方式 |
|------|------------|------------|---------|
| Trial | 1 台 | 1 个 | 免费 1 个月，到期停用 |
| Pro | 3 台 | 3 个 | 月付 / 年付 |
| Enterprise | 10 台 | 不限 | 年付，线下谈 |

- 新用户注册即获得 1 个月 Trial，到期后需付费升级
- 到期处理：
  - **提前 1 天**：iOS 推送通知 + App 内横幅提示 "您的试用/会员即将到期"
  - **到期时**：进入 24h 缓冲期，不杀已有 session，不踢 WSS 连接
  - **缓冲期内**：禁止创建新 session + 禁止绑定新设备，但已有连接和会话继续运行
  - **24h 缓冲期过后**：云端强制断开 Agent WSS → 所有活跃 session 终止 → `kn_session` 标记 `failed`
  - 允许解绑旧设备，但到期用户（含缓冲期内）不能再绑新设备
  - Pro/Enterprise 同理：过期 → 24h 缓冲 → 断连。续费后即时恢复。
  - **有效期从购买日起算**，不是首次使用日。买完不用也会到期。
  - 云端定时任务每天检查 `trial_expires_at` 和 `membership_expires_at`
- 不限时长，买了就能用
- 超出设备数 → 拒绝绑定，提示升级

**会员配置方式**：初期写配置文件，后期迁到管理后台。

```yaml
# kn-cloud-api/src/main/resources/membership-config.yml
# 所有可配置项在此文件，改后重启即生效
membership:
  grace_period_hours: 24          # 到期缓冲期
  expire_warn_hours: 24           # 提前多久提示用户（与缓冲期同时开始）

  tiers:
    trial:
      name: "Trial"
      max_devices: 1
      max_concurrent_sessions: 1
      trial_days: 30
    pro:
      name: "Pro"
      max_devices: 3
      max_concurrent_sessions: 3
      price:
        monthly: 1999             # 单位：分（¥19.99）
        yearly: 19990             # 年付 ≈ 月付 × 10
    enterprise:
      name: "Enterprise"
      max_devices: 10
      max_concurrent_sessions: -1
      price: null                 # 线下谈，不在此配置
```

**演进路径**：

```
初期（当前）                        后期
┌──────────────────┐           ┌──────────────────┐
│ membership-config.yml │  →    │ MySQL: kn_membership │
│ 纯会员制，不限时长    │       │ _tier                 │
│                      │       │                      │
│ 改配置 = 重启服务     │       │ + 管理后台 CRUD       │
│                      │       │ + 可选时长套餐扩展     │
└──────────────────┘           └──────────────────┘
```

启动时加载为 `MembershipConfig` Bean，运行时不变，改配置需重启（初期够用）。后期迁到 DB 后，加一个 `refresh()` 方法 + 管理后台改完即时生效。

**付费与卡密激活**：走 C 方案——iOS App 内只兑换，不引导购买。购买入口在 kn Desktop 和官网。

```
用户旅程：
  ① 下载 kn Desktop → 自动装 Agent → 📡 绑定（扫码）
  ② Trial 30 天免费
  ③ 即将到期 → Desktop 📡 面板点 [升级 Pro] → 打开浏览器购买页
     → 微信/支付宝支付 → 卡密平台自动发码到邮箱/页面
  ④ 激活卡密（任选一条路径）：

  路径 A — Desktop 输入：
    Desktop 输入框 "KN-{hex}"
      → IPC → Agent → WSS 发 {type:"redeem", code:"KN-{hex}"}
      → 云端通过 device_token 查出 user_id
      → 校验 code 有效且未用 → UPDATE kn_user SET membership='pro',
        membership_expires_at = NOW() + code.duration_days
      → UPDATE kn_redeem_code SET used_by=user_id, used_at=NOW(),
        redeem_source='desktop'
      → WSS 回复 {type:"redeem_result", ok:true, plan:"pro_monthly"}
      → Desktop 面板显示 🟢 Pro 会员

  路径 B — iOS 输入：
    iOS "输入卡密" 弹框 → 输入 "KN-{hex}"
      → POST /api/v1/user/redeem {code}（需用户 JWT）
      → 云端通过 JWT 查 user_id
      → 校验 → 更新 membership
      → UPDATE kn_redeem_code SET used_by=user_id, used_at=NOW(),
        redeem_source='ios'
      → 返回 {ok:true}
      → iOS 刷新 → 功能解锁

  ⑤ Desktop 和 iOS 都实时可见 Pro 已激活
```

**卡密全生命周期**：三个角色分工明确——

```
① 生成（kn 服务端自控）
  Java 工具 GenerateCodes → AES-256-GCM 加密生成卡密 → INSERT SQL → 手动导入
  格式: KN-{hex_ciphertext}（约 48 字符）
  加密内容: {plan}|{duration_days}|{timestamp}|{nonce}
  只有持有 kn 服务端密钥（AES_KEY 环境变量）才能生成有效卡密，无法被猜测或伪造

② 销售（第三方平台）
  - kn 把生成的卡密批量上传到淘宝/微信小店/卡密平台
  - 用户付款后平台自动发码
  - 平台**不参与**验证，只做分销

③ 验证（kn 云端自控）
  - 用户在 Desktop 或 iOS 输入卡密
  - kn 云端用 AES-256-GCM 解密卡密 → 提取 plan + days → 验证加密签名
  - 查 kn_redeem_code 表：code 存在 且 used_by IS NULL → 有效
  - 更新 kn_user.membership + membership_expires_at
  - 标记 code 已用（used_by + used_at + redeem_source）
```

**为什么不纯随机**：纯随机码可以被批量爆破（试错成本低）。加密后每个码自包含加密签名，即使第三方平台泄露了未使用的卡密，没有 AES 密钥也无法生成新的有效卡密。

卡密验证接口已在 §4.3 定义为 `redeem` / `redeem_result` 消息类型（WSS）和 `POST /api/v1/user/redeem`（REST）。生成工具见 `kn-cloud/tools/GenerateCodes.java`。

**卡密激活 vs 扫码绑定**：两个独立流程，但共享同一层信任。

| | 绑定 | 卡密激活 |
|------|------|---------|
| 目的 | 建立 user ↔ device 关联 | 升级会员等级 |
| 凭证 | bind_code（临时）→ device_token（永久） | 兑换码（一次性） |
| 信任基础 | iOS 扫码 + 用户 JWT | device_token 已有的 user 关联 |
| Desktop 角色 | 展示二维码 + 中转绑定结果 | 中转卡密激活请求 |
| Desktop 需要登录吗 | 不需要 | 不需要 |

#### 3.1.3 数据库表设计

```sql
-- 用户表
CREATE TABLE kn_user (
    id          BIGINT PRIMARY KEY AUTO_INCREMENT,
    email       VARCHAR(255) NOT NULL UNIQUE,
    phone       VARCHAR(20) DEFAULT NULL,       -- 手机号，后续验证码登录用
    password    VARCHAR(255) NOT NULL,  -- bcrypt hash
    nickname    VARCHAR(100),
    membership  VARCHAR(20) DEFAULT 'trial',  -- trial, pro, enterprise
    trial_expires_at DATE,                     -- 试用到期日（注册 +30 天）
    membership_expires_at DATE,                -- 付费会员到期日（Pro/Enterprise，续费后更新）
    status      VARCHAR(20) DEFAULT 'active', -- active, expired, disabled
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX idx_email (email),
    INDEX idx_membership (membership)
);

-- 设备表
CREATE TABLE kn_device (
    id          BIGINT PRIMARY KEY AUTO_INCREMENT,
    user_id     BIGINT NOT NULL,
    device_name VARCHAR(200),    -- 用户自定义名称 "办公室 Mac Studio"
    hostname    VARCHAR(255),    -- 系统 hostname
    os_version  VARCHAR(100),    -- macOS 15.0
    agent_version VARCHAR(50),   -- Agent 版本号
    
    -- 设备指纹（防 token 拷贝，用 macOS 唯一硬件 ID）
    machine_id  VARCHAR(255) NOT NULL,   -- IOPlatformUUID（存 NVRAM，仅全盘抹除后变化）
    
    device_token VARCHAR(512) NOT NULL UNIQUE,  -- 长期凭证（见下方说明）
    status      VARCHAR(20) DEFAULT 'online',  -- online, offline, paused
    last_seen   DATETIME,
    
    -- 解绑冷却（防频繁换设备）
    unbound_at  DATETIME,            -- 上次解绑时间
    
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_user_id (user_id),
    INDEX idx_machine_id (machine_id)
);

-- device_token 说明：
-- 这是 Agent 的"机器身份证"，绑定设备时由云端签发，Agent 存本地。
-- Agent 是后台守护进程，没有 UI，无法每次输入密码，所以用 device_token 代替用户凭证连接 WSS。

-- ┌─ 生成 ───────────────────────────────────────────────────────┐
-- │ 1. Agent 通过 HTTP POST /bind-init 请求临时 code { machine_id }
-- │ 2. 云端生成 bind_code (6位数字) → Redis TTL 5min → 返回给 Agent
-- │ 3. Agent 展示二维码 (含 bind_code)，随后用 code+machine_id 建立 WSS 临时连接
-- │ 4. iOS 扫码 POST /bind-confirm { code, JWT } → 云端验证通过
-- │ 5. 云端生成: device_token = SHA256(user_id + machine_id + random(32) + now)
-- │ 6. 云端写入 kn_device 表 + 通过 WSS 发 bind_result { device_token } 给 Agent
-- │ 7. Agent 收到后写入 ~/.kn/agent/device_token (0600)，切换为正式 WSS 连接
-- │ 8. Agent 通知 Desktop "绑定成功"，二维码弹窗关闭
-- └──────────────────────────────────────────────────────────────┘

-- ┌─ 存储 ───────────────────────────────────────────────────────┐
-- │ Agent 端: ~/.kn/agent/device_token  (0600, 明文, 仅一行字符串)
-- │ 云端:    kn_device.device_token       (MySQL, 与 machine_id 绑定)
-- └──────────────────────────────────────────────────────────────┘

-- ┌─ 验证 (Agent 每次 WSS 连接) ─────────────────────────────────┐
-- │ 连接请求: wss://api.knshark.com/v1/ws (Authorization: Bearer <device_token>, X-KN-Machine-Id: <machine_id>)
-- │                                                                 
-- │ 云端验证链路:                                                    
-- │   ① device_token 在 kn_device 表中存在?                         
-- │       └─ 不存在 → 403 Forbidden                                 
-- │   ② device_token 关联的用户 status = active?                    
-- │       └─ disabled/expired → 403 (会员已过期)                     
-- │   ③ device_token 关联的 machine_id 匹配?                        
-- │       └─ 不匹配 → 403 (token 被拷贝到别的机器 或 设备抹盘重装)   
-- │   ④ 同一 token 是否有其他活跃 WSS 连接?                          
-- │       └─ 有 + IP 不同 → 踢旧连接 + 告警                         
-- │   ⑤ 全部通过 → 建立 WSS，写入 Redis device:conn + device:online  
-- └──────────────────────────────────────────────────────────────┘

-- ┌─ 丢失场景与恢复 ─────────────────────────────────────────────┐
-- │ 文件被误删 → Agent 连不上 → Desktop 显示橙点 → 用户重新绑定    │
-- │ 磁盘损坏   → 同上                                             │
-- │ 卸载 Agent → launchd uninstall 清掉整个目录 → 重装后重新绑定   │
-- │                                                               │
-- │ 重新绑定 = 走完整流程，生成全新 device_token，旧的失效         │
-- │ token 泄露 → 攻击者拷走 token 文件但 machine_id 不同 → 验证    │
-- │   失败被拒。仅当攻击者还获取了同一台机器的 IOPlatformUUID 才    │
-- │   能通过，但这意味着他已经有该机器的 root 权限                  │
-- └──────────────────────────────────────────────────────────────┘

-- 安全边界：仅用于 WSS 连接鉴权，不能调用 HTTP API（HTTP 必须用用户 JWT）。

-- 会话表
-- 
-- session 标识双身份设计：
--   id (BIGINT)     → DB 内部关联用（自增主键，高效 JOIN）
--   session_nid     → WSS 协议层标识（s_ + 12位 nanoid，全局唯一，不可猜测）
-- 云端 WSS 消息路由流程：iOS 发 {session_id: "s_Vh4Kz8mPxQ2n"} 
--   → 云端查 session_nid → 获取 device_id → 转发给对应 Agent 的 WSS
-- 自增 id 不暴露给客户端，避免被枚举
CREATE TABLE kn_session (
    id          BIGINT PRIMARY KEY AUTO_INCREMENT,
    session_nid VARCHAR(20) NOT NULL UNIQUE,  -- WSS 协议层 ID: s_ + 12位 nanoid
    user_id     BIGINT NOT NULL,
    device_id   BIGINT NOT NULL,
    tool        VARCHAR(20) NOT NULL,  -- claude, codex, qoder
    profile     VARCHAR(100),          -- profile 名称
    cwd         VARCHAR(500),          -- 工作目录
    source      VARCHAR(10) DEFAULT 'local',  -- ios / local / desktop（谁发起的）
    started_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    ended_at    DATETIME COMMENT '会话结束时间（null 表示进行中）',
    INDEX idx_session_nid (session_nid),
    INDEX idx_user_id (user_id),
    INDEX idx_device_id (device_id),
    CONSTRAINT chk_source CHECK (source IN ('ios', 'local', 'desktop'))
);

-- 消息表
-- 存的是"谁在什么时候做了什么"，不是终端屏幕内容。
-- PTY 原始 ANSI 输出不存 MySQL（量太大），只存 Agent 本地 output.log（7 天）。
CREATE TABLE kn_message (
    id          BIGINT PRIMARY KEY AUTO_INCREMENT,
    session_id  BIGINT NOT NULL,
    seq         BIGINT NOT NULL,   -- 会话内序号，单调递增
    direction   VARCHAR(10) NOT NULL CHECK (direction IN ('inbound', 'system')),  -- inbound (用户→AI), system (系统事件)
    msg_type    VARCHAR(30) NOT NULL CHECK (msg_type IN ('input', 'ctrl', 'system')),  -- input, ctrl, system
    src         VARCHAR(10) NOT NULL DEFAULT 'local',  -- 输入来源: ios / local / desktop
    content     TEXT,               -- 用户输入的文本 / 系统事件描述
    created_at  DATETIME(3) DEFAULT CURRENT_TIMESTAMP(3),
    INDEX idx_session_seq (session_id, seq),
    INDEX idx_session_time (session_id, created_at)
);

-- 消息类型：
--   inbound/input   → 用户输入: "重构 auth.ts"
--   inbound/ctrl    → 控制信号: "ctrl_c"
--   system/created  → 会话创建
--   system/ended    → 会话结束 (completed / cancelled / failed)
--   system/interrupted → Agent crash 导致中断
--
-- src 字段说明：标记输入来源，用于多客户端场景下区分谁发了什么
--   ios      → iOS App 远程输入
--   local    → macOS 本地终端 (shell hook / Desktop Terminal)
--   desktop  → kn Desktop GUI 面板输入

-- 注意：没有 outbound 方向。PTY 输出不经过此表，直接流式推送给客户端后丢弃。
-- iOS 如果要看历史输出，通过 Agent IPC 读本地 output.log，不查 MySQL。

-- 卡密表（kn 服务端自生成，批量导入后由第三方销售）
CREATE TABLE kn_redeem_code (
    id        BIGINT PRIMARY KEY AUTO_INCREMENT,
    code      VARCHAR(64) NOT NULL UNIQUE,  -- 兑换码，格式 KN-{AES-256-GCM密文hex}（约48字符，加密 {plan}|{days}|{ts}|{nonce}）
    plan      VARCHAR(20) NOT NULL,         -- pro_monthly / pro_yearly
    duration_days INT NOT NULL,             -- 有效天数
    platform_source VARCHAR(50),            -- 卡密发售平台（淘宝/微信小店/卡密平台等）
    redeem_source VARCHAR(20),              -- 兑换来源: desktop / ios（当前使用）
    -- 预留字段（暂不使用）：
    --   redeem_os VARCHAR(20)             -- 兑换操作系统: windows / mac / ios / android
    used_by   BIGINT,                       -- 兑换用户 user_id
    used_at   DATETIME,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_code (code)
);

-- APNs 推送 Token 表 — 每个 iOS 设备一个 token，一个用户可有多个设备
CREATE TABLE kn_push_token (
    id           BIGINT PRIMARY KEY AUTO_INCREMENT,
    user_id      BIGINT NOT NULL,
    device_token VARCHAR(256) NOT NULL,  -- Apple APNs 颁发的推送 token（16进制字符串）
    is_active    BOOLEAN DEFAULT TRUE,   -- 失效后标记 false，下次注册时更新
    updated_at   DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE INDEX idx_token (device_token),
    INDEX idx_user_id (user_id)
);

-- device_token 生命周期:
--   获取: iOS App 启动 → registerForRemoteNotifications → didRegisterForRemoteNotificationsWithDeviceToken
--         → POST /api/v1/push/register {device_token} → INSERT 或 UPDATE is_active=true
--   失效: 用户卸载重装 App / 设备抹掉 → 新 token 覆盖旧记录
--         APNs 返回 410 Gone (BadDeviceToken) → 标记 is_active=false

-- 会话生命周期（kn_session + kn_message 的写入时机）：

-- 所有会话统一由 Agent 上报，不论谁发起。两条路径：

-- ┌─ 路径 A: iOS 远程发起 ─────────────────────────────────────┐
-- │ iOS → 云端 WSS → Agent → spawn PTY → 成功 → 上报 session_created
-- └────────────────────────────────────────────────────────────┘

-- ┌─ 路径 B: macOS 本地发起 ───────────────────────────────────┐
-- │ 终端敲 ai claude xxx → shell hook → Agent IPC → spawn PTY    │
-- │   → 成功 → Agent 通过 WSS 上报 session_created              │
-- │ kn Desktop → Agent IPC → spawn PTY                          │
-- │   → 成功 → Agent 通过 WSS 上报 session_created              │
-- └────────────────────────────────────────────────────────────┘

-- ① 创建会话 — 异步确认模式（去中心化 session_id，谁创建谁生成）
--
--  session_id 格式: "s_" + 12位 nanoid (url-safe base62, 62^12≈3.2×10^21)
--  由发起方（iOS / Agent / Desktop）本地生成，无需云端协调。全局唯一，碰撞概率可忽略。
--  MySQL UNIQUE 约束兜底——万一碰撞，云端返回 error，发起方换一个即可。
--
--  ┌─ 路径 A: iOS 远程发起 ───────────────────────────────────┐
--  │ iOS 本地生成 session_id → 发 start_session {session_id}    │
--  │   → 云端校验格式（s_ + 12位）→ 写 Redis pending          │
--  │   → 转发 Agent → Agent spawn PTY → 上报 session_created   │
--  └─────────────────────────────────────────────────────────┘
--
--  ┌─ 路径 B: macOS 本地发起 (Shell/Desktop) ──────────────────┐
--  │ Shell/Desktop → Agent IPC → Agent 本地生成 session_id     │
--  │   → spawn PTY → 通过 WSS 上报 session_created             │
--  └─────────────────────────────────────────────────────────┘
--
--  详细流程（以路径 A 为例，路径 B 跳过 Redis pending 步骤）：
--
--    调用方（iOS）本地生成 session_id → 发 start_session {session_id, tool, ...}
--      │
--      │ 云端校验 session_id 格式 → 写 Redis:
--      │   session:pending:{session_id} = {user_id, device_id, tool, profile, cwd, ts}
--      │   TTL = 30s  ← Agent 收到后立即发 ack，超时未 ack = Agent 离线
--      │
--      │ 转发给 Agent
--      │
--      ├── Agent 收到消息 → 立即发 ack → 云端刷新 TTL 到 120s
--      │     （给 shell 初始化 + AI CLI 启动留足时间）
--      │
--      ├── Agent spawn PTY → 成功后发 session_created
--      │     → 云端读 Redis 取 pending 数据 → INSERT MySQL → DEL Redis
--      │
--      └── 失败路径：
--            ├── 30s 内未收到 ack → Agent 离线 → 返回 error_notify: device_offline
--            ├── ack 后 120s 内未确认 → Agent/PTY 启动超时 → TTL 到期自动清理
--            └── Agent 主动返回 error → 同上，TTL 清理 + 返回 error 给调用方
--
--    如果 Agent 离线 → 云端立即返回 error_notify: {code: "device_offline"}
--
--    调用方视角：本地生成 session_id → 发 start_session {session_id}
--      → UI 显示 "连接中..."（session_id 立即可用，PTY 尚未就绪）
--      → 收到 session_created → UI 切换到终端界面
--      → 超时/error → UI 显示失败提示

-- ② 运行中
--    每条消息 → INSERT kn_message（seq 由 Agent 分配）
--    本地输入和 iOS 输入统一经过 Agent InputMerger，消息都标注 src 字段
--
--    Agent 不设置空闲超时，不会因 session 无活动而自动终止进程。

-- ③ 会话生命周期：ended_at 区分
--    ended_at IS NULL → 进行中
--    ended_at IS NOT NULL → 已结束

-- ┌──────────┬──────────────────────────────┬──────────────────────┐
-- │ 触发方    │ 触发条件                      │ 云端操作              │
-- ├──────────┼──────────────────────────────┼──────────────────────┤
-- │ iOS      │ 发送 start_session           │ INSERT               │
-- │          │                              │ → Cloud 回 ack       │
-- │          │                              │ → 转发给 Agent       │
-- ├──────────┼──────────────────────────────┼──────────────────────┤
-- │ Agent    │ PTY 启动成功，上报             │ (纯转发，不写 DB)     │
-- │          │ session_created               │ → 通知 iOS            │
-- ├──────────┼──────────────────────────────┼──────────────────────┤
-- │ Agent    │ 上报 session_ended            │ UPDATE               │
-- │          │ {reason:"completed"/"killed"} │ ended_at=now         │
-- ├──────────┼──────────────────────────────┼──────────────────────┤
-- │ 云端定时器 │ Agent 断线 >30min 未重连      │ UPDATE               │
-- │          │ （确认死亡，PTY 已丢失）       │ ended_at=now         │
-- └──────────┴──────────────────────────────┴──────────────────────┘

-- session_interrupted 与 failed 的关系：
--   WSS 消息 `session_interrupted` 和 DB status `failed` 描述的是同一个事件（Agent 异常断线
--   导致 session 丢失），但用途不同：
--     • DB status='failed'   → 记录终态，用于 session 历史列表展示
--     • session_interrupted 消息 → 通知客户端，携带 last_input/cwd/tool/profile 帮助用户重试
--   因此 DB 不需要单独的 'interrupted' 值，用 'failed' 统一表示非正常结束即可。

-- bind_code 说明（扫码登录模式）：
-- Agent HTTP POST /bind-init {machine_id} → 云端生成 6 位数字 → Agent 展示二维码
-- → Agent WSS connect ?code=xxx 临时连接 → iOS 扫码 POST /bind-confirm {code, JWT}
-- → 云端验证通过 → WSS 发 bind_result {device_token}。
-- 5 分钟过期、一次性消费 → Redis: bind:code:{code} → {machine_id} (TTL: 5min)

-- 绑定码表（已废弃，改用 Redis）
-- CREATE TABLE kn_bind_code (...)  ← 不需要，删掉
```

#### 3.1.4 Redis Key 设计

```
# JWT Refresh Token
refresh:token:{user_id}:{device}  → "{refresh_token}"           (TTL: 30d, 登录/刷新时写入)
refresh:revoke:{user_id}          → "{timestamp}"                (TTL: 30d, 改密/设备丢失时写入)

# 在线状态（key 使用 machineId，由 kn-cloud-ws 连接/断开时写入，handlePing 心跳续期）
device:online:{machine_id}        → "1"                        (TTL: 90s)

# WebSocket 连接路由
ws:node:{machine_id}              → "{ws_node_id}"    (Agent 所在 WS 节点)
ws:user:{user_id}                → "{ws_node_id}"    (iOS 连到哪个网关实例)

# WS 跨实例消息中继 (Redis Pub/Sub)
ws:relay:{ws_node_id}            → CHANNEL                     (每个 ws_node 实例订阅自己的 channel)
# 路由流程: ws_node-A 收到消息 → 查 ws:device:{id} 得到 ws_node-B
#          → PUBLISH ws:relay:ws_node-B <消息JSON> → ws_node-B 收到 → 本地投递
# 单实例: ws_node-A = ws_node-B = ws-1, 发布到自身 channel, 零额外开销

# 待投递消息缓存（云端暂存，Agent 断线时缓冲发给它的消息）
pending:agent:{device_id}          → LIST, 最多 1000 条          (Agent 断线时暂存，重连后批量投递)

# 离线消息缓存（iOS 端：用户离线时暂存操作事件）
offline:user:{user_id}             → LIST, 最多 1000 条          (iOS 离线时暂存，7 天无连接过期)

# 消息幂等
msg:dedup:{msg_id}               → "1"                        (TTL: 5min, 幂等去重窗口)

# 登录限流
login:rate:{email_or_ip}         → count                      (TTL: 15min)
login:locked:{email}             → "1"                        (TTL: 15min, 5次失败)

# 绑定码
bind:code:{code}                 → {machine_id}                 (TTL: 5min, 绑定前 user_id 未知)

# 会话状态
session:pending:{session_id}     → {user_id, device_id, tool, profile, cwd, ts}  (TTL: 30s, Agent 确认前暂存)
session:state:{session_id}       → {status, last_seq, ...}                      (实时状态)

# 并发连接检测
device:conn:{device_id}          → "{ws_connection_id}"        (TTL: 60s, 心跳刷新)

# 解绑冷却（按用户+设备指纹，同设备解绑后重绑不受限）
unbind:cooldown:{user_id}:{machine_id}    → timestamp       (TTL: 24h)

# APNs 推送路由
push:token:{user_id}             → SET of device_token          (一个用户多台 iOS 设备)

# 异常连接标记
device:anomaly:{device_id}       → "{reason}"                  (TTL: 7d, 异地/IP跳变)

# 跨进程控制 (Redis Pub/Sub — API → WS 通信)
# API 进程发 PUBLISH ws:control {action, device_id, ...}
# WS 进程 SUBSCRIBE ws:control → 收到后执行相应操作
ws:control                       → CHANNEL                     (Pub/Sub, 跨进程指令)
```

---

### 3.2 kn Agent (Rust)

#### 3.2.1 定位

Agent 是 **PTY 的多路复用器**——AI CLI 由 Agent 启动，PTY 由 Agent 持有，所有客户端（iOS、kn Desktop、本地终端）都是 Agent 的客户端。

```
                        ┌──────────────────────────┐
                        │        Agent (Rust)        │
                        │                            │
  iOS ◀──WSS──▶  Cloud  │  ┌──────────────────────┐ │
                        │  │   SessionManager      │ │
  kn Desktop ◀─IPC─▶    │  │                      │ │
  (Unix Socket)         │  │  ┌────────────────┐   │ │
                        │  │  │ Session "s1"   │   │ │
                        │  │  │                │   │ │
                        │  │  │  PTY master fd │◀──┼─┼── spawn → /bin/zsh -il
                        │  │  │  ├─ stdin      │──┼─┼── write  ← Input Merger
                        │  │  │  └─ stdout     │──┼─┼── read   → Output Fan-out
                        │  │  └────────────────┘   │ │
                        │  └──────────────────────┘ │
                        └──────────────────────────┘
```

**核心原则**：
- Agent 不"拦截"——它在一开始就是通道本身，不是后加的中间人
- PTY 由 Agent 创建并持有，AI CLI 由 Agent spawn
- 所有输入通过 Agent 汇入 PTY，所有输出通过 Agent 广播

#### 3.2.2 模块结构

Agent 采用 **Cargo workspace** 组织，与 Desktop 共享公共库 `kn-common`，各自独立编译。

**Repo 级目录**：

```
kn/                            # Monorepo 根
├── Cargo.toml                 # [workspace]，统一版本管理
├── common/                    # 公共库 — Desktop 和 Agent 共享
│   ├── Cargo.toml             #   serde, serde_yaml, fs2, sha2, chrono
│   └── src/
│       ├── lib.rs             #   重新导出所有公共模块
│       ├── commands.rs        #   home_dir(), find_binary()
│       ├── profile_cmd.rs     #   profile 读取、env vars 提取
│       ├── fingerprint.rs     #   IOPlatformUUID 采集
│       └── pty_trait.rs       #   PtyOutputSink trait + SharedWriter/SharedChild
│
├── agent/                     # Agent 守护进程（独立 binary crate）
│   ├── Cargo.toml             #   kn-common + tokio + tokio-tungstenite + machine_id
│   └── src/
│       ├── main.rs            # #[tokio::main] 入口：启动、信号处理、优雅退出
│       ├── state.rs           # AgentState 状态机 + crash 退避
│       ├── ws_client.rs       # WebSocket 客户端 (tokio-tungstenite)
│       │                      #   - 临时连接 (?code=xxx)：绑定流程，仅收 bind_result
│       │                      #   - 正式连接 (?device_token=xxx)：日常运行，全部功能
│       │                      #   - 重连 (指数退避) + 心跳 (15s/45s)
│       │                      #   - 消息序列号管理 + 离线消息拉取
│       ├── session.rs         # SessionManager
│       │                      #   - 创建/销毁 PTY 会话
│       │                      #   - 多 Session 并发管理 (tokio task)
│       │                      #   - 会话快照 (每 30s)
│       │                      #   - 输入合并 (InputMerger: FIFO)
│       │                      #   - 输出广播 (OutputFan-out)
│       ├── proto.rs           # 消息协议定义 (serde)
│       │                      #   - 所有 ClientMessage / ServerMessage 类型
│       ├── ipc.rs             # Unix Socket IPC Server
│       │                      #   - kn Desktop 连接
│       │                      #   - 列出活跃会话 / 接入/离开会话
│       └── launchd.rs         # launchd plist 管理
│                              #   - 安装/卸载/暂停/恢复
│
├── desktop/src-tauri/         # Desktop (Tauri) — 依赖 kn-common
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── pty.rs             # Tauri 端 PTY 管理 (保留 ChannelSink)
│       └── ...
├── shell/
└── site/
```

**为什么用 workspace 而不用 `[[bin]]` 同 crate**：
- Agent 和 Desktop 有不同的依赖需求（Agent 要 tokio，Desktop 要 tauri）
- 避免递归链接 Tauri 依赖到 Agent 二进制
- 各自的 `[dev-dependencies]` 互不污染
- Agent 独立测试、独立构建，不与 Tauri lib 耦合

#### 3.2.3 状态机

```
                          ┌─────────┐
                          │ stopped │  ← 初始 / 用户暂停
                          └────┬────┘
                               │ 启动
                               ▼
                          ┌──────────┐
                          │ starting │  检查本地文件
                          └────┬─────┘
                               │
               device_token?   │
          ┌────────────────────┼────────────────────┐
          │ 有                  │ 没有               │
          ▼                     ▼                    │
  正式 WSS 连接           ┌──────────┐               │
  ?device_token=xxx       │ unbound  │  等待用户绑定  │
       │                  └────┬─────┘               │
       │                       │                     │
       │               用户点击"绑定设备"              │
       │               HTTP /bind-init               │
       │                       │                     │
       │                  ┌────┴─────┐               │
       │                  │ binding  │  WSS 临时连接  │
       │                  │ ?code=xxx│  等待扫码结果   │
       │                  └────┬─────┘               │
       │                       │                     │
       │               iOS 扫码确认                   │
       │               收到 bind_result              │
       │               存 device_token               │
       │               WSS 切正式连接                 │
       │                       │                     │
       └───────────┬───────────┘                     │
                   ▼                                 │
           ┌───────────┐                             │
           │ connected │  ← 正式 WSS 已建立           │
           └─────┬─────┘                             │
                 │                                   │
     ┌───────────┼────────────────┐                  │
     ▼           ▼                ▼                  │
┌──────────┐ ┌───────────┐ ┌──────────────┐         │
│ running  │ │   idle    │ │ reconnecting │         │
│ (有活跃  │ │ (在线无   │ │ (断线重连    │         │
│  AI 会话) │ │  AI 会话)  │ │  中...)      │         │
└──────────┘ └───────────┘ └──────┬───────┘         │
     │                             │                 │
     │ 最后会话结束    重连成功 ───┘                 │
     ▼                             │                 │
┌───────────┐                      │                 │
│   idle    │◀─────────────────────┘                 │
└───────────┘                                        │
```

**状态转换规则**：

| 当前状态 | 触发事件 | 新状态 |
|---------|---------|--------|
| stopped | 启动命令 / launchd 触发 | starting |
| starting | 本地有 device_token → 正式 WSS 连接成功 | connected |
| connected | WSS 握手完成 | idle |
| starting | 本地无 device_token | unbound |
| starting | WSS 连接失败 (3次重试) | stopped |
| unbound | 用户点击"绑定设备"，HTTP /bind-init 成功 | binding |
| unbound | 用户执行暂停 | stopped |
| binding | 收到 bind_result {device_token}，WSS 切正式连接 | connected |
| binding | bind_code 过期 (5min) 或用户取消 | unbound |
| connected / idle | 创建一个 AI 会话 | running |
| running | 所有 AI 会话结束 | idle |
| 任何状态 | 正式 WSS 断线 | reconnecting |
| reconnecting | 重连成功 | 恢复到断线前的状态 |
| reconnecting | 连续失败超过阈值 | stopped (等待 launchd 重启) |
| binding | WSS 临时连接断开 | unbound (用户需重新发起绑定) |
| 任何状态 | 用户执行 "暂停" | stopped (优雅退出) |
| 任何状态 | 收到 SIGTERM | 优雅退出 → stopped |

#### 3.2.4 与 kn 现有代码的关系

Agent 与 Desktop 通过 **Cargo workspace** 共享公共库 `kn-common`（位于 `common/` 目录）。共享模块一览：

| 模块 | 位置 | 用途 | 复用方式 |
|------|------|------|---------|
| `commands.rs` | `common/src/commands.rs` | `home_dir()`, `find_binary()` — 纯函数不依赖 Tauri | `use kn_common::commands` |
| `profile_cmd.rs` | `common/src/profile_cmd.rs` | 读 profile 获取 env vars，启动 AI CLI 前注入 | `use kn_common::profile_cmd` |
| `fingerprint.rs` | `common/src/fingerprint.rs` | 设备指纹采集 (IOPlatformUUID) | `use kn_common::fingerprint` |
| `pty_trait.rs` | `common/src/pty_trait.rs` | `PtyOutputSink` trait + `SharedWriter`/`SharedChild` 类型 | Desktop 和 Agent 各自实现 sink |
| `config.yaml` | `~/.kn/config.yaml` | 共享配置文件，Agent 和 Desktop 读写同一文件 | 跨进程文件锁 (`fs2::lock_exclusive`) |

Desktop 侧 (`desktop/src-tauri/Cargo.toml`) 依赖 `kn-common`：
```toml
[dependencies]
kn-common = { path = "../../common" }
```

Agent 侧 (`agent/Cargo.toml`) 同样依赖 `kn-common`：
```toml
[dependencies]
kn-common = { path = "../common" }
```

**`PtyOutputSink` trait**（定义在 `common/src/pty_trait.rs`）：

当前 `pty.rs` 通过 Tauri `Channel<PtyEvent>` 向前端推送数据。Agent 没有 Tauri context，需要改为通用接口：

```rust
// common/src/pty_trait.rs
pub trait PtyOutputSink: Send + 'static {
    fn send(&self, data: &[u8]) -> Result<(), String>;
    fn on_ready(&self) -> Result<(), String> { Ok(()) }
    fn on_exit(&self, code: i32) -> Result<(), String> { Ok(()) }
    fn on_error(&self, msg: &str) -> Result<(), String> { Ok(()) }
}

// 已有的 SharedWriter/SharedChild 类型也迁移到这里
```

三个 sink 实现：
- **ChannelSink**（Desktop 现有）：包裹 `Channel<PtyEvent>`，推给前端 xterm.js
- **WssSink**（Agent 新增）：包裹 `mpsc::UnboundedSender`，推给云端 WSS
- **IpcSink**（Agent 新增）：包裹 `mpsc::UnboundedSender`，推给 Desktop 的 Unix Socket

改动量中等。核心在 `pty.rs` 抽 `PtyOutputSink` trait 到 common，同步调整：
- `drain_utf8_stream` 改用 trait 泛型，`start_pty` / `write_pty` / `resize_pty` / `kill_pty` 4 个 Tauri command 签名更新
- `PtyState` 从 `tauri::State` 管理改为 `Arc<Mutex<PtyState>>`
- Desktop 保留 `ChannelSink`，Agent 新增 `WssSink` + `IpcSink`

**config.yaml 跨进程写安全**：

Agent 和 Desktop 是独立进程。写入 `config.yaml` 必须使用 `with_write_lock_exclusive()`（组合了进程内 `Mutex<()>` + `fs2::lock_exclusive()` 文件锁），与 Python CLI 的 `fcntl.flock` 互操作。

Agent 在 tokio 异步上下文中调用文件锁时，需用 `tokio::task::spawn_blocking` 包装，避免阻塞 event loop。

**CLI Tool 启动前的预处理**：

当前 `shell/ai-profile.sh` 中有两个 shell 层 workaround，Agent 在 spawn PTY 前需在 Rust 侧等效实现：

| Tool | Shell 做法 | Agent Rust 实现 |
|------|-----------|----------------|
| **Claude** | 把 profile env vars 写入临时 `settings.json` → `claude --settings <tmp> ...` → 退出后删临时文件 | Agent spawn 前生成 temp JSON `{"env": {...}}` → PTY 内命令追加 `--settings` 参数 → PTY EOF 后清理 |
| **Codex** | 备份 `~/.codex/auth.json` → 写入含 API key 的新 `auth.json` → 启动 codex → 退出后恢复备份 | Agent spawn 前 backup → write new → PTY EOF 后 restore |
| **qoderclicn** | 无特殊处理，直接 `eval env && command` | 无额外逻辑，注入 env vars 后直接 spawn |

这两个 workaround 封装在 Agent 的 `session.rs` 中，作为 PTY spawn 流程的一部分。`profile_cmd.rs` 负责读 env vars（已有），session 层负责工具特定的启动前准备（新增）。

**新增**的 Agent 专属模块：WebSocket 客户端、Session 管理、IPC Server、launchd 管理、CLI tool 启动预处理。

#### 3.2.5 安装与生命周期

**安装位置**：`~/.kn/agent/kn-agent`（独立二进制）

**launchd plist**：`~/Library/LaunchAgents/com.kn.agent.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kn.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/xxx/.kn/agent/kn-agent</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>          <!-- 退出后自动重启 -->
    <key>ThrottleInterval</key>
    <integer>5</integer>  <!-- 最短重启间隔 -->
    <key>StandardOutPath</key>
    <string>/Users/xxx/.kn/agent/agent.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/xxx/.kn/agent/agent_error.log</string>
</dict>
</plist>
```

**Crash 退避**：launchd 的 `KeepAlive` + `ThrottleInterval=5` 保证 Agent 崩溃后 5s 重启，但如果崩溃由持久性故障（配置损坏、磁盘满）引起，会形成每 5s 一次的重启风暴。Agent 自身记录连续崩溃次数来打破循环：

```
Agent 启动时:
  读 ~/.kn/agent/crash_count
  ├── crash_count ≤ 5 → 正常启动
  └── crash_count > 5 → 进入 safe_mode
      ├── 仅维持 WSS 连接 + 响应状态查询
      ├── 不创建 session，不接受远程指令
      ├── 通过 WSS 上报 {type: "agent_error", code: "crash_loop"}
      └── Desktop 📡 显示红色 + "Agent 异常，请查看日志"

正常启动后:
  连续运行超过 60s → 重置 crash_count = 0

每次崩溃时:
  crash_count += 1（写在崩溃发生前）

用户手动修复后:
  kn agent reset-crash-count → crash_count = 0 → 退出 safe_mode
```

这样既保留了 launchd 的即时恢复能力（偶发崩溃 5s 恢复），又防止了持久性故障导致的重启死循环。

**生命周期管理**：

Agent 完全由 Desktop 托管，用户不需要知道 Agent 的存在。安装/卸载/启动/升级全部自动。

| 操作 | 触发方式 | 说明 |
|------|---------|------|
| 安装 | Desktop 首次启动 | 从 bundle 拷出 Agent → `~/.kn/agent/` → 注册 launchd |
| 卸载 | Desktop 被删除 | 下次启动检测到 .app 不存在 → 自动清理 Agent + launchd |
| 启动 | launchd RunAtLoad | 开机自启，用户无感 |
| 升级 | Desktop 启动时 | 比较版本 → bundle 新版 → 原子替换 → 重启 Agent |
| 暂停 | Desktop 📡 面板 [暂停] | Agent 优雅退出，launchd KeepAlive=false |
| 恢复 | Desktop 📡 面板 [恢复] | launchd KeepAlive=true + 启动 Agent |

**CLI 子命令** — `kn agent` 命令族（Phase 2 实现）：

Agent 二进制 `kn-agent` 支持两种运行模式，由参数自动区分：
- **Daemon 模式**（无参数）：launchd 启动，长期运行，维护 WSS + IPC + PTY
- **CLI 模式**（带子命令）：连接到已运行 Agent 的 Unix Socket，发送 IPC 请求后立即返回

```bash
# 状态与调试
kn agent status              # IPC: {"method":"status"} → 打印 JSON

# 设备绑定（无 Desktop 时使用）
kn agent bind                # IPC: {"method":"bind"} → Agent 调 /bind-init → 打印 ASCII QR

# Session 管理
kn agent sessions            # IPC: {"method":"sessions"} → 列出所有活跃 session
kn agent --new \             # IPC: {"method":"new_session", ...} → 创建 session，打印 session_id
  --tool <claude|codex|qoder> \
  --profile <name> \
  --cwd <path>
kn agent attach <sess_id>    # IPC: {"method":"attach", ...} → 将当前终端 stdin/stdout 接入 session
                             #    PTY 输出打印到终端，键盘输入转发到 PTY（类似 tmux attach）
kn agent kill <sess_id>      # IPC: {"method":"kill_session", ...} → 强制终止 session

# Crash 恢复
kn agent reset-crash-count   # IPC: {"method":"reset_crash"} → crash_count 归零，退出 safe_mode
```

**CLI 实现方式**：`kn-agent` 二进制本身就是 CLI 入口。Daemon 模式和 CLI 模式共用同一个二进制文件。CLI 模式流程：
1. 解析命令行参数
2. 连接 `~/.kn/agent/ipc.sock`
3. 发送 JSON-line 请求
4. 读取响应 → 打印到 stdout → 退出

不需要额外的 Python wrapper（`bin/profile` 继续只做 profile CRUD）。`kn agent` 命令通过 `PATH` 中的 `kn-agent` 二进制或 shell alias 映射到 `~/.kn/agent/kn-agent`。

**Shell hook 集成**（§3.2.8）：`ai()` 包装函数中通过 `kn agent --new --tool "${1}" ...` 创建 session。此命令在 Phase 2 与 IPC Server 一同实现。

#### 3.2.6 版本管理：Desktop 统一管理 Agent 版本

**原则**：Agent 不自更新，Desktop 管 Agent 版本。同一个 git repo，同一个 tag，同一个 release。

```
Desktop 启动
  │
  ├── 读 bundle/Resources/kn-agent 的版本 (编译期嵌入)
  │   例如: kn-agent v1.2.0
  │
  ├── 读 ~/.kn/agent/kn-agent 的版本
  │   例如: kn-agent v1.1.0
  │
  ├── 比较
  │   ├── 本地不存在 → cp bundle → ~/.kn/agent/ → 注册 launchd → 启动
  │   ├── 版本一致  → 无事
  │   └── bundle 更新 → cp 替换 → 重启 Agent
  │        替换过程: cp → .tmp → chmod +x → rename → 原子替换
  │
  └── Desktop 自身走 Tauri updater 更新 .app
      → 下次启动时 bundle 里已是新版 Agent → 自动同步到本地
```

**发布打包**：Agent 二进制嵌在 Desktop .app bundle 里，编译脚本从 workspace 根 `target/release/kn-agent` 拷到 `desktop/src-tauri/resources/kn-agent`。Tauri 打包时用同一个 Developer ID 证书对 bundle 内所有 Mach-O 文件签名（包括 Agent），公证流程不需要额外步骤。详见 §3.2.7 构建流程。

**升级时处理活跃会话**：替换 Agent 二进制需要重启，这会影响正在运行的 AI session。处理策略：

```
Desktop 触发升级
  │
  ├── Agent 当前无活跃 session → 直接替换重启（< 1s，用户无感）
  │
  └── Agent 有活跃 session → 弹窗三选一：
        [等待结束] [立即升级] [稍后提醒]

        用户选 [等待结束] →
          Agent 进入 drain 模式：
            - 拒绝新 session（返回 "agent_upgrading"）
            - 现有 session 继续运行
            - 每 5s 向 Desktop 报告剩余 N 个活跃会话
          Desktop 显示 "等待会话结束... 剩余 2 个 (已等 3 分钟)"

          所有 session 自然结束 → Desktop 立即替换重启

          如果 30 分钟还没结束 → 自动降级弹窗：
            "已等待 30 分钟，仍有 1 个会话在运行。"
            [继续等待] [立即升级]
```

drain 模式下只阻挡新 session，已运行的 AI 任务持续到自然结束。30 分钟上限防止极端情况（AI 卡死），用户始终可强制升级。

**Agent 不保留独立自更新能力**：去掉 `updater.rs` 模块。紧急安全补丁走 Desktop 热修复发布。

#### 3.2.7 桌面端 Agent 部署

kn Desktop .app bundle 结构：

```
kn.app
└── Contents
    ├── MacOS/
    │   └── kn              ← Desktop (Tauri)
    └── Resources/
        └── kn-agent        ← Agent 守护进程（由 build-agent.sh 拷贝）
```

**Tauri bundle.resources 配置**（`desktop/src-tauri/tauri.conf.json`）：

```json
{
  "bundle": {
    "resources": {
      "../resources/kn-agent": "./"
    }
  }
}
```

**构建流程**（CI 和本地一致）：

```bash
# 1. 先编译 Agent
cargo build --release --bin kn-agent

# 2. 拷贝到 Resources
cp target/release/kn-agent desktop/src-tauri/resources/kn-agent

# 3. 编译 + 打包 Desktop（Tauri 自动将 resources/ 嵌入 bundle）
cd desktop && npm run tauri build
```

**代码签名**：打包时，Tauri 的 `tauri-bundler` 使用同一 `Developer ID Application` 证书对 bundle 内所有 Mach-O 可执行文件签名，包括 `kn`（Desktop）和 `kn-agent`（Agent）。不需要额外签名步骤。

```bash
# 签名验证（打包后）
codesign -dvvv kn.app/Contents/MacOS/kn
codesign -dvvv kn.app/Contents/Resources/kn-agent
# 两者应显示相同的 TeamIdentifier 和 Authority
```

**macOS 公证**：Tauri bundler 生成的 `.dmg` 提交给 Apple Notary Service。bundle 内所有签名文件共用一张公证票据（stapled to `.app`），`kn-agent` 作为 Resources 中的可执行文件包含在内，不需要单独公证。

**Desktop 启动时**：
1. 检查 `launchctl list com.kn.agent` — Agent 是否在运行
2. 启动或确保 Agent 运行
3. 检查版本 → 需要则升级替换 → 重启
4. Agent 就绪后，Desktop 工具栏 📡 显示绿色或其他状态

Agent 只做一件事：收到 Desktop 指令和 iOS 指令后，维护 PTY / WSS 连接 / session 管理。

#### 3.2.8 shell hook — 自动路由到 Agent

用户不用改变习惯。Hook `ai()` 函数自动将调用路由到 Agent。

**改造方式**：将现有 `shell/ai-profile.sh` 中的 `ai()` 函数重命名为 `_ai_direct()`（保留全部原有逻辑不变），新建 `ai()` 包装函数做 Agent 路由。原有逻辑无需任何改动。

```bash
# ~/.kn/shell-rc 中改造（伪代码，实际以 ai-profile.sh 为准）

# ── _ai_direct() = 当前 ai() 的完整逻辑，仅改名 ──
# 包含：profile 选择链（显式参数 → .ai-profile 文件 → 默认 profile → fzf 交互）、
#       Claude --settings 临时文件生成、Codex auth.json swap、project 自动注册、
#       tool 存在性检查、env vars 注入、子 shell 启动
_ai_direct() {
  # 现有 ai() 函数体的完整拷贝（约 80 行），不新增任何逻辑
  # 详见 shell/ai-profile.sh 中的实际实现
  local cmd="${1:-}"
  case "$cmd" in
    claude|codex|qoderclicn)
      # ... profile 选择 + env 注入 + 启动（当前逻辑，不变）
      ;;
    profile)
      # ... profile 管理子命令（当前逻辑，不变）
      ;;
    -h|--help|help) _ai_help ;;
    tips)            _ai_tips ;;
    *)               echo "Unknown command: $cmd" >&2 ;;
  esac
}

# ── 新 ai() — Agent 路由包装 ──
ai() {
  if /bin/launchctl list | grep -q com.kn.agent 2>/dev/null; then
    # Agent 在运行，通过 IPC 新建会话
    if kn agent --new --tool "${1}" --profile "${2}" --cwd "$(pwd)" 2>/dev/null; then
      echo "Session created via kn Agent. Use 'kn attach' to connect."
      return 0
    fi
    echo "Agent IPC 不可用，本次会话仅本地运行" >&2
  fi
  # 回退：走原有完整流程（直接启动 AI CLI，无远程能力）
  _ai_direct "$@"
}
```

**改造量**：纯 shell 重构，`ai()` → `_ai_direct()` 改名 + 新增 10 行包装函数。原有 profile 选择、Claude/Codex workaround、fzf 交互等全部保持不变，无需往 Rust 侧迁移。

**Profile 选择**：Agent 路由模式下，profile 选择由 shell 层完成（选好 profile 名后作为参数传给 Agent），Agent 通过 `kn_common::profile_cmd` 读取 env vars 注入 PTY。shell 层和 Agent 层都能访问同一 `~/.kn/config.yaml`，不存在选择逻辑重复。

**待实现**：`kn agent` CLI 命令族（详见 §3.2.5 CLI 子命令），需在 Phase 2 与 IPC Server 一同实现。当前 `bin/profile` 仅有 profile CRUD 功能，不影响。

#### 3.2.9 IPC 协议 — Agent ↔ Desktop 通信

Agent 与 Desktop 之间通过 **Unix Domain Socket** 通信。Agent 是服务端，Desktop 是客户端。

**Socket 路径**：`~/.kn/agent/ipc.sock`，权限 `0600`（仅当前用户可读写）。

**帧协议**：JSON-line —— 每条消息是一行完整 JSON，以 `\n` 结尾。请求-响应模式，一问一答。

**连接生命周期**：
- Agent 启动 → 创建 socket 文件 → `bind()` → `listen()` → accept loop
- Desktop 启动 → `connect()` → 发送请求 → 读取响应 → 保持连接（长连接，复用）
- Agent crash / 退出 → socket 文件消失 → Desktop 检测到断连 → 每 2s 重试 → Agent 重启后自动重连
- 心跳保活：Agent 每 60s 向所有已连接的 Desktop 客户端发 `{"type":"heartbeat"}`，Desktop 无需回复。Desktop 侧超时 120s 未收到 heartbeat → 判断 Agent 离线。

**消息类型**：

##### Desktop → Agent (request)

| method | 参数 | 返回 | 说明 |
|--------|------|------|------|
| `status` | `{}` | `{status, crash_count, safe_mode}` | 查询 Agent 当前状态和 crash 信息 |
| `sessions` | `{}` | `{sessions: [{id, tool, profile, cwd, cols, rows, created_at}]}` | 列出所有活跃 session |
| `bind` | `{}` | `{action: "bind", bind_code, hostname}` 或 `{error}` | 发起设备绑定：Agent 调云服务 `/bind-init` → 返回 bind_code 给 Desktop |
| `pause` | `{}` | `{ok: true}` | 暂停 Agent（优雅退出，launchd KeepAlive=false） |
| `resume` | `{}` | `{ok: true}` | 恢复 Agent（KeepAlive=true + 启动） |
| `new_session` | `{tool, profile?, cwd, cols?, rows?}` | `{session_id, status: "created"}` | 创建新 AI 会话（供 shell hook `kn agent --new` 使用） |
| `attach` | `{session_id}` | `{ok: true}` 随后流式推送 `output` | 订阅 session 的 PTY 输出流，同时开始接收输入 |
| `detach` | `{session_id}` | `{ok: true}` | 取消订阅 session 输出 |
| `input` | `{session_id, text}` | `{ok: true}` | 向 session 的 PTY stdin 写入文本 |
| `ctrl` | `{session_id, signal}` | `{ok: true}` | 发送控制信号 (`ctrl_c` → `\x03`, `ctrl_d` → `\x04`) |
| `resize` | `{session_id, cols, rows}` | `{ok: true}` | 调整 session PTY 窗口尺寸 |
| `kill_session` | `{session_id}` | `{ok: true}` | 强制终止 session |
| `get_output_history` | `{session_id, offset?, limit?}` | `{lines: [...], offset, total}` | 分页读取 session 历史输出（从本地 `output.log`） |
| `get_version` | `{}` | `{version, agent_version}` | Desktop 启动时检查版本是否需要升级 |

##### Agent → Desktop (push / response)

| type | 内容 | 说明 |
|------|------|------|
| `status_changed` | `{status, crash_count, safe_mode}` | Agent 状态变化时主动推送给 Desktop（📡 图标刷新） |
| `heartbeat` | `{ts}` | 每 60s 保活信号 |
| `output` | `{session_id, text}` | PTY 输出流式推送（attach 后持续推送直到 detach） |
| `session_created` | `{session_id, tool, profile, cwd}` | 任何来源（iOS/Shell/Desktop）创建 session 后通知 |
| `session_ended` | `{session_id, reason, exit_code?}` | session 结束通知 |
| `bind_result` | `{ok: true, device_token}` 或 `{ok: false, error}` | 绑定结果通知 |
| `error` | `{code, message}` | 操作失败（session 不存在、Agent 在 safe_mode、drain 中等） |

**流式输出语义**：
- Desktop 发 `attach {session_id}` → Agent 开始通过同一连接异步推送 `output` 消息（不等请求）
- Desktop 发 `detach {session_id}` → Agent 停止推送
- 同一时间 Desktop 可以 attach 多个 session，`output` 消息中的 `session_id` 用于区分
- PTY 输出量大时，Agent 端以 100ms 窗口或 64KB 积压任一达到即 flush，合并碎片

**错误码**：

| code | 说明 |
|------|------|
| `agent_not_running` | Agent 未运行或 IPC 无法连接 |
| `session_not_found` | session_id 不存在或已结束 |
| `session_locked` | 会话被其他客户端锁定（只读） |
| `safe_mode` | Agent 在安全模式，拒绝创建 session |
| `drain_mode` | Agent 升级中，拒绝新 session |
| `unknown_method` | 未知 IPC method |
| `invalid_params` | 参数格式错误或缺失必填字段 |

**与 WSS 协议的关系**：IPC 是进程内/本机通信，WSS 是远程通信。两者的消息格式和序列化方式一致（JSON），但消息类型独立——`start_session` 走 WSS（iOS → Cloud → Agent），`new_session` 走 IPC（Shell/Desktop → Agent）。Agent 内部统一处理后路由到 PTY。

---

### 3.3 iOS App (SwiftUI)

#### 3.3.1 架构

**最低 iOS 版本**：iOS 17.0。理由：SwiftUI `@Observable` 宏、`NavigationStack`、`URLSessionWebSocketTask` async API 均需 iOS 17 以上；2026 年中 iOS 17 覆盖率接近 100%（iPhone XS 及以上均支持）。

**v1 平台限制**：仅支持 iPhone 竖屏。iPad 和横屏模式后续版本适配。

**JavaScript 桥接**：WKWebView 内嵌 xterm.js，通过以下 bridge 与 Swift 通信：
- **Swift → JS**：`webView.evaluateJavaScript("window.writeANSIBase64('...')")` 推送 PTY 输出
- **JS → Swift**：`window.webkit.messageHandlers.terminalInput.postMessage(data)` 发送用户输入
- **JS → Swift**：`window.webkit.messageHandlers.terminalResize.postMessage({cols, rows})` 上报终端尺寸变化

WKUserContentController 注册两个 handler（`terminalInput`、`terminalResize`），Coordinator 实现 `WKScriptMessageHandler` 协议处理回调。

**键盘避让**：Terminal 区域使用 `ignoresSafeArea(.keyboard)` 在键盘弹出时自动收缩。xterm.js 的 `FitAddon.fit()` 在键盘显示/隐藏时重新计算 cols/rows。

```
iOS App (v1: Terminal-only)
├── App/
│   ├── KnApp.swift              # @main 入口
│   └── AppState.swift           # 全局状态: 登录态、连接态
├── Auth/
│   ├── LoginView.swift          # 登录页
│   ├── RegisterView.swift       # 注册页
│   └── AuthViewModel.swift
├── Devices/
│   ├── DeviceListView.swift     # 设备列表 (绑定/选择)
│   ├── BindDeviceView.swift     # 扫码绑定
│   └── DeviceViewModel.swift
├── Terminal/
│   ├── TerminalView.swift       # WKWebView + xterm.js 终端
│   ├── InputAccessoryBar.swift  # 自定义键盘工具栏 (Ctrl, Esc, Tab, ↑↓)
│   └── TerminalViewModel.swift
├── Services/
│   ├── APIClient.swift          # HTTP REST 客户端
│   ├── WebSocketClient.swift    # WSS 客户端 (URLSessionWebSocketTask)
│   ├── KeychainManager.swift    # JWT 安全存储
│   └── PushManager.swift        # APNs 注册
└── Models/
    ├── User.swift
    ├── Device.swift
    ├── Session.swift
    └── Message.swift
```

#### 3.3.2 核心交互设计

v1 为 Terminal-only 模式。WKWebView + xterm.js 渲染完整 PTY 终端。

```
┌─────────────────────────────────┐
│  Mac Studio  ·  claude  ·  🟢   │ ← 状态条 44pt
│─────────────────────────────────│
│                                  │
│                                  │
│          Terminal Area            │
│     (WKWebView + xterm.js)       │
│         填满剩余空间               │
│                                  │
│                                  │
│─────────────────────────────────│
│ [⚡] 输入栏: [重构 auth.ts______]  │ ← [⚡]直通开关 + 编辑区
│─────────────────────────────────│
│ [⏹][Esc][Tab][Spc][▲][▼][◀][▶][🎤][▶▶]│ ← 工具栏 + 发送按钮
└─────────────────────────────────┘
```

输入栏规则：
- 默认模式：键盘输入和语音输入都先进输入栏，点 ▶▶ 发送才进终端（防误触）
- **直通模式**：点击输入栏左侧 [⚡] 开关，键盘输入直接写入 PTY stdin，无需经过编辑区
  - 适用于频繁交互场景（`[y/N]` 确认、连续 Enter、Tab 补全等）
  - 语音输入在直通模式下仍然先进输入栏（防识别错误）
  - 关闭直通模式回到默认安全模式

Tab Bar 三页导航：
- **Terminal** — 主屏，终端 + 状态条 + 键盘工具栏
- **Sessions** — 活跃/已完成会话列表，右滑操作，+ 新建
- **Devices** — 设备列表 + 扫码绑定入口

配色采用**工业控制台**风格：深黑底色 `#0a0c0f`，绿色在线指示 `#00e676`，橙色重连中 `#ff9100`，红色离线 `#ff3d00`。终端字体 JetBrains Mono 12pt。

#### 3.3.3 自定义键盘工具栏

```
┌──────────────────────────────────────────────────────┐
│ [⏹] [Esc] [Tab] [Space] [▲][▼][◀][▶] [Ctrl] [Done] │
└──────────────────────────────────────────────────────┘
```

**独立功能键**（一键操作，高频优先）：

| 按钮 | 发送 | 说明 |
|------|------|------|
| ⏹ | `\x03` (Ctrl+C) | 中断当前 AI 操作，最高频 |
| Esc | `\x1b` | 退出模式 |
| Tab | `\t` | 自动补全 |
| Space | ` ` | 空格 |
| ▲▼◀▶ | `\x1b[A]` 等 | 历史命令 / 光标移动 |

**Ctrl sticky 键**（保留，低频组合用）：

| 操作 | 行为 |
|------|------|
| 点击 Ctrl | 激活（蓝色高亮），按下一个字母键后自动取消 |
| 长按 Ctrl | 锁定模式，可连续发多个控制码，再次点击取消 |

**设计原则**：高频操作一键直达，低频组合走 Ctrl sticky。手机打字成本高，能少按一次是一次。

**语音输入**：iOS 原生语音转文字（键盘麦克风按钮），识别结果填入输入栏，用户确认/编辑后点发送才进终端，避免识别错误直接发出去。键盘直接敲的字符也先进输入栏（透明 overlay），回车才发送。

#### 3.3.4 iOS 后台策略

| iOS 状态 | 行为 |
|----------|------|
| 前台 | 全部功能可用，WSS 实时推送 |
| 后台 (前 30s) | 保持 WSS，继续接收输出 |
| 挂起 | 系统切断 WSS，云端标记 iOS 离线 |
| 被杀死 | 云端通过心跳超时检测离线 |

#### 3.3.5 推送与离线消息策略

**核心原则**：APNs 只推摘要，不推原始输出。PTY 输出不缓冲，操作记录限量缓冲。

**APNs 前置依赖**：
- Apple Developer Program 会员 ($99/year)
- APNs Key (p8) 或 Certificate，服务端用 HTTP/2 调用 APNs provider API
- p8 key 无过期时间，但在 Apple Developer 后台 revoke 后立即失效。APNs 返回 403 时记录错误日志并自动降级（推送不可用，核心功能不受影响），运维收到告警后更新 p8 key

**在线/离线的判定**：云端查 Redis `ws:user:{user_id}`（iOS WSS 连接时写入，心跳 60s 刷新，断线自动过期）。存在 = 在线，走 WSS；不存在 = 离线，走 APNs + 缓冲。

**不会重复推送**：因为判定是原子的——查 Redis 的结果决定走哪条路，不会两条都走。

```
事件发生（AI 完成）
  │
  │  云端: 查 Redis ws:user:{user_id} 存在?
  │
  ├── 存在 → iOS 在线 → WSS 推 → 不调 APNs
  │
  └── 不存在 → iOS 离线 →
        ├── APNs 推摘要 (仅标题，不含数据)
        └── 消息进 Redis 离线缓冲
```

**竞态场景**：iOS 刚好从后台恢复，WSS 正在建连的几百毫秒窗口内：

```
事件发生时 WSS 刚好在重连
  │
  │  云端查 Redis → 还未写入 → 判定离线
  │  → APNs 发出 + 消息进缓冲
  │
  │  毫秒后 WSS 建立 → iOS 立即同步缓冲消息
  │
  │  结果: 用户锁屏看到推送提示，点开 App 看到完整结果
  │  不重复。APNs 只是铃铛（标题），WSS 才是信（完整数据）。
```

**不缓冲 PTY 原始输出的原因**：一次 AI 会话可能输出几 MB ANSI 文本。用户离线 7 天回来，如果全部缓存在 Redis 再一口气推给 iOS，服务端压力大、iOS 会卡死。

**离线恢复流程**：

```
用户点推送 → iOS 打开 App → WSS 重连
  │
  │ iOS 上报 last_ack_seq
  │
  │ 云端返回:
  │   ① 缓冲的操作事件 (input/ctrl/system)
  │      来源: Redis offline:user:{user_id}，最多 1000 条
  │      分页推送：每页 100 条，首次推首页 + 总页数
  │      iOS 端用户上滑加载更多时才拉取后续页
  │      超过 1000 条的旧事件直接丢弃（事件日志可丢失，不影响功能）
  │
  │   ② session 状态同步
  │      "s1 已结束 (completed)" / "s2 已中断 (failed)"
  │
  │   ③ 不推送历史 PTY 输出
  │      用户想看终端完整历史 → 点 session → iOS 调 Agent IPC
  │      → Agent 读本地 output.log → 流式推给 iOS
```

**APNs 推送事件**（仅重要事件，普通输出不推）：

| 事件 | 推送 | 原因 |
|------|------|------|
| AI 执行完成 (PTY EOF) | ✅ | 用户需要知道结果 |
| AI 需要确认 (>5s 无输入) | ✅ | 可能需要用户决策 |
| Agent crash / 会话中断 | ✅ | 异常通知 |
| 试用即将到期 | ✅ | 业务提醒 |
| PTY 每一行输出 | ❌ | 太多，无意义 |
| 心跳 / 状态变更 | ❌ | 不打扰用户 |

**Redis 缓冲限制**：

```
offline:user:{user_id}  LIST
  ├── 最多 1000 条（操作事件，非原始输出）
  ├── 超过 → LPOP 丢弃最旧
  └── 7 天无 WSS 连接 → EXPIRE 整条 key
```

用户离线 7 天回来 → 云端只推最近 1000 条操作事件（几百 KB），秒级恢复。PTY 历史输出从 Agent 本地按需拉取。

---

### 3.4 kn Desktop 适配

#### 3.4.1 启动时的 Agent 管理

Desktop 启动时，Agent 版本检查和升级在 **Rust `lib.rs` 的 `setup()` 阶段**完成（窗口显示之前），确保用户看到窗口时 Agent 已就绪。

```
Tauri app.launch()
  │
  ├─ Rust setup() 阶段（窗口未显示）
  │   ├── 检查 launchctl list com.kn.agent → 是否运行
  │   ├── 不在运行 → 从 bundle 拷出 → 注册 launchd → 启动
  │   ├── 版本旧   → 原子替换 → 重启 Agent
  │   └── 正常     → 无事
  │
  └─ 窗口显示（React 渲染）
      └── 📡 灰点闪烁 → 连 IPC → 收到首次 push → 切实际状态
```

Agent 已嵌入 .app bundle，不存在"未安装"状态——用户永远无需手动安装 Agent。版本管理详见 §3.2.6。

#### 3.4.2 工具栏 📡 按钮 — 五种图标状态

```
┌──────────────────────────────────────────────────────────────┐
│  kn                    [+] [🔍] [🌙] [🖥] [📡]              │
│                                       终端   远程控制        │
└──────────────────────────────────────────────────────────────┘
```

| 图标 | 状态 | 含义 | 触发条件 |
|------|------|------|---------|
| 📡 灰点闪烁 | 连接中 | Desktop 正在连接 Agent IPC，等待首次状态推送 | **初始状态** — Desktop 渲染后立即显示 |
| 📡 灰色 | Agent 未运行 | Agent 进程不存在或 IPC 持续不通 | 灰点闪烁 5 秒后仍未收到 IPC 响应 |
| 📡 橙点闪烁 | 未绑定 | Agent 运行中，无 device_token | Agent push status=unbound |
| 📡 橙点呼吸 | WSS 重连中 | 已绑定但 WSS 断开，Agent 自动重连 | Agent push status=reconnecting |
| 📡 绿点 | 已连接 | WSS 正式连接正常，可远程控制 | Agent push status=connected/idle/running |

初始状态设为"灰点闪烁"（而非灰色），覆盖两种场景：① Agent 已在运行，连 IPC 即可；② Agent 未运行，Desktop 需要先启动 Agent 再连 IPC。5 秒内收到 IPC 推送 → 切到实际状态；5 秒超时 → 切到灰色。

Desktop 通过 Unix Socket 接收 Agent 状态推送，图标自动刷新。Agent 在状态变化时主动 push 通知（连接/断开/binding 等），Desktop 无需轮询。

#### 3.4.3 完整流转逻辑

```
kn Desktop 启动
  │
  │  📡 = 灰点闪烁（初始状态，连 IPC 中）
  │
  ├─ 5 秒内 IPC 响应 → 📡 切到实际状态（灰/橙/绿）
  └─ 5 秒超时       → 📡 灰色，继续 2s 重试连 IPC
  │
  │ ① 确保 Agent 运行（从 bundle 拷出 / 版本升级 / 启动）
  │
  ▼
Agent IPC 就绪 → Desktop 收到首次 status push
  │
  ├── Agent 无 device_token
  │    │
  │    │  📡 = 橙点闪烁 (未绑定)
  │    │
  │    │  用户点击 📡 →
  │    │  ┌──────────────────────────────┐
  │    │  │        📱 绑定设备            │
  │    │  │                              │
  │    │  │      [ 二维码图片 ]           │
  │    │  │                              │
  │    │  │  请用 kn iOS App 扫码绑定     │
  │    │  │                              │
  │    │  │         [取消]                │
  │    │  └──────────────────────────────┘
  │    │
  │    │  流程：Desktop IPC → Agent HTTP /bind-init → 拿到 code
  │    │        Agent 建立临时 WSS (?code=xxx)
  │    │        iOS 扫码 POST /bind-confirm → 云端发 bind_result
  │    │        Agent 存 device_token → 切正式 WSS → 通知 Desktop
  │    │
  │    ├── 扫码成功 → 📡 = 绿点 → 弹窗自动关闭
  │    └── 取消/超时 → 弹窗关闭 → 📡 回到橙点闪烁
  │
  ├── Agent 有 device_token → 建立正式 WSS
  │    │
  │    ├── WSS 连接成功
  │    │    │
  │    │    │  📡 = 绿点 (已连接)
  │    │    │
  │    │    │  用户点击 📡 →
  │    │    │  ┌─────────────────────────────┐
  │    │    │  │  🟢 设备在线                 │
  │    │    │  │  设备名: 办公室 Mac           │
  │    │    │  │                             │
  │    │    │  │  活跃会话: 2 个              │
  │    │    │  │  ┌─────────────────────────┐ │
  │    │    │  │  │ s1  claude   project-A  │ │
  │    │    │  │  │ s2  codex    project-B  │ │
  │    │    │  │  └─────────────────────────┘ │
  │    │    │  │                             │
  │    │    │  │      [暂停连接]              │
  │    │    │  └─────────────────────────────┘
  │    │    │
  │    │    └── 用户点 [暂停连接] → Agent 优雅退出
  │    │        launchd KeepAlive = false
  │    │        📡 = 灰色 (Agent 未运行)
  │    │
  │    ├── WSS 连接失败 / 断线
  │    │    │
  │    │    │  📡 = 橙点呼吸 (重连中)
  │    │    │
  │    │    │  用户点击 📡 →
  │    │    │  ┌─────────────────────────────┐
  │    │    │  │  🟠 连接中断，正在重连...      │
  │    │    │  │                             │
  │    │    │  │  活跃会话: 2 个 (只读)       │
  │    │    │  │  (重连期间无法操作)           │
  │    │    │  │                             │
  │    │    │  │      [暂停连接]              │
  │    │    │  └─────────────────────────────┘
  │    │    │
  │    │    └── 重连成功 → 📡 = 绿点
  │    │        重连失败超过阈值 → Agent 退出
  │    │        → launchd 重启 Agent → 回到 ①
  │    │
  │    └── Agent 正常运行中，WSS 断线
  │         → Agent 主动 push 断线事件 → 📡 自动切换到橙点呼吸
  │
  └── Agent 被暂停后，用户点击 📡
       ┌─────────────────────────────┐
       │  🟡 远程控制已暂停           │
       │                             │
       │      [恢复连接]              │
       └─────────────────────────────┘
       用户点 [恢复] → launchd KeepAlive=true → 启动 Agent → 回到 ①
```

**Desktop 不需要登录**。用户身份在 iOS 端确认，Agent 凭 device_token 连云端。**一台 Mac 只能绑定一个 kn 账号**，解绑前其他用户无法绑定。多用户共享 Mac 场景需各自用独立 macOS 账户（各自有独立 `~/.kn/agent/`）。

**不轮询绑定结果**：扫码确认后云端通过 WSS 主动推 `bind_result` 给 Agent，Agent 再 IPC 通知 Desktop 关闭二维码。

**Desktop 获取 Agent 状态**：Agent 在状态变化时主动通过 Unix Socket push 事件给 Desktop，Desktop 收到后更新 📡 图标。每 60s 发一次心跳保活作为兜底检测 Agent 活性和 IPC 链路健康。用户无需手动刷新。

#### 3.4.4 活跃会话 ≠ WSS 连接

"活跃会话"是 Agent 管理下正在运行的 **AI CLI 进程**，不是 WSS 连接。WSS 只有一条长连接。

```
Agent
├── WSS ──────────▶ 云服务  (1 条，始终在)
│
├── Session "s1" ──▶ claude CLI (PID 12345)
├── Session "s2" ──▶ codex CLI  (PID 12346)
│
└── Desktop 通过 Unix Socket 问 Agent → 返回 Session 列表 → 面板显示
```

#### 3.4.5 Agent 是 AI 会话的统一管理入口

Agent 是远程控制和 session 管理的统一入口，但 **Desktop 保留直接 PTY 能力**作为本地降级路径。

```
Agent 在线时（正常路径）：

Desktop Terminal ──IPC──┐
Shell ai() ────────IPC──┼──→ Agent ──→ PTY ──→ AI CLI
iOS ────WSS──→ Cloud ───┘
│                        │
│                        └── session 上报云端，支持远程控制

Agent 离线时（降级路径）：

Desktop Terminal ──→ 直接 spawn PTY（现有 pty.rs 代码）
Shell ai()        ──→ _ai_direct()（现有 ai() 逻辑）
                     │
                     └── session 标记 local-only，不上报云端
```

两条路径的切换由 Agent IPC 连接状态决定：IPC 可通 → 走 Agent；IPC 不通 → 走本地直接 PTY。Desktop 的 `pty.rs` 代码完整保留，不删。

**Session 数据来源**：

| 数据 | 存储 | 用途 |
|------|------|------|
| AI 会话状态（运行中/已结束） | Agent（内存 + 磁盘）→ IPC | 实时 PTY 输入输出流、会话生命周期 |
| 终端标签页历史 | localStorage `SessionRecord`（现状不变） | "上次从这里启动了哪个 profile"——快速恢复/切换 |

两者互不冲突：Agent 是 PTY 的持有者，实时的终端读写走 Agent IPC（替代当前 Tauri Channel）；终端面板的标签页历史（`SessionRecord`）继续由前端在 localStorage 维护，不经过 Agent。

| | Agent (本地) | 云端 kn_session |
|------|------|------|
| 存储 | 内存 + `~/.kn/agent/sessions/` | MySQL + Redis |
| 内容 | 完整 session 状态 + PTY 输出 + checkpoint | user_id, device_id, tool, profile, cwd, source, status, timestamps + 关联 kn_message |
| 用途 | 本地 Terminal 面板 + iOS 历史输出按需拉取 | 跨设备历史 + iOS 查看 |
| 可见范围 | 当前机器 | 用户所有设备 |

**改动的文件**：

| 文件 | 改动 |
|------|------|
| `App.tsx` | 启动时 Agent 版本检测 + 📡 按钮 + 五态面板 |
| `useTerminal.ts` | PTY 实时读写改为 Agent IPC（替代 Tauri Channel）；标签页历史保留 localStorage 不变 |
| 新增 `useAgent.ts` | 封装 IPC 通信（状态/绑定/活跃会话列表拉取） |

---

## 4. 通信协议

### 4.1 WebSocket 连接建立

所有 WSS 连接统一使用 `wss://api.knshark.com/v1/ws`（无 query string），鉴权信息通过 **HTTP `Authorization` header** 传递：

```
# iOS 连接
GET /v1/ws HTTP/1.1
Upgrade: websocket
Authorization: Bearer <access_token>
X-KN-Role: ios
X-KN-Protocol-Version: 1

# Agent 正式连接（绑定后）
GET /v1/ws HTTP/1.1
Upgrade: websocket
Authorization: Bearer <device_token>
X-KN-Machine-Id: <machine_id>
X-KN-Protocol-Version: 1

# Agent 临时连接（绑定中，code 有效期 5min）
GET /v1/ws HTTP/1.1
Upgrade: websocket
Authorization: Bearer <bind_code>
X-KN-Machine-Id: <machine_id>
X-KN-Protocol-Version: 1
```

**为什么用 HTTP Header 而不是 query string**：Nginx/负载均衡器默认将完整 URL（含 query string）写入 access log。token 类敏感凭证放在 header 中不会被记录，防止日志泄露。

连接成功后：
  → 服务端推送: {type: "connected", ws_session_id: "ws_xxx", protocol_version: 1}
  → Agent（正式连接）推送 profile_list（可用 profile 列表，仅名称/类型/描述，不含 env vars）
  → Agent（临时连接）静默等待 bind_result，不发 profile_list
  → 客户端开始心跳

| 模式 | 凭证 | 能力 | 何时用 |
|------|------|------|--------|
| 临时连接 | `code={bind_code}` | 仅接收 `bind_result`，不能创建 session | 绑定流程中 |
| 正式连接 | `device_token={token}` | 全部能力 | 绑定完成后 + 日常运行 |

### 4.2 消息格式与版本协商

所有消息使用 JSON，外层统一结构：

```json
{
  "type": "<消息类型>",
  "seq": 142,
  "ts": 1718400000123,
  "session_id": "s_Vh4Kz8mPxQ2n",
  "data": { }
}
```

`session_id` 格式：`s_` + 12 位 nanoid（url-safe base62, 62^12 ≈ 3.2×10^21）。由发起方（iOS / Agent / Desktop）本地生成，通过 `start_session` 消息体携带。无需云端协调，全局唯一。MySQL UNIQUE 约束兜底碰撞（概率 ~10^-12）。

**协议版本协商**：

WSS 连接建立后，云端立即下发 `connected` 消息，其中携带 `protocol_version` 字段：

```json
{"type": "connected", "ws_session_id": "ws_xxx", "protocol_version": 1}
```

客户端收到后检查版本：
- `protocol_version <= 客户端支持的最高版本` → 正常通信，以服务端版本为准
- `protocol_version > 客户端支持的最高版本` → 客户端立即断开，提示用户升级

版本号规则：
- 递增整数（1, 2, 3, ...），不回退
- 新增消息类型 → 小版本号不变（向前兼容）
- 修改/删除已有消息类型 → 大版本号 +1（向后不兼容）
- 客户端在 WSS URL 中携带 `&version=1`（可选），服务端可据此调整行为

当前版本：**`protocol_version = 1`**

**消息格式错误处理**：

| 错误场景 | 检测方 | 处理 |
|---------|--------|------|
| 非 JSON 数据 | Cloud/Agent | WebSocket close code `1003` (Unsupported Data)，不回复 |
| 合法 JSON 但 `type` 未知 | Cloud/Agent | 返回 `error_notify: {code: "unknown_message_type", message: "<type>"}`，不关闭连接 |
| 已知 type 但必填字段缺失 | Cloud/Agent | 返回 `error_notify: {code: "invalid_message", message: "missing field: <field>"}` |
| `session_id` 指向不存在的会话 | Cloud/Agent | 返回 `error_notify: {code: "session_not_found"}` |
| `session_id` 指向已结束的会话 | Cloud/Agent | 返回 `error_notify: {code: "session_already_ended"}` |

错误消息统一通过 `error_notify` 返回，不关闭 WSS 连接（除非客户端反复发送非法数据，累计 5 次后断开）。

### 4.3 消息类型定义

> **说明**：以下为 WSS 消息类型（Client ↔ Cloud ↔ Agent）。IPC（Desktop ↔ Agent）的消息类型见 §3.2.9。

#### 客户端 → 服务端 (inbound)

> ⚠️ **权威实现声明**: 以下消息类型表为早期设计稿，**不完全等同于实际实现**。
> 权威来源为 `kn-cloud` Java 源码:
> - `kn-cloud-ws/.../component/MessageTypes.java` (15 种已实现消息类型)
> - `kn-cloud-ws/.../handler/KnWsHandler.java` (角色白名单 + 消息分发)
>
> 以下类型在 Java 代码中**未实现**（设计预留/规划中）:
> `kill_session`, `agent_info` (改用 HTTP headers), `agent_error` (不在 agent 白名单),
> `bind_result` (走 HTTP 轮询), `redeem`/`redeem_result` (走 HTTP API),
> `resize_pty`, `lock_session`, `write_file`, `read_output_log`,
> `device_status`, `state_change`, `profile_update`, `current_state`, `missed_messages`
>
> **Agent 出站白名单** (Java ALLOWED_MESSAGES):
> `ping, session_created, session_ended, output, profile_list, session_interrupted`
>
> **Agent 入站** (cloud → agent):
> `pong, connected, start_session, input, ctrl, profile_list_ack, error_notify`

| type | 说明 | data 字段 | 实现状态 |
|------|------|-----------|---------|
| `ping` | 心跳 | `{}` | ✅ |
| `start_session` | 创建新 AI 会话 | `{ session_id, tool, profile, cwd, cols, rows }` | ✅ |
| `profile_list` | Agent 上报可用 profile 列表 | `{ profiles: [{name, tool, desc}] }` | ✅ |
| `input` | 用户输入文本 | `{ session_id, text }` | ✅ |
| `ctrl` | 控制信号 | `{ session_id, signal: "ctrl_c"\|"ctrl_d"\|"ctrl_z" }` | ✅ |
| `kill_session` | 强制结束会话 | 实际走 session_ended | ❌ Java 未实现 |

#### 服务端 → 客户端 (outbound)

| type | 说明 | data 字段 | 实现状态 |
|------|------|-----------|---------|
| `pong` | 心跳响应 | `{}` | ✅ |
| `connected` | WSS 连接建立成功 | `{ ws_session_id, protocol_version? }` | ✅ |
| `start_session_ack` | 收到 start_session 并已通知 Agent | `{ session_id, session_nid }` | ✅ (仅 mobile) |
| `session_created` | Agent 确认 PTY 已就绪 | `{ session_id, session_nid }` | ✅ |
| `session_ended` | AI 会话已结束 | `{ session_id, reason }` | ✅ |
| `session_interrupted` | Agent crash 导致会话丢失 | `{ sessions: [...] }` | ✅ |
| `output` | PTY 原始输出 (v1: raw ANSI) | `{ to_session_id: Long, ansi_text }` | ✅ |
| `error_notify` | 服务端错误通知 | `{ code, message }` | ✅ |
| `agent_error` | Agent 异常通知 | 不在 Java 白名单 | ❌ Java 未实现 |

### 4.4 消息频率限制

**限流维度**：per WSS 连接。每个客户端的 WebSocket 连接独立计数，不在连接之间共享限额。实现用本地 `ConcurrentHashMap`（不需要 Redis），连接断开时自动清除计数器。

| 消息类型 | 限制 | 超限处理 |
|----------|------|---------|
| `start_session` | 10 次/分钟 | 返回 `error_notify: rate_limited` |
| `input` | 20 条/秒 | 丢弃 + ack `{msg_seq, dropped: true}`，客户端提示"未送达"自动重发 |
| `ctrl` | 5 次/秒 | 丢弃 + ack `{msg_seq, dropped: true}` |

**规则**：
- ack 本身**不受限流**——否则 dropped 通知到达不了客户端
- `output`（PTY 推送）不受限流——丢几帧屏幕刷新太快看不出来
- `ctrl` dropped 后客户端可立即重试，不影响用户体验

### 4.5 消息序列号与确认

```
每条 outbound 消息带 seq（会话内单调递增）

客户端收到后发送 ack：
  → {type: "ack", data: {msg_seq: 142}}

服务端在限流丢弃消息时同样返回 ack：
  → {type: "ack", data: {msg_seq: 142, dropped: true}}  // 客户端看到 dropped 后自动重发

服务端记录最大已确认 seq。
断线重连时，客户端发送 {last_ack_seq: 142}，
服务端补推 143 及之后的消息。
```

---

## 5. 可靠性设计

### 5.1 WebSocket 断线重连

```
Agent                                    云服务
  │                                        │
  │── connect(device_token, machine_id) ──▶│
  │◀─ session_id ──────────────────────────│  ← 分配会话 ID（断线不变）
  │                                        │
  │  ═══════ 正常工作 ═══════════════     │
  │                                        │
  │  ═══════ 断线 ════════════════════     │
  │                                        │
  │  等待: 1s → 2s → 4s → 8s → 16s → 30s  │  ← 指数退避 (cap 30s)
  │                                        │
  │── reconnect(session_id, last_ack) ────▶│
  │◀─ missed_messages ─────────────────────│  ← 补推断线期间的消息
  │◀─ current_state ───────────────────────│  ← 当前 PTY 状态快照
```

**心跳参数**：
- ping 间隔：15 秒
- ping 超时：90 秒（6 个心跳周期，扛住短暂休眠）
- 重连间隔：指数退避 1s → 2s → 4s → 8s → 16s → 30s，无限重试

**休眠恢复**：Agent 重连后立即 `kill(pid, 0)` 检查所有活跃 session 的 AI CLI 进程：
- 进程存活 → 通过 WSS 重新上报 session 状态（云端恢复 `running`）
- 进程已死 → 上报 `session_ended {reason: "interrupted"}`（云端标记 `failed`）
- 云端 `session failed` 的最后判定阈值从 5 分钟延长到 **30 分钟**，防止休眠期间误杀

### 5.2 消息可靠投递

| 层 | 策略 |
|----|------|
| 传输层 | WSS (TLS 1.3)，TCP 保证可靠传输 |
| 应用层 | msg_seq 单调递增 + ack 确认 + 去重 |
| 离线缓存 | Redis LIST，Agent 断线时缓存最多 1000 条 |
| 弱网 | Agent 端批量合并 PTY 碎片（100ms 窗口 或 64KB 积压，任一达到即 flush）；超过 10KB 的单次输出分片推送 |
| 超时重传 | 用户输入/指令类消息 5s 无 ack 自动重发；PTY 输出流消息允许丢失 |

### 5.3 Agent 进程恢复

```
launchd KeepAlive → Agent 崩溃 → 5s 后自动重启

重启恢复流程：
1. Agent 启动 → 读取本地 session checkpoint
2. 连接云端 WSS → 拉取离线消息
3. 检查是否有残留的活跃会话
4. 如果有 → 推状态给 iOS: "会话已中断，AI 进程已丢失"
5. iOS 显示恢复选项: [重新执行上一条指令] [查看最后输出] [关闭会话]
```

**会话快照 (每 30s)**：

```json
{
  "agent_state": "running",
  "sessions": [
    {
      "session_id": "s_xxx",
      "last_input": "重构 auth.ts 的 JWT 验证",
      "last_output_snippet": "正在分析...",
      "cwd": "/Users/xxx/project",
      "tool": "claude",
      "profile": "deepseek"
    }
  ]
}
```

**Checkpoint 写入约束**：
- 原子写：先写 `.tmp` 文件，`fsync` 后 `rename`，与 config.yaml 的写入模式一致
- `last_input` 截断到 200 字符，`last_output_snippet` 截断到 500 字符，防止无限膨胀

**诚实原则**：PTY 进程死后，AI 的内存状态无法恢复。但可以恢复上下文（cwd、上次输入、输出历史），重放进新的 AI 会话。Agent 重启后 iOS 收到 `session_interrupted` 事件，显示"连接中断，AI 会话已丢失" + [重新执行上一条指令] [关闭会话]。

**已知痛点**：长时间 AI 任务（如批量重构）中断后需从头重跑，体验差。v2 计划从 `output.log` 提取上下文摘要注入新 session，减少重复工作。

### 5.4 云服务重启惊群预防

所有 Agent 同时断线 → 错峰重连 (随机延迟 0-3s)，防止同时冲击云服务。

---

## 6. 安全设计

### 6.1 设备绑定流程

**本质**：扫码登录的变体——手机已登录（有 JWT），帮电脑"授权登入"。跟微信网页版扫码登录同原理。

```
Mac (Desktop/CLI)       Agent (WSS)             云服务                   iOS
 │                         │                       │                       │
 │ 1. 点击"绑定设备"        │                       │                       │
 │──IPC→ kn agent bind     │                       │                       │
 │                         │                       │                       │
 │                         │ 2. HTTP POST           │                       │
 │                         │   /api/v1/device/      │                       │
 │                         │   bind-init            │                       │
 │                         │   {machine_id}         │                       │
 │                         │──────────────────────▶│                       │
 │                         │                       │ 3. 生成 bind_code     │
 │                         │◀── {bind_code,         │   Redis TTL 5min      │
 │                         │      expires_in} ──────│                       │
 │                         │                       │                       │
 │◀── bind_code ──────────│                       │                       │
 │                         │                       │                       │
 │ 4. 展示二维码            │                       │                       │
 │   (code + hostname)     │                       │                       │
 │                         │                       │                       │
 │                         │ 5. WSS connect         │                       │
 │                         │   ?code=xxx            │                       │
 │                         │   &machine_id=yyy      │                       │
 │                         │──────────────────────▶│                       │
 │                         │                       │ 6. 验证 code+machine_id │
 │                         │◀── connected (临时) ───│   匹配 → WSS 建立      │
 │                         │                       │                       │
 │                         │                       │  7. 📱 扫码             │
 │                         │                       │  POST /api/v1/device/  │
 │                         │                       │  bind-confirm          │
 │                         │                       │  {code, JWT}          │
 │                         │                       │◀──────────────────────│
 │                         │                       │                       │
 │                         │                       │ 8. 验证全部：          │
 │                         │                       │   code + JWT(user)    │
 │                         │                       │   + machine_id + 限额    │
 │                         │                       │   → 绑定！             │
 │                         │                       │──────────────────────▶│
 │                         │                       │                       │
 │                         │◀── bind_result ────────│                       │
 │                         │   {device_token}       │                       │
 │                         │                       │                       │
 │ 5. 存 device_token      │                       │                       │
 │   ~/.kn/agent/          │                       │                       │
 │   device_token (0600)   │                       │                       │
 │                         │                       │                       │
 │   WSS 重新连接:          │                       │                       │
 │   ?device_token=xxx     │                       │                       │
 │   &machine_id=yyy       │                       │                       │
 │   → 正式连接             │                       │                       │
 │                         │                       │                       │
 │◀── 绑定成功 ────────────│                       │                       │
```

**安全要点**（与扫码登录相同）：

| 环节 | 措施 |
|------|------|
| `/bind-init` HTTP | Nginx `limit_req` 单 IP 3次/5min，防刷 |
| WSS 临时连接 | `?code=xxx` 作临时凭证，只能"进门"拿不到 device_token |
| 最终授权 | 必须 iOS 扫码 + **用户 JWT** 确认，code + machine_id + JWT 三方匹配 |
| bind_code 生命周期 | Redis TTL 5min，过期自动销毁 |
| bind_code 不能爆破 | 6位数字 100万可能 × 3次/5min 限流 ÷ 5min 窗口 = 不可能 |

**二维码内容**：
```json
{
  "bind_code": "482916",
  "hostname": "zhaojun-macstudio"
}
```

**Mac 端展示方式**：
- kn Desktop：弹窗直接显示二维码图片
- CLI：`kn agent bind` → 打印 ASCII QR + 可选 `--open` 打开浏览器显示

### 6.2 认证机制

```
access_token:  15 分钟过期    → 携带用户身份 + 权限（JWT，无状态）
refresh_token: 30 天过期      → 存在 Redis: refresh:token:{userId}
                                仅 iOS 端存 Keychain。Agent 不用 refresh_token，用 device_token 连 WSS
```

**Token 生命周期**：
- `access_token` 过期 → iOS 用本地存的 `refresh_token` 调 `POST /api/v1/auth/refresh`
- 云端查 Redis 比对通过 → 同时签发：
  - 新 `access_token`（15 分钟有效）
  - 新 `refresh_token`（30 天有效，覆盖 Redis 旧值，**旧 token 立即作废**）
- 客户端收到后需用新 `refresh_token` 替换 Keychain 中的旧值
- **Refresh Token Rotation**（RFC 6749 建议）：防止泄露的 refresh_token 被无限滥用
  - 每次刷新时轮换 refresh_token，旧值作废
  - 如果攻击者和合法用户各自持有同一 refresh_token，其中一方刷新后另一方立即失败
  - 双方都失败 → 说明 token 被泄露，需重新登录
- Redis 中 `refresh:token:{userId}` 过期（30 天未刷新）→ 要求重新登录
- 登出 / 修改密码 → 云端删除 `refresh:token:{userId}` → 该用户 refresh token 即时失效

### 6.3 命令安全模型

**Agent 不执行任意 shell 命令。** iOS 发送的是"操作意图"，Agent 映射为预定义操作：

| iOS 发送 | Agent 执行 |
|----------|-----------|
| `{type: "start_session", tool: "claude", profile: "deepseek", cwd: "/project"}` | 用指定 profile 名启动 Claude CLI。Agent 本地从 config.yaml 匹配完整 env vars，不经过网络传输 |
| `{type: "input", text: "重构 auth.ts\n"}` | 写入 PTY stdin |
| `{type: "ctrl", signal: "ctrl_c"}` | 发送 SIGINT 到 PTY |
| `{type: "kill_session"}` | → SIGTERM → 超时 5s → SIGKILL |
| `{type: "read_file", path: "src/auth.ts"}` | 读取文件内容（仅限 cwd 子树） |
| `{type: "write_file", path: "src/...", content: "..."}` | 写入文件（需用户 iOS 端二次确认） |

### 6.4 设备防共享

**问题**：一个付费账号，N 个人用——A 用完 B 用，或者 token 拷贝到多台机器。

**四层防线**：

#### Layer 1: 会员等级硬限制

```
绑定设备时：
1. 查询用户会员等级
2. 查询当前已绑定且未解绑的设备数
3. 超过上限 → 拒绝绑定：
   {error: "device_limit_reached", limit: 3, current: 3,
    hint: "请解绑一台旧设备后再绑定，解绑后有 24h 冷却期"}
```

#### Layer 2: 解绑冷却期

```
用户解绑设备 → 写入 unbound_at 时间戳
24h 内尝试绑新设备 → 拒绝:
  {error: "unbind_cooldown", remaining_seconds: 43200,
   hint: "解绑后需等待 24 小时才能绑定新设备"}
```

- 同一设备重新绑定（machine_id 匹配）不受冷却限制
- Enterprise 用户不受冷却限制

#### Layer 3: 设备指纹校验

Agent 首次绑定时上报设备指纹（单因子，用 macOS 唯一硬件 ID）：

```
Agent 采集:
  machine_id = IOPlatformUUID  // macOS 唯一硬件标识，存 NVRAM，仅全盘抹除后变化
```

为什么不用 hostname / MAC：
- `hostname` — 用户随时可改，无安全价值
- `mac_addr` — 换网卡、USB 适配器、macOS 私有 Wi-Fi 地址随机化都会导致变化

**连接时校验**：

```
Agent 每次连接 WSS 时：
1. JWT 验证通过
2. 设备指纹验证：
   - 查询 DB: device_token → 存储的 machine_id
   - Agent 上报 current_machine_id
   ├── 匹配 → 放行（同一台机器）
   └── 不匹配 → 拒绝连接:
       {error: "machine_id_mismatch",
        detail: "token 被拷贝到其他设备，或该设备已抹盘重装，请重新绑定"}
```

**抹盘重装场景**：IOPlatformUUID 仅在全盘抹除重装 macOS 后会变。此时旧 token 失效，用户需在 iOS 上解绑旧设备记录，重新扫码绑定。这是低频操作（几年一次），代价可接受。

**hostname 保留**：`kn_device.hostname` 仍存储并展示（方便用户在设备列表辨识"办公室 Mac Studio"），但不参与指纹校验。

#### Layer 4: 并发连接检测

```
同一 device_token 只能有一个活跃 WSS 连接。

Agent 连接时：
  Redis SETNX device:conn:{device_id} {connection_id} EX 60

如果已有连接 ≠ 当前 connection_id:
  → 可能旧连接残留（心跳超时但连接未完全断开）
  → 先发 kick 给旧连接 → 等待 5s → SET 新连接
  
如果旧连接 IP 与当前连接 IP 不同:
  → 标记异常: device:anomaly:{device_id} = "ip_mismatch"
  → 通知用户（iOS 推送 + 邮件）
```

### 6.5 profile env var 加密存储

`~/.kn/config.yaml` 中的 env var value（如 `OPENAI_API_KEY`）是明文存储的，任何人读文件即拿到 API Key。

**方案**：AES-256-GCM 加密 env var value，主密钥存 macOS Keychain。

```
保存 profile:
  value' = AES-256-GCM(key, value)
  → config.yaml: KEY: "kn:v1:<hex_ciphertext>"

读取 profile:
  config.yaml: KEY: "kn:v1:<hex_ciphertext>"
  → AES-256-GCM 解密 → 原始 value → 注入 PTY 环境

主密钥:
  macOS Keychain item: com.kn.agent/config-key
  首次自动生成 (SecRandomCopyBytes, 256-bit)
  三个组件 (Agent / Desktop / Python CLI) 通过 Keychain API 读取
```

**加密粒度**：只加密 env var 的 **value**。key 名和 profile 名保留明文（可搜索、可编辑）。

**向前兼容**：`kn-common` 解密时识别 `kn:v1:` 前缀。无此前缀的旧明文 value 正常读取，下次保存时自动升级为加密。所有组件无需迁移脚本。

**实现位置**：`common/src/config_crypto.rs`，`kn-common` 新增 `aes-gcm` + `security-framework` (macOS Keychain) 依赖。

### 6.6 其他安全措施

| 威胁 | 措施 |
|------|------|
| WSS 被中间人劫持 | TLS 1.3 + iOS 端 certificate pinning |
| device_token 泄露 | 本地文件 0600；设备指纹校验 + IP 异常检测 |
| 重放攻击 | 每条指令带 `nonce` (单调递增)，同一 nonce 只处理一次 |
| 暴力破解 | Redis 限流：5 次失败 → IP + 账号锁定 15 分钟 |
| iOS 后台截屏 | 敏感信息（API Key）在 iOS 任务切换器中模糊处理 |

---

## 7. ANSI 解析与 Chat 模式

##### v1: Terminal 模式（当前阶段）

Agent **不做 ANSI 解析**，原样转发 PTY 输出。iOS 端用 WKWebView + xterm.js 渲染完整终端，与电脑端体验一致。

- Agent 代码量减少 ~30%，无解析不准的风险
- Claude/Codex/Qoder 自适应终端 cols，手机窄屏正常排版
- 所有 CLI 输出 100% 完整展示，不会丢失任何信息

**xterm.js 配置与 addons**：

```js
// 必须的 addons
import { FitAddon } from 'xterm-addon-fit';        // 自适应屏幕尺寸
import { WebLinksAddon } from 'xterm-addon-web-links'; // 链接可点击
import { Unicode11Addon } from 'xterm-addon-unicode11'; // CJK/emoji 正确渲染

const term = new Terminal({
  scrollback: 5000,  // 最多保留 5000 行，超出旧行自动丢弃
  fontSize: 12,
  fontFamily: "'JetBrains Mono', monospace",
  theme: { background: '#0a0c0f', foreground: '#e0e0e0', cursor: '#00e676' },
});

term.loadAddon(new FitAddon());
term.loadAddon(new WebLinksAddon());
term.loadAddon(new Unicode11Addon());
term.open(document.getElementById('terminal'));
fitAddon.fit();  // 自动计算 cols/rows 填满屏幕
```

**内存控制**：xterm.js 没有真正的虚拟滚动，整个 buffer 存在 JS 内存中。`scrollback: 5000` 限制最多保留 5000 行，约 300-500KB 纯文本 + ANSI 转义序列 ≤ 1-2MB，WKWebView 可稳定运行。超出旧行自动丢弃。

##### v2: Chat 模式（后期可选增强）

Agent 增加 ANSI 解析器，提取结构化事件（text/progress/diff/code/error/confirm），iOS 端渲染为原生聊天气泡卡片。Terminal 模式保留作为兜底。

---

## 8. 并发与多客户端

### 8.1 多来源输入合并

```
iOS 输入 ──▶  ┐
               ├──▶ InputMerger (FIFO) ──▶ PTY stdin ──▶ AI CLI
本地输入 ──▶  ┘
```

输入按到达顺序写入 PTY，不合并、不排队。跟两个人在同一个 tmux 里打字体验相同。

每条输入标记来源 (`[iOS]` / `[local]`)，在其他客户端的终端视图中显示来源前缀。

### 8.2 输出广播

```
PTY stdout ──▶ Output Fan-out ──┬──▶ WSS → Cloud → iOS
                                ├──▶ IPC → kn Desktop
                                └──▶ 写入本地 session log
```

IPC 客户端（kn Desktop）收到完整输出流。WSS 客户端（iOS）在弱网下可能丢帧——PTY 输出允许丢失（屏幕刷太快少几行无感知），用户输入/指令类消息走 ack 确认保证可靠投递。

### 8.3 多 Session 并发

一个 Agent 管理多个 AI 会话：

```
Agent (单进程, tokio runtime)
  ├── SessionManager
  │   ├── Session "s1" (claude + project-A)
  │   │   ├── PTY handle
  │   │   └── output buffer
  │   └── Session "s2" (codex + project-B)
  │       ├── PTY handle
  │       └── ...
  └── WS Connection (单连接，复用)
```


### 8.4 会话锁定

提供"锁定"功能：一个人操作时，其他人只读。

```
iOS 用户: 锁定会话 → {type: "lock_session"}
Agent: 锁定 → 其他客户端收到 state_change → 显示 🔒 图标
其他客户端: 尝试输入 → Agent 返回 error: "session_locked"
```

---

## 9. 存储与持久化

### 9.1 数据分布

| 数据 | 存储 | 保留策略 |
|------|------|---------|
| 用户/会员 | MySQL (云端) | 永久 |
| 设备信息 | MySQL (云端) | 永久 |
| 会话元数据 | MySQL (云端) | 永久 |
| 用户输入记录 | MySQL `kn_message` (云端) | 90 天（每日凌晨定时清理过期记录） |
| PTY 原始输出 | Agent 本地 `output.log` | 7 天 |
| Agent 状态/心跳 | Redis (云端) | 实时 |
| WSS 离线消息（操作事件，非 PTY 输出） | Redis (云端) | 最多 1000 条，7 天无连接过期 |

### 9.2 Agent 本地文件布局

```
~/.kn/
├── config.yaml                 # 现有：profile 配置
├── agent/
│   ├── device_token            # 长期凭证 (权限 0600)
│   ├── logs/
│   │   ├── agent.YYYY-MM-DD.log    # Agent 运行日志（每日翻转，保留 7 天）
│   ├── sessions/
│   │   ├── {session_id}/       # 目录名 = wire format 的 session_id (s_ + 12位 nanoid)
│   │   │   ├── metadata.json   # tool, profile, cwd, start_time
│   │   │   ├── output.log      # 原始 PTY stdout
│   │   │   └── checkpoint.json # 会话快照 (每 30s)
│   │   └── ...
│   └── versions/
│       ├── current/            # 当前版本 (symlink)
│       └── v1.0.0/
│           └── kn-agent
```

**output.log 清理**：Agent 定时任务（每天凌晨 3:00）遍历 `~/.kn/agent/sessions/` 目录，删除 7 天前已结束 session 的整个目录（含 `output.log`、`metadata.json`、`checkpoint.json`）。活跃 session 不受影响。

---

## 10. 异常场景处理矩阵

### 10.1 连接与进程异常

| 异常 | 检测方式 | 恢复策略 | 用户体验 |
|------|---------|---------|---------|
| WSS 断线 | 心跳超时 45s | 指数退避重连 + 补推离线消息 | iOS 显示"重连中..." |
| Agent crash | launchd 监控 | 5s 后自动重启 + 会话快照恢复 | "连接中断，AI 会话已丢失。[重新执行] [关闭]" |
| AI CLI 退出 | PTY 返回 EOF | Agent 推送 session_ended 事件 | "Claude 已退出 [重新启动]" |
| PTY 分配失败 | `openpty()` 返回错误 | Agent 通过 WSS 返回 `error_notify: pty_alloc_failed`，session 不创建 | "终端创建失败，请重试" |
| Shell 启动失败 | PTY spawn 后立即 EOF 或 exit≠0 | Agent 返回 `error_notify: shell_spawn_failed`，清理 PTY | "Shell 启动失败，检查 /bin/zsh" |
| AI CLI 二进制未找到 | `find_binary()` 三层 fallback 均无结果 | Agent 返回 `error_notify: cli_not_found {tool}`，session 不创建 | "未找到 {tool}，请确认已安装" |
| config.yaml 损坏 | profile 读取解析失败 | Agent 返回 `error_notify: config_parse_error`，session 不创建 | "配置文件损坏，请检查 ~/.kn/config.yaml" |
| 云服务重启 | Agent 心跳检测 | 所有 Agent 错峰重连 (0-3s 随机延迟) | 无明显感知 |
| macOS 休眠 | 心跳消失 | 唤醒后 Agent 重连 → kill(pid,0) 检查 AI CLI 存活 → 存活则恢复 running / 已死则标记 failed | "设备离线" → 唤醒后自动恢复（进程存活）或提示"会话中断"（进程已死） |
| 用户手动 kill Agent | launchd KeepAlive | 5s 后重启 (除非用户执行了 pause) | 短暂中断 |

### 10.2 网络异常

| 异常 | 处理 |
|------|------|
| 网络丢包 | TCP 层重传；应用层 ack 超时 5s 重发 + msg_id 去重 |
| 手机切网 (WiFi→4G) | 云端检测同 user 新连接 IP 变化 → 关联到原 session |
| 弱网 | Agent 端批量合并 (100ms / 64KB)；PTY 输出普通行允许丢失，重要消息走 ack 确认 |
| 长时间断网 | 离线消息缓存（最多 1000 条），恢复后批量推送 |

### 10.3 业务异常

| 异常 | 处理 |
|------|------|
| AI 需要用户输入 (Overwrite?) | Agent 检测 PTY 无新输出 >5s + 最后输出含 `[y/N]` 等交互提示 → APNs 通知用户打开 App |
| AI 长时间运行 (>30min) | Agent 持续推送进度事件；iOS 可能已挂起 → 完成后 APNs 推送 |
| Agent 版本过旧 | 云端返回 `agent_outdated` → Desktop 下次启动时自动同步替换 → 重启 Agent |
| 多设备同时接入同一会话 | 正常支持，FIFO 写入 PTY；提供锁定机制 |
| AI 输出量极大 | Agent 分片推送 (每片 ≤10KB → seq + total)；批量合并 (100ms / 64KB 任一达到即 flush) |
| Profile 被删除/修改 | Agent 检测配置变更 → 通知 iOS "Profile 已变更" |
| 会员到期（提前 1 天） | iOS 推送 + App 横幅提示续费 |
| 会员到期（当天） | 进入 24h 缓冲期：已有会话继续，禁止新建，到期 WSS 未断 |
| 会员到期（缓冲期过后） | 云端强制断开 Agent WSS → 所有 session 终止 → 标记 failed |
| 设备绑定超限 | 拒绝绑定，返回当前限额 + 已绑定数 + 解绑指引 |
| 解绑冷却期内绑新设备 | 拒绝绑定，返回剩余冷却时间 |
| 设备指纹不匹配 | 拒绝连接，记录异常事件，通知用户 |
| 同一 token 多地连接 | 踢旧连接 + IP 跳变告警 + 通知用户 |

---

## 11. 实施路线图

### Phase 1: 基础设施 (预计 2-3 周)

```
□ 云服务 — 项目骨架（kn-cloud-api + kn-cloud-ws + kn-cloud-common）
□ 云服务 — DB 初始化 SQL + schema 变更记录
□ 云服务 — 用户模块（注册/登录/JWT/会员等级）
□ 云服务 — 设备模块（绑定/解绑/冷却期/设备指纹校验/并发检测）
□ 云服务 — WebSocket 消息中继（连接管理/路由/心跳）
□ 云服务 — CI/CD: GitHub Actions 自动构建 → SSH 部署到服务器
□ 云服务 — systemd service 配置 (kn-cloud-api.service + kn-cloud-ws.service)
□ Agent — Rust 独立二进制骨架（launchd 安装/卸载）
□ Agent — 设备指纹采集（IOPlatformUUID，单因子）
□ Agent — WebSocket 客户端（连接时指纹上报 + 重连 + 心跳）
□ Agent — session 管理（创建/销毁 PTY）
□ Agent — 单元测试（state/fingerprint/proto/session，`cargo test --bin kn-agent`）
□ 云服务 — 单元测试（JUnit 5 + Mockito） + 集成测试（Testcontainers MySQL/Redis）
```

**部署方案（Phase 1）**：单服务器 JAR + systemd。两个独立进程共享 MySQL + Redis，Nginx 反向代理。详见 §附录 C.3。

### Phase 2: 核心功能 (预计 3-4 周)

```
□ Agent — 输入合并 + 输出广播
□ Agent — IPC server（Unix Socket，给 kn Desktop 用）
□ Agent — shell hook（ai() 自动路由）
□ Agent — IPC 集成测试（nc -U 连接 + 请求-响应验证）
□ 云服务 — 消息持久化（MySQL 存储聊天记录）
□ 云服务 — APNs 推送集成
□ 云服务 — WSS 协议集成测试（wscat 连接 + 消息收发验证）
□ iOS — 项目骨架 + 登录/注册 + Keychain
□ iOS — WebSocket 客户端 + 消息模型
```

### Phase 3: iOS UI (预计 2-3 周)

```
□ iOS — TerminalView (WKWebView + xterm.js) + InputAccessoryBar
□ iOS — 设备列表 + 绑定流程
□ iOS — 推送通知处理
□ iOS — UI 测试（XCTest + XCUITest 基础覆盖）
```

### Phase 4: 集成与打磨 (预计 2 周)

```
□ kn Desktop — Agent 模式适配（版本检测 + 启动时自动同步替换）
□ kn Desktop — Agent 二进制打包进 .app bundle
□ 端到端测试（绑定流程 + session 生命周期 + crash 恢复）
□ 异常恢复场景测试（断网/Agent crash/macOS 休眠/会员到期）
□ Python CLI 兼容加密 config — `lib/config.py` 集成 macOS Keychain + AES-GCM 解密，使 `bin/profile list -j` 能正确展示 `kn:v1:` 加密的 env var value
□ 性能优化（PTY 输出吞吐 + WSS 消息延迟 + 多 session 并发）
```

### Phase 5: 后期增强 (v2+)

```
□ Agent — ANSI 解析器 → iOS Chat 模式（结构化气泡卡片）
□ 会员 — 时长套餐扩展
□ 微服务拆分（按需）
```

---

## 附录 A: 技术风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| iOS 后台限制 | 无法保持长连接 | 异步模式 + APNs 推送 |
| 云服务成本 | 长期运营费用 | Phase 1 完成后评估；后期可加 P2P 直连 |
| xterm.js 在移动端性能 | 大量输出时可能卡顿 | 输出分片推送 + scrollback 行数限制 (5000 行)，超出旧行自动丢弃 |

## 附录 B: 备选方案记录

| 方案 | 结论 |
|------|------|
| P2P 直连 (Tailscale 模式) | 暂不采用，需要云服务做用户系统；将来可做混合模式优化 |
| WebRTC 数据通道 | 暂不采用，中继模式更稳定，将来大文件传输可引入 |
| React Native (跨平台) | 暂不采用，优先 iOS 原生 |
| Flutter (跨平台) | 暂不采用，优先 iOS 原生 |

## 附录 C: 已知技术债务与后续规划

### C.1 运维与可观测性

初期手动运维，不建立自动化监控体系。以下记录为后续需补齐：

- Agent 日志仅本地 `~/.kn/agent/agent*.log`，无远程采集
- 云服务日志 + 监控告警方案（日志平台、Agent 离线告警、API 错误率等）
- 数据库备份策略

### C.2 数据库 Schema 迁移

初期无 schema 变更管理工具（Flyway/Liquibase），直接执行 SQL。表结构稳定后再引入。

### C.3 降级与限流

初期不做自动降级/限流。用户量上来后按三层熔断逐步实施（Level 1 新 session 限流 → Level 2 拒绝新 WSS → Level 3 仅维持已有连接，**已有 session 永不被 kill**）。

### C.4 实施路线图

Phase 1-4 时间估算不变。实际周期取决于投入程度，代码实现由项目 maintainer 全栈完成。
