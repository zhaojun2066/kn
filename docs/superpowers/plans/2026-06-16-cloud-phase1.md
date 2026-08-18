# Cloud Phase 1 — Spring Boot 骨架 + 用户系统 + 设备绑定 + WSS 中继

> ⚠️ **权威版本在 `../kn-cloud/docs/2026-06-16-cloud-phase1.md`**。此文件为 kn monorepo 中的只读副本，修改请到 kn-cloud repo。
>
> ⚠️ **协议设计以实际 Java 代码为准**: 本文档中部分消息类型（如 `kick_device`、绑定时的 WSS `bind_result` 等）在实际 Java 实现中可能未采用或使用了不同方案。开发 kn-agent 时请以 `kn-cloud` 项目中的 `MessageTypes.java` + `KnWsHandler.java` 为权威协议定义。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]` checkbox syntax.

**Goal:** 在私有 repo `kn-cloud` 初始化 Spring Boot 项目，实现用户注册/登录/JWT、设备绑定/解绑（含冷却期）、WSS 连接管理/消息中继。

**Architecture:** `kn-cloud-api` (HTTP) + `kn-cloud-ws` (WebSocket) 两个独立进程，共享 `kn-cloud-common`。Nginx 反向代理，Spring Filter 鉴权。

**Tech Stack:** Java 21 + Spring Boot 3.x + MyBatis Plus + MySQL + Redis + Nginx

**Prerequisites:** 在 GitHub 创建私有 repo `kn-cloud`，clone 到本地。

---

## Pre-flight: 项目骨架

### Task 1: Maven 多模块项目初始化

**Files:**
- Create: `pom.xml` (parent)
- Create: `kn-cloud-common/pom.xml`
- Create: `kn-cloud-api/pom.xml`
- Create: `kn-cloud-ws/pom.xml`

- [ ] **Step 1: 父 pom.xml**

创建 `kn-cloud/pom.xml`：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>dev.kn</groupId>
    <artifactId>kn-cloud</artifactId>
    <version>0.1.0</version>
    <packaging>pom</packaging>

    <parent>
        <groupId>org.springframework.boot</groupId>
        <artifactId>spring-boot-starter-parent</artifactId>
        <version>3.3.0</version>
    </parent>

    <modules>
        <module>kn-cloud-common</module>
        <module>kn-cloud-api</module>
        <module>kn-cloud-ws</module>
    </modules>

    <properties>
        <java.version>21</java.version>
        <mybatis-plus.version>3.5.7</mybatis-plus.version>
        <jjwt.version>0.12.6</jjwt.version>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.projectlombok</groupId>
            <artifactId>lombok</artifactId>
            <optional>true</optional>
        </dependency>
        <dependency>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-test</artifactId>
            <scope>test</scope>
        </dependency>
    </dependencies>
</project>
```

- [ ] **Step 2: kn-cloud-common pom.xml**

创建 `kn-cloud/kn-cloud-common/pom.xml`：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>
    <parent><groupId>dev.kn</groupId><artifactId>kn-cloud</artifactId><version>0.1.0</version></parent>
    <artifactId>kn-cloud-common</artifactId>

    <dependencies>
        <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency>
        <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-data-redis</artifactId></dependency>
        <dependency><groupId>com.baomidou</groupId><artifactId>mybatis-plus-spring-boot3-starter</artifactId><version>${mybatis-plus.version}</version></dependency>
        <dependency><groupId>com.mysql</groupId><artifactId>mysql-connector-j</artifactId></dependency>
        <dependency><groupId>io.jsonwebtoken</groupId><artifactId>jjwt-api</artifactId><version>${jjwt.version}</version></dependency>
        <dependency><groupId>io.jsonwebtoken</groupId><artifactId>jjwt-impl</artifactId><version>${jjwt.version}</version><scope>runtime</scope></dependency>
        <dependency><groupId>io.jsonwebtoken</groupId><artifactId>jjwt-jackson</artifactId><version>${jjwt.version}</version><scope>runtime</scope></dependency>
    </dependencies>
</project>
```

- [ ] **Step 3: kn-cloud-api pom.xml** (Spring Boot 应用，依赖 common)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <parent><groupId>dev.kn</groupId><artifactId>kn-cloud</artifactId><version>0.1.0</version></parent>
    <artifactId>kn-cloud-api</artifactId>

    <dependencies>
        <dependency><groupId>dev.kn</groupId><artifactId>kn-cloud-common</artifactId><version>0.1.0</version></dependency>
        <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter</artifactId></dependency>
    </dependencies>

    <build><plugins><plugin><groupId>org.springframework.boot</groupId><artifactId>spring-boot-maven-plugin</artifactId></plugin></plugins></build>
</project>
```

- [ ] **Step 4: kn-cloud-ws pom.xml** (WebSocket 应用，依赖 common)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <parent><groupId>dev.kn</groupId><artifactId>kn-cloud</artifactId><version>0.1.0</version></parent>
    <artifactId>kn-cloud-ws</artifactId>

    <dependencies>
        <dependency><groupId>dev.kn</groupId><artifactId>kn-cloud-common</artifactId><version>0.1.0</version></dependency>
        <dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-websocket</artifactId></dependency>
    </dependencies>

    <build><plugins><plugin><groupId>org.springframework.boot</groupId><artifactId>spring-boot-maven-plugin</artifactId></plugin></plugins></build>
</project>
```

- [ ] **Step 5: 初始化目录结构 + 主类**

创建 `kn-cloud-common/src/main/java/dev/kn/cloud/common/`、`kn-cloud-api/src/main/java/dev/kn/cloud/api/`、`kn-cloud-ws/src/main/java/dev/kn/cloud/ws/` 目录。

`kn-cloud-api/src/main/java/dev/kn/cloud/api/KnCloudApiApplication.java`：

```java
package dev.kn.cloud.api;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.scheduling.annotation.EnableScheduling;

@SpringBootApplication(scanBasePackages = "dev.kn.cloud")
@EnableScheduling  // 启用 @Scheduled 定时任务（MembershipScheduler）
public class KnCloudApiApplication {
    public static void main(String[] args) {
        SpringApplication.run(KnCloudApiApplication.class, args);
    }
}
```

`kn-cloud-ws/src/main/java/dev/kn/cloud/ws/KnCloudWsApplication.java`：

```java
package dev.kn.cloud.ws;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication(scanBasePackages = "dev.kn.cloud",
    // 排除 AuthFilter：WS 进程不处理 JWT，鉴权在 handleAgentConnect/handleUserConnect 中完成
    exclude = {dev.kn.cloud.api.config.AuthFilter.class})
public class KnCloudWsApplication {
    public static void main(String[] args) {
        SpringApplication.run(KnCloudWsApplication.class, args);
    }
}
```

- [ ] **Step 6: 编译验证**

```bash
cd kn-cloud && mvn clean compile
```

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: init Spring Boot multi-module project"
```

---

### Task 2: 数据库 Entity + Mapper

**Files:**
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/entity/KnUser.java`
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/entity/KnDevice.java`
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/entity/KnSession.java`
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/entity/KnMessage.java`
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/entity/KnRedeemCode.java`
- Create: Mapper XML + Mapper interfaces
- Create: `application.yml` (DB + Redis 配置)

**(详细代码省略，按设计文档 §3.1.3 SQL schema 生成 Entity)**

- [ ] **Step 1: 用 MyBatis Plus 代码生成器或手写 Entity**

以 `KnUser.java` 为例：

```java
package dev.kn.cloud.common.entity;

import com.baomidou.mybatisplus.annotation.*;
import lombok.Data;
import java.time.LocalDate;
import java.time.LocalDateTime;

@Data
@TableName("kn_user")
public class KnUser {
    @TableId(type = IdType.AUTO)
    private Long id;
    private String email;
    private String phone;
    private String password;  // bcrypt
    private String nickname;
    private String membership;  // trial / pro / enterprise
    private LocalDate trialExpiresAt;
    private LocalDate membershipExpiresAt;
    private String status;  // active / expired / disabled
    private LocalDateTime createdAt;
    private LocalDateTime updatedAt;
}
```

（其余 Entity 按设计文档表结构照写。）

- [ ] **Step 2: 创建 Mapper**

```java
package dev.kn.cloud.common.mapper;

import com.baomidou.mybatisplus.core.mapper.BaseMapper;
import dev.kn.cloud.common.entity.KnUser;
import org.apache.ibatis.annotations.Mapper;

@Mapper
public interface KnUserMapper extends BaseMapper<KnUser> {
}
```

- [ ] **Step 3: Spring Boot 多环境配置 (dev / prod)**

Spring Boot 通过 `spring.profiles.active` 切换环境。共享配置放 `application.yml`，差异化配置放 `application-{profile}.yml`，启动时指定 profile。

**文件结构**：

```
kn-cloud-api/src/main/resources/
├── application.yml          # 共享配置（所有环境通用）
├── application-dev.yml      # 本地开发
└── application-prod.yml     # 生产环境
```

`kn-cloud-api/src/main/resources/application.yml`（共享配置）：

```yaml
spring:
  jackson:
    property-naming-strategy: SNAKE_CASE

mybatis-plus:
  mapper-locations: classpath*:/mapper/**/*.xml
  global-config:
    db-config:
      table-prefix: kn_
```

`kn-cloud-api/src/main/resources/application-dev.yml`（本地开发）：

```yaml
server:
  port: 8080

spring:
  datasource:
    url: jdbc:mysql://localhost:3306/kn_cloud?useSSL=false&serverTimezone=UTC&allowPublicKeyRetrieval=true
    username: root
    password: 12345678
  data:
    redis:
      host: localhost
      port: 6379
      # 本地 Redis 无密码

kn:
  jwt:
    secret: dev-secret-do-not-use-in-production-256bit-minimum!!!

logging:
  level:
    dev.kn: DEBUG
```

`kn-cloud-api/src/main/resources/application-prod.yml`（生产环境）：

```yaml
server:
  port: 8080

spring:
  datasource:
    url: jdbc:mysql://${DB_HOST:localhost}:3306/kn_cloud?useSSL=true&serverTimezone=UTC
    username: ${DB_USER}
    password: ${DB_PASS}
  data:
    redis:
      host: ${REDIS_HOST:localhost}
      port: ${REDIS_PORT:6379}
      password: ${REDIS_PASS:}

kn:
  jwt:
    secret: ${JWT_SECRET}  # 生产密钥通过环境变量注入，不写死在文件里
  apns:
    team-id: ${APNS_TEAM_ID}
    key-id: ${APNS_KEY_ID}
    key: ${APNS_KEY}  # p8 文件内容，通过 K8s Secret / systemd EnvironmentFile 注入
    production: true

logging:
  level:
    dev.kn: INFO
```

**WS 服务同理**，`kn-cloud-ws/src/main/resources/` 下创建同名三个文件：

```yaml
# application.yml (共享)
spring:
  jackson:
    property-naming-strategy: SNAKE_CASE

# application-dev.yml
server:
  port: 8081
spring:
  data:
    redis:
      host: localhost
      port: 6379

# application-prod.yml
server:
  port: 8081
spring:
  data:
    redis:
      host: ${REDIS_HOST:localhost}
      port: ${REDIS_PORT:6379}
      password: ${REDIS_PASS:}
```

**启动方式**：

```bash
# 本地开发
java -jar kn-cloud-api.jar --spring.profiles.active=dev

# 生产
java -jar kn-cloud-api.jar --spring.profiles.active=prod
```

- [ ] **Step 4: 创建数据库初始化 SQL**

创建 `kn-cloud/deploy/init.sql`：

```sql
-- ============================================================================
-- kn-cloud 数据库初始化
-- 用法: mysql -u root -p12345678 < init.sql
-- ============================================================================

CREATE DATABASE IF NOT EXISTS kn_cloud
  DEFAULT CHARACTER SET utf8mb4
  DEFAULT COLLATE utf8mb4_unicode_ci;

USE kn_cloud;

-- ============================================================================
-- 1. 用户表
-- ============================================================================
CREATE TABLE IF NOT EXISTS kn_user (
    id                   BIGINT PRIMARY KEY AUTO_INCREMENT COMMENT '自增主键',
    email                VARCHAR(255) NOT NULL COMMENT '登录邮箱',
    phone                VARCHAR(20) DEFAULT NULL COMMENT '手机号（预留）',
    password             VARCHAR(255) NOT NULL COMMENT 'bcrypt 哈希',
    nickname             VARCHAR(100) DEFAULT NULL COMMENT '用户昵称',
    membership           VARCHAR(20) NOT NULL DEFAULT 'trial' COMMENT '会员等级: trial / pro / enterprise',
    trial_expires_at     DATE DEFAULT NULL COMMENT '试用到期日（注册 +30 天）',
    membership_expires_at DATE DEFAULT NULL COMMENT '付费会员到期日',
    status               VARCHAR(20) NOT NULL DEFAULT 'active' COMMENT '账号状态: active / expired / disabled',
    created_at           DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
    updated_at           DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
    -- 索引
    UNIQUE INDEX idx_email (email),
    INDEX idx_membership (membership),
    INDEX idx_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='用户表';

-- ============================================================================
-- 2. 设备表 (Agent 绑定的 Mac 电脑)
-- ============================================================================
CREATE TABLE IF NOT EXISTS kn_device (
    id            BIGINT PRIMARY KEY AUTO_INCREMENT COMMENT '自增主键',
    user_id       BIGINT NOT NULL COMMENT '所属用户 ID',
    device_name   VARCHAR(200) DEFAULT NULL COMMENT '用户自定义设备名（如 办公室 Mac Studio）',
    hostname      VARCHAR(255) DEFAULT NULL COMMENT '系统 hostname',
    os_version    VARCHAR(100) DEFAULT NULL COMMENT 'macOS 版本',
    agent_version VARCHAR(50) DEFAULT NULL COMMENT 'kn-agent 版本号',
    machine_id    VARCHAR(255) NOT NULL COMMENT '设备指纹: IOPlatformUUID（NVRAM 唯一硬件 ID）',
    device_token  VARCHAR(512) DEFAULT NULL COMMENT 'Agent 长期凭证（云端签发，0600 存本地）',
    status        VARCHAR(20) NOT NULL DEFAULT 'online' COMMENT '在线状态: online / offline / paused',
    last_seen     DATETIME DEFAULT NULL COMMENT '最后在线时间',
    unbound_at    DATETIME DEFAULT NULL COMMENT '上次解绑时间（24h 冷却期内禁止绑新设备）',
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '绑定时间',
    -- 索引
    INDEX idx_user_id (user_id),
    INDEX idx_machine_id (machine_id),
    UNIQUE INDEX idx_device_token (device_token)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='设备表';

-- ============================================================================
-- 3. 会话表
-- - id:         自增主键，仅 DB 内部 JOIN 使用，不暴露给客户端
-- - session_nid: WSS 协议层标识 (s_ + 12位 nanoid)，全局唯一，不可猜测
-- ============================================================================
CREATE TABLE IF NOT EXISTS kn_session (
    id          BIGINT PRIMARY KEY AUTO_INCREMENT COMMENT '自增主键（内部用，不暴露）',
    session_nid VARCHAR(20) NOT NULL COMMENT 'WSS 协议层 ID: s_ + 12位 nanoid（全局唯一）',
    user_id     BIGINT NOT NULL COMMENT '所属用户 ID',
    device_id   BIGINT NOT NULL COMMENT '所属设备 ID',
    tool        VARCHAR(20) NOT NULL COMMENT 'AI CLI 工具: claude / codex / qoder',
    profile     VARCHAR(100) DEFAULT NULL COMMENT '使用的 kn profile 名称',
    cwd         VARCHAR(500) DEFAULT NULL COMMENT '工作目录',
    source      VARCHAR(10) NOT NULL DEFAULT 'local' COMMENT '发起来源: ios / local / desktop',
    status      VARCHAR(20) NOT NULL DEFAULT 'running' COMMENT '会话状态: running / completed / failed / cancelled',
    started_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '开始时间',
    ended_at    DATETIME DEFAULT NULL COMMENT '结束时间',
    -- 索引
    UNIQUE INDEX idx_session_nid (session_nid),
    INDEX idx_user_id (user_id),
    INDEX idx_device_id (device_id),
    INDEX idx_status (status),
    -- 约束
    CONSTRAINT chk_session_source CHECK (source IN ('ios', 'local', 'desktop')),
    CONSTRAINT chk_session_status CHECK (status IN ('running', 'completed', 'failed', 'cancelled'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='会话表';

-- ============================================================================
-- 4. 消息表 (用户输入 + 系统事件，不含 PTY 原始输出)
-- ============================================================================
CREATE TABLE IF NOT EXISTS kn_message (
    id         BIGINT PRIMARY KEY AUTO_INCREMENT COMMENT '自增主键',
    session_id BIGINT NOT NULL COMMENT '所属会话 ID',
    seq        BIGINT NOT NULL COMMENT '会话内单调递增序号',
    direction  VARCHAR(10) NOT NULL COMMENT '消息方向: inbound (用户→AI) / system (系统事件)',
    msg_type   VARCHAR(30) NOT NULL COMMENT '消息类型: input / ctrl / system',
    src        VARCHAR(10) NOT NULL DEFAULT 'local' COMMENT '输入来源: ios / local / desktop',
    content    TEXT DEFAULT NULL COMMENT '用户输入文本 / 系统事件描述',
    created_at DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间（毫秒精度）',
    -- 索引
    INDEX idx_session_seq (session_id, seq),
    INDEX idx_session_time (session_id, created_at),
    -- 约束
    CONSTRAINT chk_direction CHECK (direction IN ('inbound', 'system')),
    CONSTRAINT chk_msg_type CHECK (msg_type IN ('input', 'ctrl', 'system'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='消息表（用户输入 + 系统事件）';

-- ============================================================================
-- 5. 卡密表 (AES-256-GCM 加密，kn 服务端自生成，第三方平台销售)
-- ============================================================================
CREATE TABLE IF NOT EXISTS kn_redeem_code (
    id              BIGINT PRIMARY KEY AUTO_INCREMENT COMMENT '自增主键',
    code            VARCHAR(64) NOT NULL COMMENT '卡密: KN-{AES-256-GCM 密文 hex}（约 48 字符）',
    plan            VARCHAR(20) NOT NULL COMMENT '激活方案: pro_monthly / pro_yearly',
    duration_days   INT NOT NULL COMMENT '有效天数',
    platform_source VARCHAR(50) DEFAULT NULL COMMENT '发售平台（淘宝/微信小店/卡密平台等）',
    redeem_source   VARCHAR(20) DEFAULT NULL COMMENT '兑换来源: desktop / ios',
    used_by         BIGINT DEFAULT NULL COMMENT '兑换用户 user_id',
    used_at         DATETIME DEFAULT NULL COMMENT '兑换时间',
    created_at      DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '生成时间',
    -- 索引
    UNIQUE INDEX idx_code (code),
    INDEX idx_used_by (used_by)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='卡密表';

-- ============================================================================
-- 6. APNs 推送 Token 表 (每台 iOS 设备一条记录)
-- ============================================================================
CREATE TABLE IF NOT EXISTS kn_push_token (
    id           BIGINT PRIMARY KEY AUTO_INCREMENT COMMENT '自增主键',
    user_id      BIGINT NOT NULL COMMENT '所属用户 ID',
    device_token VARCHAR(256) NOT NULL COMMENT 'Apple APNs 颁发的推送 token（16 进制字符串）',
    is_active    BOOLEAN NOT NULL DEFAULT TRUE COMMENT '是否有效（APNs 返回 410 时标记 false）',
    updated_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
    created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT '注册时间',
    -- 索引
    UNIQUE INDEX idx_token (device_token),
    INDEX idx_user_id (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='APNs 推送 Token 表';
```

- [ ] **Step 5: 执行初始化**

```bash
mysql -u root -p < kn-cloud/deploy/init.sql
mysql -u root -p kn_cloud -e "SHOW TABLES;"
# 预期: kn_user, kn_device, kn_session, kn_message, kn_redeem_code, kn_push_token
```

- [ ] **Step 6: Commit**

```bash
git add kn-cloud/deploy/init.sql
git commit -m "feat: database init SQL — all 5 tables for kn-cloud"
```

```bash
git add -A && git commit -m "feat: add database entities, mappers, and configuration"
```

---

### Task 2.5: 统一响应基础设施 — ApiResponse + 错误码 + 全局异常处理

**目标**：所有 Controller 返回 `ApiResponse<T>` 而非 `Map<String, Object>`，业务错误通过 `BizException` 统一处理。

**Files:**
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/dto/ApiResponse.java`
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/exception/ErrorCode.java`
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/exception/BizException.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/config/GlobalExceptionHandler.java`

- [ ] **Step 1: ApiResponse**

```java
package dev.kn.cloud.common.dto;

import lombok.AllArgsConstructor;
import lombok.Data;
import lombok.NoArgsConstructor;

@Data
@NoArgsConstructor
@AllArgsConstructor
public class ApiResponse<T> {
    private int code;
    private String message;
    private T data;

    public static <T> ApiResponse<T> ok(T data) {
        return new ApiResponse<>(0, "ok", data);
    }

    public static <T> ApiResponse<T> ok() {
        return new ApiResponse<>(0, "ok", null);
    }

    public static <T> ApiResponse<T> fail(int code, String message) {
        return new ApiResponse<>(code, message, null);
    }
}
```

- [ ] **Step 2: ErrorCode 枚举**

```java
package dev.kn.cloud.common.exception;

public enum ErrorCode {
    // 通用
    SUCCESS(0, "ok"),
    BAD_REQUEST(400, "请求参数错误"),
    UNAUTHORIZED(401, "未登录或 token 已过期"),
    FORBIDDEN(403, "权限不足"),
    RATE_LIMITED(429, "请求过于频繁"),
    NOT_FOUND(404, "资源不存在"),
    INTERNAL_ERROR(500, "服务器内部错误"),

    // 用户模块 (1xxx)
    EMAIL_EXISTS(1001, "邮箱已注册"),
    INVALID_CREDENTIALS(1002, "邮箱或密码错误"),
    ACCOUNT_DISABLED(1003, "账号已被禁用"),
    TOKEN_INVALID(1004, "refresh_token 无效或已过期"),

    // 设备模块 (2xxx)
    CODE_EXPIRED(2001, "绑定码已过期或不存在"),
    DEVICE_LIMIT_REACHED(2002, "设备数已达上限，请升级会员或解绑旧设备"),
    DEVICE_NOT_FOUND(2003, "设备不存在或不属于当前用户"),
    UNBIND_COOLDOWN(2004, "解绑冷却期内，无法绑定新设备"),
    MACHINE_ID_MISMATCH(2005, "设备指纹不匹配，请重新绑定"),

    // 会员模块 (3xxx)
    MEMBERSHIP_EXPIRED(3001, "会员已过期"),
    CODE_NOT_FOUND(3002, "卡密不存在"),
    CODE_ALREADY_USED(3003, "卡密已被使用"),
    INVALID_CODE_FORMAT(3004, "卡密格式无效"),

    // Session 模块 (4xxx)
    SESSION_NOT_FOUND(4001, "会话不存在"),
    SESSION_ALREADY_ENDED(4002, "会话已结束"),
    SESSION_LOCKED(4003, "会话已被锁定，其他客户端正在操作");

    public final int code;
    public final String defaultMessage;

    ErrorCode(int code, String defaultMessage) {
        this.code = code;
        this.defaultMessage = defaultMessage;
    }
}
```

- [ ] **Step 3: BizException**

```java
package dev.kn.cloud.common.exception;

import lombok.Getter;

@Getter
public class BizException extends RuntimeException {
    private final ErrorCode errorCode;

    public BizException(ErrorCode errorCode) {
        super(errorCode.defaultMessage);
        this.errorCode = errorCode;
    }

    public BizException(ErrorCode errorCode, String detail) {
        super(detail);
        this.errorCode = errorCode;
    }
}
```

- [ ] **Step 4: GlobalExceptionHandler**

```java
package dev.kn.cloud.api.config;

import dev.kn.cloud.common.dto.ApiResponse;
import dev.kn.cloud.common.exception.BizException;
import lombok.extern.slf4j.Slf4j;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.MethodArgumentNotValidException;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.RestControllerAdvice;

import java.util.stream.Collectors;

@Slf4j
@RestControllerAdvice
public class GlobalExceptionHandler {

    @ExceptionHandler(BizException.class)
    public ResponseEntity<ApiResponse<Void>> handleBiz(BizException e) {
        int httpStatus = switch (e.getErrorCode()) {
            case UNAUTHORIZED -> 401;
            case FORBIDDEN -> 403;
            case RATE_LIMITED -> 429;
            case BAD_REQUEST -> 400;
            case NOT_FOUND -> 404;
            default -> 200;  // 业务错误仍返回 200，通过 code 区分
        };
        return ResponseEntity.status(httpStatus)
            .body(ApiResponse.fail(e.getErrorCode().code,
                e.getMessage() != null ? e.getMessage() : e.getErrorCode().defaultMessage));
    }

    @ExceptionHandler(MethodArgumentNotValidException.class)
    public ResponseEntity<ApiResponse<Void>> handleValidation(MethodArgumentNotValidException e) {
        String msg = e.getBindingResult().getFieldErrors().stream()
            .map(f -> f.getField() + ": " + f.getDefaultMessage())
            .collect(Collectors.joining("; "));
        return ResponseEntity.badRequest().body(ApiResponse.fail(400, msg));
    }

    @ExceptionHandler(Exception.class)
    public ResponseEntity<ApiResponse<Void>> handleUnknown(Exception e) {
        log.error("未捕获异常", e);
        return ResponseEntity.status(500).body(ApiResponse.fail(500, "服务器内部错误"));
    }
}
```

- [ ] **Step 5: Commit**

```bash
git add kn-cloud-common/src/main/java/dev/kn/cloud/common/dto/
git add kn-cloud-common/src/main/java/dev/kn/cloud/common/exception/
git add kn-cloud-api/src/main/java/dev/kn/cloud/api/config/GlobalExceptionHandler.java
git commit -m "feat: unified ApiResponse + ErrorCode + BizException + global exception handler"
```

---

### Task 3: 用户模块 — 注册/登录/JWT

**Files:**
- Create: `kn-cloud-common/src/main/java/dev/kn/cloud/common/service/JwtService.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/auth/AuthController.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/auth/AuthService.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/config/AuthFilter.java`

- [ ] **Step 1: JWT Service**

```java
package dev.kn.cloud.common.service;

import io.jsonwebtoken.*;
import io.jsonwebtoken.security.Keys;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Service;
import javax.crypto.SecretKey;
import java.nio.charset.StandardCharsets;
import java.util.Date;

@Service
public class JwtService {
    private final SecretKey key;
    private final long accessExpireMs = 15 * 60 * 1000;  // 15 分钟

    public JwtService(@Value("${kn.jwt.secret}") String secret) {
        this.key = Keys.hmacShaKeyFor(secret.getBytes(StandardCharsets.UTF_8));
    }

    public String generateAccessToken(Long userId) {
        return Jwts.builder()
            .subject(String.valueOf(userId))
            .issuedAt(new Date())
            .expiration(new Date(System.currentTimeMillis() + accessExpireMs))
            .signWith(key)
            .compact();
    }

    public String generateRefreshToken(Long userId) {
        return Jwts.builder()
            .subject(String.valueOf(userId))
            .issuedAt(new Date())
            .expiration(new Date(System.currentTimeMillis() + 30L * 24 * 3600 * 1000)) // 30 天
            .signWith(key)
            .compact();
    }

    public Claims parse(String token) {
        return Jwts.parser().verifyWith(key).build()
            .parseSignedClaims(token).getPayload();
    }

    public boolean validate(String token) {
        try { parse(token); return true; }
        catch (JwtException e) { return false; }
    }
}
```

- [ ] **Step 2: AuthController** (注册/登录/刷新)

```java
package dev.kn.cloud.api.auth;

import dev.kn.cloud.common.dto.ApiResponse;
import dev.kn.cloud.common.entity.KnUser;
import dev.kn.cloud.common.exception.BizException;
import dev.kn.cloud.common.exception.ErrorCode;
import dev.kn.cloud.common.mapper.KnUserMapper;
import dev.kn.cloud.common.service.JwtService;
import com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper;
import jakarta.validation.Valid;
import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;
import org.springframework.security.crypto.bcrypt.BCryptPasswordEncoder;
import org.springframework.web.bind.annotation.*;
import java.time.LocalDate;

@RestController
@RequestMapping("/api/v1/auth")
public class AuthController {
    private final KnUserMapper userMapper;
    private final JwtService jwtService;
    private final BCryptPasswordEncoder encoder = new BCryptPasswordEncoder();

    public AuthController(KnUserMapper userMapper, JwtService jwtService) {
        this.userMapper = userMapper;
        this.jwtService = jwtService;
    }

    @PostMapping("/register")
    public ApiResponse<AuthResult> register(@Valid @RequestBody RegisterRequest req) {
        var exist = userMapper.selectOne(
            new LambdaQueryWrapper<KnUser>().eq(KnUser::getEmail, req.email()));
        if (exist != null) throw new BizException(ErrorCode.EMAIL_EXISTS);

        KnUser user = new KnUser();
        user.setEmail(req.email());
        user.setPassword(encoder.encode(req.password()));
        user.setMembership("trial");
        user.setTrialExpiresAt(LocalDate.now().plusDays(30));
        user.setStatus("active");
        userMapper.insert(user);

        String access = jwtService.generateAccessToken(user.getId());
        String refresh = jwtService.generateRefreshToken(user.getId());
        return ApiResponse.ok(new AuthResult(access, refresh, user.getId()));
    }

    @PostMapping("/login")
    public ApiResponse<AuthResult> login(@Valid @RequestBody LoginRequest req) {
        var user = userMapper.selectOne(
            new LambdaQueryWrapper<KnUser>().eq(KnUser::getEmail, req.email()));
        if (user == null || !encoder.matches(req.password(), user.getPassword()))
            throw new BizException(ErrorCode.INVALID_CREDENTIALS);
        if (!"active".equals(user.getStatus()))
            throw new BizException(ErrorCode.ACCOUNT_DISABLED);

        String access = jwtService.generateAccessToken(user.getId());
        String refresh = jwtService.generateRefreshToken(user.getId());
        return ApiResponse.ok(new AuthResult(access, refresh, user.getId()));
    }

    @PostMapping("/refresh")
    public ApiResponse<RefreshResult> refresh(@Valid @RequestBody RefreshRequest req) {
        if (!jwtService.validate(req.refreshToken()))
            throw new BizException(ErrorCode.TOKEN_INVALID);
        Long userId = Long.parseLong(jwtService.parse(req.refreshToken()).getSubject());
        return ApiResponse.ok(new RefreshResult(jwtService.generateAccessToken(userId)));
    }

    // ── DTOs ──

    public record RegisterRequest(
        @NotBlank @Email String email,
        @NotBlank @Size(min = 6) String password
    ) {}

    public record LoginRequest(
        @NotBlank String email,
        @NotBlank String password
    ) {}

    public record RefreshRequest(
        @NotBlank String refreshToken
    ) {}

    public record AuthResult(String accessToken, String refreshToken, Long userId) {}
    public record RefreshResult(String accessToken) {}
}
```

- [ ] **Step 3: AuthFilter** (按设计文档 §3.1.1 实现，使用 ErrorCode 统一 401/403 响应)

```java
package dev.kn.cloud.api.config;

import dev.kn.cloud.common.service.JwtService;
import jakarta.servlet.*;
import jakarta.servlet.http.*;
import org.springframework.stereotype.Component;
import org.springframework.web.filter.OncePerRequestFilter;
import java.io.IOException;
import java.util.List;

@Component
public class AuthFilter extends OncePerRequestFilter {
    private static final List<String> PUBLIC_PATHS = List.of(
        "/api/v1/auth/register", "/api/v1/auth/login",
        "/api/v1/auth/refresh", "/api/v1/device/bind-init"
    );
    private final JwtService jwtService;
    private final ObjectMapper mapper = new ObjectMapper();

    public AuthFilter(JwtService jwtService) { this.jwtService = jwtService; }

    @Override
    protected void doFilterInternal(HttpServletRequest request,
                                    HttpServletResponse response,
                                    FilterChain chain) throws IOException, ServletException {
        String path = request.getRequestURI();
        if (PUBLIC_PATHS.stream().anyMatch(path::startsWith)) {
            chain.doFilter(request, response);
            return;
        }
        String auth = request.getHeader("Authorization");
        if (auth == null || !auth.startsWith("Bearer ")) {
            writeError(response, 401, ErrorCode.UNAUTHORIZED);
            return;
        }
        String token = auth.substring(7);
        if (!jwtService.validate(token)) {
            writeError(response, 401, ErrorCode.UNAUTHORIZED);
            return;
        }
        request.setAttribute("userId", jwtService.parse(token).getSubject());
        chain.doFilter(request, response);
    }

    private void writeError(HttpServletResponse response, int httpStatus, ErrorCode code) throws IOException {
        response.setStatus(httpStatus);
        response.setContentType("application/json;charset=UTF-8");
        mapper.writeValue(response.getWriter(), ApiResponse.fail(code.code, code.defaultMessage));
    }
}
```

- [ ] **Step 4: 启动服务 + curl 测试**

```bash
cd kn-cloud && mvn -pl kn-cloud-api spring-boot:run
# 另开终端:
curl -X POST http://localhost:8080/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"test@kn.dev","password":"test123"}'
# 预期: {"access_token":"...", "refresh_token":"...", "user_id":1}
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: user registration, login, JWT, auth filter"
```

---

### Task 4: 设备绑定模块

**Files:**
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/device/DeviceController.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/device/DeviceService.java`

- [ ] **Step 1: DeviceController**

```java
package dev.kn.cloud.api.device;

import dev.kn.cloud.common.dto.ApiResponse;
import dev.kn.cloud.common.entity.KnDevice;
import dev.kn.cloud.common.exception.BizException;
import dev.kn.cloud.common.exception.ErrorCode;
import dev.kn.cloud.common.mapper.KnDeviceMapper;
import dev.kn.cloud.common.mapper.KnUserMapper;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.transaction.annotation.Transactional;
import org.springframework.web.bind.annotation.*;
import java.security.SecureRandom;
import java.time.Duration;
import java.time.LocalDateTime;
import java.util.List;
import java.util.UUID;

@RestController
@RequestMapping("/api/v1/device")
public class DeviceController {
    private final KnDeviceMapper deviceMapper;
    private final KnUserMapper userMapper;
    private final StringRedisTemplate redis;
    private final SecureRandom random = new SecureRandom();

    @org.springframework.beans.factory.annotation.Value("${kn.base-url}")
    private String baseUrl;  // dev: http://localhost:8080, prod: https://api.knshark.com

    public DeviceController(KnDeviceMapper dm, KnUserMapper um, StringRedisTemplate r) {
        this.deviceMapper = dm; this.userMapper = um; this.redis = r;
    }

    // POST /api/v1/device/bind-init (公开 + Nginx 限流)
    @PostMapping("/bind-init")
    public ApiResponse<BindInitResult> bindInit(@RequestBody BindInitRequest body) {
        String code = String.format("%06d", random.nextInt(1_000_000));
        redis.opsForValue().set("bind:code:" + code, body.machineId(), Duration.ofMinutes(5));
        String bindUrl = baseUrl + "/api/v1/device/bind?code=" + code;
        return ApiResponse.ok(new BindInitResult(code, 300, bindUrl));
    }

    // POST /api/v1/device/bind-confirm (需 JWT)
    // 注意：整个方法需在 @Transactional 中运行，
    // SELECT ... FOR UPDATE 锁住该用户所有 device 行，防止并发绑定时超出设备数上限
    @PostMapping("/bind-confirm")
    @Transactional
    public ApiResponse<BindResult> bindConfirm(@RequestBody BindConfirmRequest body,
                                               @RequestAttribute("userId") String userId) {
        // 原子"读+删"，防止两个 iOS 同时用同一 code 确认 → 重复绑定
        String machineId = redis.opsForValue().getAndDelete("bind:code:" + body.code());
        if (machineId == null) throw new BizException(ErrorCode.CODE_EXPIRED);

        Long uid = Long.parseLong(userId);
        var user = userMapper.selectById(uid);
        if (user == null) throw new BizException(ErrorCode.UNAUTHORIZED);

        // 悲观锁：SELECT ... FOR UPDATE 锁住该用户的所有 device 行，
        // 并发 bind-confirm 在此串行化，第二个请求等锁释放后读到已更新的 count
        long deviceCount = deviceMapper.selectCount(
            new com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper<KnDevice>()
                .eq(KnDevice::getUserId, uid)
                .last("FOR UPDATE"));  // MyBatis Plus: .last() 追加 SQL 片段
        int limit = switch (user.getMembership()) {
            case "pro" -> 3; case "enterprise" -> 10; default -> 1;
        };
        if (deviceCount >= limit)
            throw new BizException(ErrorCode.DEVICE_LIMIT_REACHED,
                "limit=" + limit + ", current=" + deviceCount);

        String deviceToken = UUID.randomUUID().toString().replace("-", "") +
                             UUID.randomUUID().toString().replace("-", "");

        KnDevice device = new KnDevice();
        device.setUserId(uid);
        device.setMachineId(machineId);
        device.setDeviceToken(deviceToken);
        device.setStatus("online");
        device.setLastSeen(LocalDateTime.now());
        deviceMapper.insert(device);

        // getAndDelete 已删除 bind_code，无需再次 delete

        // 跨进程通知 WS 进程：向等待中的 Agent 发送 bind_result
        redis.convertAndSend("ws:bind-result",
            String.format("{\"code\":\"%s\",\"device_token\":\"%s\"}", body.code(), deviceToken));

        return ApiResponse.ok(new BindResult(deviceToken, device.getId()));
    }

    // GET /api/v1/device/list (需 JWT)。返回 DeviceInfo 列表，含 online 字段
    @GetMapping("/list")
    public ApiResponse<List<DeviceInfo>> listDevices(@RequestAttribute("userId") String userId) {
        var devices = deviceMapper.selectList(
            new com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper<KnDevice>()
                .eq(KnDevice::getUserId, Long.parseLong(userId)));
        // 批量查在线状态 → Redis multiGet device:online:{machineId}
        var onlineKeys = devices.stream().map(d -> d.getMachineId() != null ? d.getMachineId() : "").toList();
        var online = redis.<String, Object>opsForValue().multiGet(onlineKeys);
        var result = new ArrayList<DeviceInfo>();
        for (int i = 0; i < devices.size(); i++) {
            var d = devices.get(i);
            var info = new DeviceInfo(d);
            info.setOnline(online.get(i) != null);
            result.add(info);
        }
        return ApiResponse.ok(result);
    }

    // POST /api/v1/device/unbind (需 JWT)
    @PostMapping("/unbind")
    public ApiResponse<Void> unbind(@RequestBody UnbindRequest body,
                                    @RequestAttribute("userId") String userId) {
        var device = deviceMapper.selectById(body.deviceId());
        if (device == null || !device.getUserId().equals(Long.parseLong(userId)))
            throw new BizException(ErrorCode.DEVICE_NOT_FOUND);

        // 24h 冷却
        if (device.getUnboundAt() != null &&
            device.getUnboundAt().plusHours(24).isAfter(LocalDateTime.now())) {
            long remaining = java.time.Duration.between(
                LocalDateTime.now(), device.getUnboundAt().plusHours(24)).getSeconds();
            throw new BizException(ErrorCode.UNBIND_COOLDOWN,
                "剩余 " + remaining + " 秒");
        }
        device.setStatus("offline");
        device.setUnboundAt(LocalDateTime.now());
        device.setDeviceToken(null);
        deviceMapper.updateById(device);
        return ApiResponse.ok();
    }

    // ── DTOs ──
    public record BindInitRequest(String machineId) {}
    public record BindInitResult(String bindCode, int expiresIn, String bindUrl) {}
    public record BindConfirmRequest(String code) {}
    public record BindResult(String deviceToken, Long deviceId) {}
    public record UnbindRequest(Long deviceId) {}
}
```

- [ ] **Step 2: Redis 设备状态 key 管理**

在线状态由 kn-cloud-ws 模块管理，key 使用 machineId（硬件指纹）；
kn-cloud-api 通过 Redis 批量查询（multiGet）在线状态并在设备列表 API 中返回：

```java
// DeviceService.isOnline — 通过 machineId 查询
public boolean isOnline(String machineId) {
    return machineId != null && !machineId.isBlank()
        && Boolean.TRUE.equals(redis.hasKey("device:online:" + machineId));
}
```

- [ ] **Step 3: 测试绑定流程**

```bash
# 1. Agent 请求 bind-init
curl -X POST http://localhost:8080/api/v1/device/bind-init \
  -H 'Content-Type: application/json' \
  -d '{"machine_id":"ABC123-DEF456"}'
# 预期: {"bind_code":"482916","expires_in":300,"bind_url":"http://localhost:8080/api/v1/device/bind?code=482916"}

# 2. iOS 端确认绑定（用 JWT）
curl -X POST http://localhost:8080/api/v1/device/bind-confirm \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer <access_token>' \
  -d '{"code":"482916"}'
# 预期: {"device_token":"...","device_id":1}
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: device bind-init, bind-confirm, list, unbind with cooldown"
```

---

### Task 4.5: SessionController + DeviceController profiles + RedeemController

**设计文档端点补全**（§3.1.1 中定义的 HTTP 端点，Phase 1 完成）：

**Files:**
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/session/SessionController.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/redeem/RedeemController.java`
- Modify: `kn-cloud-api/src/main/java/dev/kn/cloud/api/device/DeviceController.java` (追加 profiles 端点)

- [ ] **Step 1: SessionController**

```java
package dev.kn.cloud.api.session;

import dev.kn.cloud.common.dto.ApiResponse;
import dev.kn.cloud.common.entity.KnSession;
import dev.kn.cloud.common.entity.KnMessage;
import dev.kn.cloud.common.mapper.KnSessionMapper;
import dev.kn.cloud.common.mapper.KnMessageMapper;
import com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper;
import org.springframework.web.bind.annotation.*;
import java.util.List;

@RestController
@RequestMapping("/api/v1/session")
public class SessionController {
    private final KnSessionMapper sessionMapper;
    private final KnMessageMapper messageMapper;

    public SessionController(KnSessionMapper sm, KnMessageMapper mm) {
        this.sessionMapper = sm; this.messageMapper = mm;
    }

    /** GET /api/v1/session/list — 用户会话历史（需 JWT） */
    @GetMapping("/list")
    public ApiResponse<List<KnSession>> listSessions(@RequestAttribute("userId") String userId) {
        var sessions = sessionMapper.selectList(
            new LambdaQueryWrapper<KnSession>()
                .eq(KnSession::getUserId, Long.parseLong(userId))
                .orderByDesc(KnSession::getStartedAt));
        return ApiResponse.ok(sessions);
    }

    /** GET /api/v1/session/{id}/messages — 会话消息历史（需 JWT） */
    @GetMapping("/{id}/messages")
    public ApiResponse<List<KnMessage>> getMessages(@PathVariable Long id,
                                                     @RequestAttribute("userId") String userId) {
        var session = sessionMapper.selectById(id);
        if (session == null || !session.getUserId().equals(Long.parseLong(userId)))
            return ApiResponse.fail(404, "会话不存在");
        var messages = messageMapper.selectList(
            new LambdaQueryWrapper<KnMessage>()
                .eq(KnMessage::getSessionId, id)
                .orderByAsc(KnMessage::getSeq));
        return ApiResponse.ok(messages);
    }
}
```

- [ ] **Step 2: DeviceController 追加 profiles 端点**

在 `DeviceController.java` 中追加：

```java
/** GET /api/v1/device/{id}/profiles — iOS 获取设备可用 profile 列表（需 JWT，不含 API Key） */
@GetMapping("/{id}/profiles")
public ApiResponse<List<ProfileInfo>> getDeviceProfiles(@PathVariable Long id,
                                                         @RequestAttribute("userId") String userId) {
    var device = deviceMapper.selectById(id);
    if (device == null || !device.getUserId().equals(Long.parseLong(userId)))
        throw new BizException(ErrorCode.DEVICE_NOT_FOUND);
    // profiles 数据从 Agent 的 profile_list WSS 消息获取，缓存在 Redis
    // Redis key: device:profiles:{device_id} → JSON list
    String cached = redis.opsForValue().get("device:profiles:" + id);
    if (cached == null) return ApiResponse.ok(List.of());  // Agent 尚未上报
    try {
        List<ProfileInfo> profiles = new ObjectMapper()
            .readValue(cached, new TypeReference<List<ProfileInfo>>() {});
        return ApiResponse.ok(profiles);
    } catch (Exception e) {
        return ApiResponse.ok(List.of());
    }
}

// DTO
public record ProfileInfo(String name, String tool, String desc) {}
```

- [ ] **Step 3: RedeemController（HTTP 端点，iOS 路径 B）**

```java
package dev.kn.cloud.api.redeem;

import dev.kn.cloud.common.dto.ApiResponse;
import dev.kn.cloud.common.exception.BizException;
import dev.kn.cloud.common.exception.ErrorCode;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/v1/user")
public class RedeemController {
    private final RedeemService redeemService;

    public RedeemController(RedeemService rs) { this.redeemService = rs; }

    /** POST /api/v1/user/redeem — iOS 端输入卡密激活（需 JWT） */
    @PostMapping("/redeem")
    public ApiResponse<RedeemResult> redeem(@RequestBody RedeemRequest body,
                                            @RequestAttribute("userId") String userId) {
        var result = redeemService.redeem(body.code(), Long.parseLong(userId), "ios");
        return ApiResponse.ok(new RedeemResult(result.plan(), result.days()));
    }

    public record RedeemRequest(String code) {}
    public record RedeemResult(String plan, int days) {}
}
```

注意：`RedeemService` 在 Cloud Phase 2 Task 11 中实现。Phase 1 中可先放空实现，Phase 2 完成后 Controller 自动生效。

- [ ] **Step 4: Commit**

```bash
git add kn-cloud-api/src/main/java/dev/kn/cloud/api/session/
git add kn-cloud-api/src/main/java/dev/kn/cloud/api/redeem/
git add kn-cloud-api/src/main/java/dev/kn/cloud/api/device/DeviceController.java
git commit -m "feat: SessionController, DeviceController/profiles, RedeemController"
```

---

### Task 5: WebSocket 服务 — 连接管理 + 消息中继

**Files:**
- - Create: `kn-cloud-ws/src/main/java/dev/kn/cloud/ws/WsServerConfig.java`
- Create: `kn-cloud-ws/src/main/java/dev/kn/cloud/ws/WsMessageRelay.java`
- Create: `kn-cloud-ws/src/main/resources/application.yml` (端口 8081)
- Create: `kn-cloud-ws/src/main/java/dev/kn/cloud/ws/WsConnectionManager.java`
- Create: `kn-cloud-ws/src/main/java/dev/kn/cloud/ws/WsMessageRelay.java`

- [ ] **Step 1: WebSocket 端点**

```java
package dev.kn.cloud.ws;

import dev.kn.cloud.common.entity.KnDevice;
import dev.kn.cloud.common.entity.KnSession;
import dev.kn.cloud.common.mapper.KnDeviceMapper;
import dev.kn.cloud.common.mapper.KnSessionMapper;
import com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import org.springframework.web.socket.*;
import org.springframework.web.socket.handler.TextWebSocketHandler;
import java.net.URI;
import java.time.Duration;
import java.util.concurrent.ConcurrentHashMap;

@Component
public class KnWsHandler extends TextWebSocketHandler {
    private final KnDeviceMapper deviceMapper;
    private final KnSessionMapper sessionMapper;
    private final StringRedisTemplate redis;
    private final JwtService jwtService;       // iOS 连接鉴权
    private final MessageService messageService;  // 消息持久化
    private final String wsNodeId;  // 本实例唯一 ID，如 "ws-" + UUID
    // device_id → session
    private final ConcurrentHashMap<Long, WebSocketSession> agentSessions = new ConcurrentHashMap<>();
    // user_id → session (iOS)
    private final ConcurrentHashMap<Long, WebSocketSession> userSessions = new ConcurrentHashMap<>();
    // bind_code → session (临时绑定连接，等待 bind_result)
    private final ConcurrentHashMap<String, WebSocketSession> bindingSessions = new ConcurrentHashMap<>();

    // 限流: per session (WSS sessionId → 计数器)
    private final ConcurrentHashMap<String, AtomicInteger> inputRates = new ConcurrentHashMap<>();
    private final ConcurrentHashMap<String, AtomicInteger> ctrlRates = new ConcurrentHashMap<>();
    private final ConcurrentHashMap<String, AtomicInteger> startSessionRates = new ConcurrentHashMap<>();

    // 应用层心跳超时检测: agent:{machineId} 或 user:{userId} → 最后收到 ping 的时间戳
    private final ConcurrentHashMap<String, Long> lastPongTimes = new ConcurrentHashMap<>();
    private static final long PONG_TIMEOUT_MS = 90_000;  // 90s 超时

    // 消息大小上限
    private static final int MAX_MESSAGE_SIZE_BYTES = 1_048_576;  // 1MB

    public KnWsHandler(KnDeviceMapper dm, KnSessionMapper sm, StringRedisTemplate r,
                       JwtService js, MessageService ms) {
        this.deviceMapper = dm; this.sessionMapper = sm; this.redis = r;
        this.jwtService = js; this.messageService = ms;
        this.wsNodeId = "ws-" + UUID.randomUUID().toString().substring(0, 8);

        // 每秒清零 input/ctrl 计数器
        Executors.newSingleThreadScheduledExecutor().scheduleAtFixedRate(() -> {
            inputRates.clear();
            ctrlRates.clear();
        }, 1, 1, TimeUnit.SECONDS);
        // 每分钟清零 start_session 计数器
        Executors.newSingleThreadScheduledExecutor().scheduleAtFixedRate(() -> {
            startSessionRates.clear();
        }, 1, 1, TimeUnit.MINUTES);

        // 启动 Redis Pub/Sub 订阅，监听发给本实例的中继消息
        new Thread(() -> {
            redis.getConnectionFactory().getConnection().subscribe((message, pattern) -> {
                String payload = new String(message.getBody());
                byte[] channel = message.getChannel();
                String channelName = new String(channel);

                if (("ws:relay:" + wsNodeId).equals(channelName)) {
                    // 消息中继 → 本地投递
                    try {
                        var relay = new ObjectMapper().readTree(payload);
                        long targetId = relay.get("target_id").asLong();
                        boolean isDevice = "device".equals(relay.get("target_type").asText());
                        WebSocketSession target = isDevice
                            ? agentSessions.get(targetId)
                            : userSessions.get(targetId);
                        if (target != null && target.isOpen()) {
                            send(target, relay.get("payload").toString());
                        }
                    } catch (Exception ignored) {}
                } else if ("ws:control".equals(channelName)) {
                    // 跨进程控制指令（API → WS）
                    try {
                        var ctrl = new ObjectMapper().readTree(payload);
                        String action = ctrl.get("action").asText();
                        if ("kick_device".equals(action)) {
                            long deviceId = ctrl.get("device_id").asLong();
                            kickDevice(deviceId);
                        }
                    } catch (Exception ignored) {}
                } else if ("ws:bind-result".equals(channelName)) {
                    // 绑定结果通知（API → WS）：DeviceController bind-confirm 完成后发布
                    try {
                        var result = new ObjectMapper().readTree(payload);
                        String code = result.get("code").asText();
                        String deviceToken = result.get("device_token").asText();
                        WebSocketSession bindSession = bindingSessions.remove(code);
                        if (bindSession != null && bindSession.isOpen()) {
                            sendMessage(bindSession, "bind_result", data -> {
                                data.put("device_token", deviceToken);
                            });
                        }
                    } catch (Exception ignored) {}
                }
            }, ("ws:relay:" + wsNodeId).getBytes(), "ws:control".getBytes(), "ws:bind-result".getBytes());
        }).start();

        // 心跳超时检测：每 30s 扫描一次，超过 90s 无 ping 的 session 主动 close
        new Thread(() -> {
            while (true) {
                try { Thread.sleep(30_000); } catch (InterruptedException e) { break; }
                long now = System.currentTimeMillis();
                for (var entry : lastPongTimes.entrySet()) {
                    if (now - entry.getValue() > PONG_TIMEOUT_MS) {
                        // 僵尸连接：清理
                        String wsSessionId = entry.getKey();
                        agentSessions.values().removeIf(s -> s.getId().equals(wsSessionId));
                        userSessions.values().removeIf(s -> s.getId().equals(wsSessionId));
                        lastPongTimes.remove(wsSessionId);
                        // 清理对应的 device:conn
                        // (略，遍历 agentSessions 找到对应 device_id)
                    }
                }
            }
        }).start();
    }

    @Override
    public void afterConnectionEstablished(WebSocketSession session) {
        var headers = session.getHandshakeHeaders();
        String auth = headers.getFirst("Authorization");
        String machineId = headers.getFirst("X-KN-Machine-Id");
        String role = headers.getFirst("X-KN-Role");

        Map<String, String> params = new HashMap<>();
        if (auth != null && auth.startsWith("Bearer ")) {
            String token = auth.substring(7);
            if ("ios".equals(role)) {
                params.put("access_token", token);
                handleUserConnect(session, params);
            } else if (machineId != null) {
                // 用 token 查 kn_device: device_token 长的 = 正式, 6位数字 = 临时 code
                var device = deviceMapper.selectOne(
                    new LambdaQueryWrapper<KnDevice>().eq(KnDevice::getDeviceToken, token));
                if (device != null) {
                    params.put("device_token", token);
                    params.put("machine_id", machineId);
                    handleAgentConnect(session, params);
                } else {
                    params.put("code", token);
                    params.put("machine_id", machineId);
                    handleBindingConnect(session, params);
                }
            }
        } else {
            close(session, 4003);
        }
    }

    private void handleAgentConnect(WebSocketSession session, Map<String, String> params) {
        String deviceToken = params.get("device_token");
        String machineId = params.get("machine_id");

        var device = deviceMapper.selectOne(
            new LambdaQueryWrapper<KnDevice>().eq(KnDevice::getDeviceToken, deviceToken));
        if (device == null) { close(session, 4003); return; }
        if (!device.getMachineId().equals(machineId)) { close(session, 4003); return; }

        // 原子替换旧连接（put 返回旧值，避免 remove+put 之间的并发窗口）
        WebSocketSession old = agentSessions.put(device.getId(), session);
        if (old != null && old.isOpen()) { try { old.close(); } catch (Exception ignored) {} }
        // 记录 Agent 所在网关实例，用于跨实例消息路由
        redis.opsForValue().set("ws:device:" + device.getId(), wsNodeId, Duration.ofSeconds(90));
        redis.opsForValue().set("device:online:" + device.getId(), "1", Duration.ofSeconds(60));
        sendMessage(session, "connected", data -> {
            data.put("ws_session_id", session.getId());
            data.put("protocol_version", 1);
        });
    }

    private void handleUserConnect(WebSocketSession session, Map<String, String> params) {
        String token = params.get("access_token");
        if (token == null || !jwtService.validate(token)) {
            close(session, 4001);  // 4001 = Unauthorized
            return;
        }
        Long userId = Long.parseLong(jwtService.parse(token).getSubject());
        params.put("user_id", String.valueOf(userId));  // 回填供后续使用

        // 原子替换旧连接
        WebSocketSession old = userSessions.put(userId, session);
        if (old != null && old.isOpen()) { try { old.close(); } catch (Exception ignored) {} }
        redis.opsForValue().set("ws:user:" + userId, wsNodeId, Duration.ofSeconds(90));

        sendMessage(session, "connected", data -> {
            data.put("ws_session_id", session.getId());
            data.put("protocol_version", 1);
        });
    }

    private void handleBindingConnect(WebSocketSession session, Map<String, String> params) {
        String code = params.get("code");
        String machineId = params.get("machine_id");
        if (code == null || machineId == null) { close(session, 4003); return; }

        // 验证 bind_code 存在且匹配 machine_id
        String stored = redis.opsForValue().get("bind:code:" + code);
        if (stored == null || !stored.equals(machineId)) { close(session, 4003); return; }

        // 临时连接：注册到 bindingSessions，等待 bind_result
        bindingSessions.put(code, session);
        sendMessage(session, "connected", data -> {
            data.put("ws_session_id", session.getId());
            data.put("mode", "binding");
        });
    }

    @Override
    protected void handleTextMessage(WebSocketSession session, TextMessage message) {
        String payload = message.getPayload();
        // 消息大小上限 1MB，防止 OOM
        if (payload.length() > MAX_MESSAGE_SIZE_BYTES) {
            close(session, 1009);  // 1009 = Message Too Big
            return;
        }
        try {
            var msg = new com.fasterxml.jackson.databind.ObjectMapper().readTree(payload);
            String type = msg.has("type") ? msg.get("type").asText() : null;
            String sessionId = msg.has("session_id") ? msg.get("session_id").asText() : null;

            if (type == null) {
                sendError(session, "invalid_message", "缺少字段: type");
                return;
            }

            // 消息去重：seq-based，5min 窗口
            if (msg.has("seq") && sessionId != null) {
                String dedupKey = "msg:dedup:" + sessionId + ":" + msg.get("seq").asText();
                if (Boolean.TRUE.equals(redis.hasKey(dedupKey))) {
                    return;  // 重复消息，静默丢弃
                }
                redis.opsForValue().set(dedupKey, "1", Duration.ofMinutes(5));
            }

            if ("ping".equals(type)) {
                lastPongTimes.put(session.getId(), System.currentTimeMillis());
                sendSimple(session, "pong", "ts", String.valueOf(System.currentTimeMillis()));
                return;
            }

            if ("agent_info".equals(type)) {
                // Agent 上报版本/环境信息 → 更新 kn_device
                Long deviceId = findDeviceByWsSession(session);
                if (deviceId != null) {
                    String agentVer = msg.has("agent_version") ? msg.get("agent_version").asText() : null;
                    String osVer = msg.has("os_version") ? msg.get("os_version").asText() : null;
                    String hostname = msg.has("hostname") ? msg.get("hostname").asText() : null;
                    var device = deviceMapper.selectById(deviceId);
                    if (device != null) {
                        if (agentVer != null) device.setAgentVersion(agentVer);
                        if (osVer != null) device.setOsVersion(osVer);
                        if (hostname != null) device.setHostname(hostname);
                        device.setLastSeen(LocalDateTime.now());
                        deviceMapper.updateById(device);
                    }
                }
                return;
            }

            if ("profile_list".equals(type)) {
                // Agent 上报 profile 列表 → 缓存到 Redis 供 iOS 查询
                Long deviceId = findDeviceByWsSession(session);
                if (deviceId != null && msg.has("profiles")) {
                    redis.opsForValue().set("device:profiles:" + deviceId,
                        msg.get("profiles").toString(), Duration.ofHours(1));
                }
                return;
            }

            if ("redeem".equals(type) && msg.has("code")) {
                // Desktop 端卡密激活：Agent WSS 中转到云端
                // 通过 device_token 查出 user_id → 调 RedeemService
                Long deviceId = findDeviceByWsSession(session);
                if (deviceId != null) {
                    var device = deviceMapper.selectById(deviceId);
                    if (device != null) {
                        try {
                            var result = redeemService.redeem(
                                msg.get("code").asText(), device.getUserId(), "desktop");
                            sendMessage(session, "redeem_result", data -> {
                                data.put("ok", true);
                                data.put("plan", result.plan());
                                data.put("message", "激活成功: " + result.plan() + " " + result.days() + "天");
                            });
                        } catch (BizException e) {
                            sendMessage(session, "redeem_result", data -> {
                                data.put("ok", false);
                                data.put("message", e.getMessage());
                            });
                        }
                    }
                }
                return;
            }

            // 限流检查
            String sid = session.getId();
            if ("start_session".equals(type) && incrAndCheck(startSessionRates, sid, 10)) {
                sendError(session, "rate_limited", "操作频率过高，请稍后重试");
                return;
            }
            if ("input".equals(type) && incrAndCheck(inputRates, sid, 20)) {
                // 丢弃但发 ack 带 dropped:true
                long seq = msg.has("seq") ? msg.get("seq").asLong() : 0;
                sendMessage(session, "ack", data -> {
                    data.put("msg_seq", seq);
                    data.put("dropped", true);
                });
                return;
            }
            if ("ctrl".equals(type) && incrAndCheck(ctrlRates, sid, 5)) {
                long seq = msg.has("seq") ? msg.get("seq").asLong() : 0;
                sendMessage(session, "ack", data -> {
                    data.put("msg_seq", seq);
                    data.put("dropped", true);
                });
                return;
            }

            // 消息路由: iOS → Agent（通过 session_nid 查找）
            if (sessionId != null) {
                var sessionRow = sessionMapper.selectOne(
                    new LambdaQueryWrapper<KnSession>().eq(KnSession::getSessionNid, sessionId));
                if (sessionRow == null) {
                    sendError(session, "session_not_found", "会话不存在: " + sessionId);
                    return;
                }
                if (!"running".equals(sessionRow.getStatus())) {
                    sendError(session, "session_already_ended", "会话已结束: " + sessionId);
                    return;
                }
                if (sessionRow != null) {
                    long deviceId = sessionRow.getDeviceId();

                    // ── 消息持久化 + session:state 更新 ──
                    if ("input".equals(type)) {
                        messageService.recordInput(sessionRow.getId(), msg.get("seq").asLong(),
                            msg.get("text").asText());
                    } else if ("session_created".equals(type)) {
                        messageService.recordSystem(sessionRow.getId(), 0, "system",
                            "Session created");
                        updateSessionState(sessionId, "running", "created",
                            msg.has("seq") ? msg.get("seq").asLong() : 0);
                    } else if ("session_ended".equals(type)) {
                        messageService.recordSystem(sessionRow.getId(), 0, "system",
                            "Session ended: " + msg.get("reason").asText());
                        updateSessionState(sessionId, msg.get("status").asText(), "ended",
                            msg.has("seq") ? msg.get("seq").asLong() : 0);
                    }

                    WebSocketSession agent = agentSessions.get(deviceId);

                    if (agent != null && agent.isOpen()) {
                        // 同实例：直接投递
                        send(agent, payload);
                    } else {
                        // 跨实例：查 Redis 找到目标 ws_node，Pub/Sub 中继
                        String targetWsNode = redis.opsForValue().get("ws:device:" + deviceId);
                        if (targetWsNode != null && !targetWsNode.equals(wsNodeId)) {
                            var relay = new ObjectMapper().createObjectNode();
                            relay.put("target_type", "device");
                            relay.put("target_id", deviceId);
                            relay.put("payload", new ObjectMapper().readTree(payload));
                            redis.convertAndSend("ws:relay:" + targetWsNode, relay.toString());
                        } else if (targetWsNode == null) {
                            // Agent 离线（所有 ws_node 上都没有连接）→ 缓存到 pending 队列
                            String pendingKey = "pending:agent:" + deviceId;
                            redis.opsForList().rightPush(pendingKey, payload);
                            redis.expire(pendingKey, Duration.ofDays(7));  // 7 天无连接过期
                            // 如果是 input 类型，返回 ack 告知发送方已暂存
                            if ("input".equals(type) && msg.has("seq")) {
                                sendMessage(session, "ack", data -> {
                                    data.put("msg_seq", msg.get("seq").asLong());
                                    data.put("pending", true);
                                });
                            }
                        }
                    }
                }
            }
        } catch (com.fasterxml.jackson.core.JsonParseException e) {
            // 非 JSON 数据 → close(1003)
            close(session, 1003);
        } catch (Exception e) {
            sendError(session, "parse_error", "消息解析失败: " + e.getMessage());
        }
    }

    @Override
    protected void handleBinaryMessage(WebSocketSession session, BinaryMessage message) {
        close(session, 1003);  // 不接受二进制消息
    }

    @Override
    public void afterConnectionClosed(WebSocketSession session, CloseStatus status) {
        // 清理 Agent 连接对应的 Redis 键
        for (var entry : agentSessions.entrySet()) {
            if (entry.getValue().equals(session)) {
                redis.delete("device:online:" + entry.getKey());
                redis.delete("device:conn:" + entry.getKey());
                redis.delete("ws:device:" + entry.getKey());
                break;
            }
        }
        // 清理 iOS 连接对应的 Redis 键
        for (var entry : userSessions.entrySet()) {
            if (entry.getValue().equals(session)) {
                redis.delete("ws:user:" + entry.getKey());
                break;
            }
        }
        agentSessions.values().removeIf(s -> s.equals(session));
        userSessions.values().removeIf(s -> s.equals(session));
        bindingSessions.values().removeIf(s -> s.equals(session));
    }

    // ── Jackson 消息构造（替代手动拼 JSON 字符串）──
    private final ObjectMapper mapper = new ObjectMapper();

    private void send(WebSocketSession s, String json) {
        try { s.sendMessage(new TextMessage(json)); } catch (Exception ignored) {}
    }

    private void sendMessage(WebSocketSession s, String type, java.util.function.Consumer<ObjectNode> dataBuilder) {
        var root = mapper.createObjectNode();
        root.put("type", type);
        if (dataBuilder != null) {
            var data = mapper.createObjectNode();
            dataBuilder.accept(data);
            root.set("data", data);
        }
        try { s.sendMessage(new TextMessage(root.toString())); } catch (Exception ignored) {}
    }

    private void sendSimple(WebSocketSession s, String type, String key, String value) {
        var root = mapper.createObjectNode();
        root.put("type", type);
        root.put(key, value);
        try { s.sendMessage(new TextMessage(root.toString())); } catch (Exception ignored) {}
    }

    private void sendError(WebSocketSession s, String code, String detail) {
        sendMessage(s, "error_notify", data -> {
            data.put("code", code);
            data.put("message", detail);
        });
    }

    /** 外部（MembershipScheduler 通过 Redis Pub/Sub）调用的踢设备方法 */
    public void kickDevice(Long deviceId) {
        WebSocketSession ws = agentSessions.remove(deviceId);
        if (ws != null && ws.isOpen()) {
            sendError(ws, "kicked", "会员已到期");
            try { ws.close(new CloseStatus(4000)); } catch (Exception ignored) {}
        }
        redis.delete("device:online:" + deviceId);
        redis.delete("device:conn:" + deviceId);
    }

    private void close(WebSocketSession s, int code) {
        try { s.close(new CloseStatus(code)); } catch (Exception ignored) {}
    }

    /// 计数器 +1，超过 limit 返回 true
    private boolean incrAndCheck(ConcurrentHashMap<String, AtomicInteger> map, String key, int limit) {
        AtomicInteger counter = map.computeIfAbsent(key, k -> new AtomicInteger(0));
        return counter.incrementAndGet() > limit;
    }

    // ── session:state Redis 维护 ──

    /** session 生命周期事件（创建/结束/中断）→ 写入 session:state */
    private void updateSessionState(String sessionId, String status, String lastEvent, long lastSeq) {
        String key = "session:state:" + sessionId;
        var data = new HashMap<String, String>();
        data.put("status", status);
        data.put("last_event", lastEvent);
        data.put("last_seq", String.valueOf(lastSeq));
        redis.opsForHash().putAll(key, data);
        redis.expire(key, Duration.ofDays(7));  // session 结束后保留 7 天
    }

    /**
     * 在以下时机调用：
     *   - start_session 消息 → Redis pending 写入后，调用 updateSessionState(sid, "pending", "start_requested", 0)
     *   - session_created 上报 → INSERT MySQL 后，调用 updateSessionState(sid, "running", "created", seq)
     *   - session_ended 上报   → UPDATE MySQL 后，调用 updateSessionState(sid, status, "ended", seq)
     */

}
```

- [ ] **Step 2: WebSocket 配置**

```java
package dev.kn.cloud.ws;

import org.springframework.context.annotation.Configuration;
import org.springframework.web.socket.config.annotation.*;

@Configuration
@EnableWebSocket
public class WsConfig implements WebSocketConfigurer {
    private final KnWsHandler handler;
    public WsConfig(KnWsHandler h) { this.handler = h; }

    @Override
    public void registerWebSocketHandlers(WebSocketHandlerRegistry registry) {
        registry.addHandler(handler, "/ws").setAllowedOrigins("*");
    }
}
```

- [ ] **Step 3: 启动 ws 服务 + 测试**

`kn-cloud-ws/src/main/resources/application.yml`:
```yaml
server:
  port: 8081   # WebSocket 独立端口
spring:
  data:
    redis:
      host: localhost
      port: 6379
```

```bash
cd kn-cloud && mvn -pl kn-cloud-ws spring-boot:run
# 用 wscat 测试: wscat -c ws://localhost:8081/ws -H 'Authorization: Bearer <device_token>' -H 'X-KN-Machine-Id: xxx'
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: WebSocket handler - agent/user connection, message relay"
```

---

---

### Task 6: Nginx 反向代理 + Redis 限流 + 并发检测

**Files:**
- Create: `kn-cloud/deploy/nginx.conf`
- Modify: `kn-cloud-api` — 新增 RateLimitFilter
- Modify: `kn-cloud-ws` — WsHandler 补并发检测

- [ ] **Step 0: Let's Encrypt 证书 + 自动续期**

```bash
# 1. 安装 certbot (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install -y certbot python3-certbot-nginx

# 2. 首次获取证书（HTTP-01 challenge，需域名已解析到本机 + 80 端口可达）
sudo certbot certonly --nginx -d api.knshark.com --non-interactive --agree-tos -m admin@knshark.com

# 3. certbot 自动续期（安装时已自动创建 systemd timer）
#    验证: systemctl status certbot.timer
#    certbot 每天随机时间检查一次，到期前 30 天内自动续签
#    续签后自动 reload nginx: /etc/letsencrypt/renewal-hooks/deploy/nginx-reload.sh
```

**certbot 续期原理**：
- systemd timer `certbot.timer` 每天触发一次
- `certbot renew` 检查所有证书，到期 <30 天的自动续签
- 续签成功 → 执行 `/etc/letsencrypt/renewal-hooks/deploy/` 下的 hook → reload nginx
- 零运维，Let's Encrypt 完全免费

**Nginx 需增加 HTTP (80) server block 用于 certbot challenge**：

```nginx
# HTTP → HTTPS 重定向 + certbot ACME challenge
server {
    listen 80;
    server_name api.knshark.com;
    
    # certbot HTTP-01 challenge
    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }
    
    # 其他请求重定向到 HTTPS
    location / {
        return 301 https://$host$request_uri;
    }
}
```

```bash
sudo mkdir -p /var/www/certbot
```

- [ ] **Step 1: Nginx 配置**

创建 `kn-cloud/deploy/nginx.conf`：

```nginx
# HTTP → HTTPS 重定向 + certbot ACME challenge
server {
    listen 80;
    server_name api.knshark.com;

    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }

    location / {
        return 301 https://$host$request_uri;
    }
}

upstream kn_api {
    server 127.0.0.1:8080;
}

upstream kn_ws {
    server 127.0.0.1:8081;
}

# 全局限流 — /bind-init 单 IP 3次/5min
limit_req_zone $binary_remote_addr zone=bind_limit:10m rate=3r/m;

server {
    listen 443 ssl http2;
    server_name api.knshark.com;

    ssl_certificate     /etc/letsencrypt/live/api.knshark.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.knshark.com/privkey.pem;

    # HTTP API
    location /api/v1/ {
        proxy_pass http://kn_api;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;

        # /bind-init 额外限流
        location /api/v1/device/bind-init {
            limit_req zone=bind_limit burst=2 nodelay;
            proxy_pass http://kn_api;
        }
    }

    # WebSocket
    location /ws {
        proxy_pass http://kn_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 60s;
    }
}
```

- [ ] **Step 2: Redis 登录限流**

在 `kn-cloud-api` 中新增 `LoginRateLimiter`：

```java
package dev.kn.cloud.api.auth;

import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import java.time.Duration;

@Component
public class LoginRateLimiter {
    private final StringRedisTemplate redis;

    public LoginRateLimiter(StringRedisTemplate redis) { this.redis = redis; }

    public boolean isLocked(String email) {
        return Boolean.TRUE.equals(redis.hasKey("login:locked:" + email));
    }

    public void recordAttempt(String emailOrIp) {
        String key = "login:rate:" + emailOrIp;
        Long count = redis.opsForValue().increment(key);
        if (count == 1) redis.expire(key, Duration.ofMinutes(15));
        if (count != null && count >= 5) {
            redis.opsForValue().set("login:locked:" + emailOrIp, "1", Duration.ofMinutes(15));
        }
    }

    public void clearAttempts(String emailOrIp) {
        redis.delete("login:rate:" + emailOrIp);
        redis.delete("login:locked:" + emailOrIp);
    }
}
```

在 `AuthController.login()` 中：登录前检查 `isLocked()`，失败时调 `recordAttempt()`，成功时 `clearAttempts()`。

- [ ] **Step 3: WSS 并发连接检测**

在 `KnWsHandler.handleAgentConnect()` 中：

```java
private void handleAgentConnect(WebSocketSession session, Map<String, String> params) {
    // ... 现有 device_token 验证 ...

    // 并发连接检测
    String connKey = "device:conn:" + device.getId();
    String existingConnId = redis.opsForValue().get(connKey);

    if (existingConnId != null && !existingConnId.equals(session.getId())) {
        // 旧连接还在 → 踢掉
        WebSocketSession old = agentSessions.get(device.getId());
        if (old != null && old.isOpen()) {
            sendError(old, "kicked", "检测到新的设备连接，旧连接被踢");
            try { old.close(); } catch (Exception ignored) {}
        }

        // IP 不同 → 告警
        String oldIp = redis.opsForValue().get("device:ip:" + device.getId());
        String newIp = getClientIp(session);
        if (oldIp != null && !oldIp.equals(newIp)) {
            redis.opsForValue().set("device:anomaly:" + device.getId(),
                "ip_mismatch", Duration.ofDays(7));
            // TODO: 发邮件/iOS 推送通知用户
        }
    }

    // 写入新连接
    agentSessions.put(device.getId(), session);
    redis.opsForValue().set(connKey, session.getId(), Duration.ofSeconds(60));
    redis.opsForValue().set("device:online:" + device.getId(), "1", Duration.ofSeconds(60));
    redis.opsForValue().set("device:ip:" + device.getId(), getClientIp(session));

    sendMessage(session, "connected", data -> {
        data.put("ws_session_id", session.getId());
        data.put("protocol_version", 1);
    });
}

private String getClientIp(WebSocketSession session) {
    if (session.getRemoteAddress() != null) {
        return session.getRemoteAddress().getAddress().getHostAddress();
    }
    return "unknown";
}
```

- [ ] **Step 4: 消息持久化 (kn_message INSERT)**

新增 `MessageService`：

```java
package dev.kn.cloud.common.service;

import dev.kn.cloud.common.entity.KnMessage;
import dev.kn.cloud.common.mapper.KnMessageMapper;
import org.springframework.stereotype.Service;

@Service
public class MessageService {
    private final KnMessageMapper messageMapper;

    public MessageService(KnMessageMapper mapper) { this.messageMapper = mapper; }

    public void recordInput(Long sessionId, long seq, String content) {
        KnMessage msg = new KnMessage();
        msg.setSessionId(sessionId);
        msg.setSeq(seq);
        msg.setDirection("inbound");
        msg.setMsgType("input");
        msg.setContent(content);
        messageMapper.insert(msg);
    }

    public void recordSystem(Long sessionId, long seq, String msgType, String content) {
        KnMessage msg = new KnMessage();
        msg.setSessionId(sessionId);
        msg.setSeq(seq);
        msg.setDirection("system");
        msg.setMsgType(msgType);
        msg.setContent(content);
        messageMapper.insert(msg);
    }
}
```

在 WsHandler 的消息路由中，收到 `input` → `messageService.recordInput()`；`session_created` / `session_ended` → `messageService.recordSystem()`。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: nginx config, login rate limit, concurrent detection, message persistence"
```

---

### Task 6.5: 单元测试 + 集成测试

- [ ] **Step 1: 单元测试（JUnit 5 + Mockito）**

创建 `kn-cloud-api/src/test/`：

```java
// AuthControllerTest.java
@WebMvcTest(AuthController.class)
class AuthControllerTest {
    @MockBean KnUserMapper userMapper;
    @MockBean JwtService jwtService;
    @Autowired MockMvc mvc;

    @Test
    void register_shouldReturnToken() throws Exception {
        when(userMapper.selectOne(any())).thenReturn(null);
        // ...
        mvc.perform(post("/api/v1/auth/register")
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"email\":\"t@kn.dev\",\"password\":\"x\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.access_token").exists());
    }
}

// DeviceControllerTest.java — 测试 bind-init / bind-confirm / 设备数上限
// AuthFilterTest.java — 测试公开路径放行 + JWT 鉴权 + 401/403
```

- [ ] **Step 2: 集成测试（Testcontainers MySQL + Redis）**

创建 `kn-cloud-api/src/test/java/dev/kn/cloud/api/`:

```java
@SpringBootTest(webEnvironment = SpringBootTest.WebEnvironment.RANDOM_PORT)
@Testcontainers
class CloudApiIntegrationTest {
    @Container
    static MySQLContainer<?> mysql = new MySQLContainer<>("mysql:8.0");
    @Container
    static GenericContainer<?> redis = new GenericContainer<>("redis:7-alpine")
        .withExposedPorts(6379);

    @DynamicPropertySource
    static void props(DynamicPropertyRegistry r) {
        r.add("spring.datasource.url", mysql::getJdbcUrl);
        r.add("spring.datasource.username", mysql::getUsername);
        r.add("spring.datasource.password", mysql::getPassword);
        r.add("spring.data.redis.host", redis::getHost);
        r.add("spring.data.redis.port", () -> redis.getMappedPort(6379));
    }

    @Test
    void fullBindFlow() {
        // 1. 注册 → 2. 登录 → 3. bind-init → 4. bind-confirm → 5. 查设备列表
    }
}
```

`kn-cloud-api/pom.xml` 添加：
```xml
<dependency>
    <groupId>org.testcontainers</groupId>
    <artifactId>testcontainers</artifactId>
    <version>1.19.8</version>
    <scope>test</scope>
</dependency>
<dependency>
    <groupId>org.testcontainers</groupId>
    <artifactId>mysql</artifactId>
    <version>1.19.8</version>
    <scope>test</scope>
</dependency>
```

- [ ] **Step 3: CI 运行测试**

更新 `.github/workflows/deploy-cloud.yml`:
```yaml
- name: Test
  working-directory: kn-cloud
  run: mvn test  # 不加 -DskipTests
```

- [ ] **Step 4: Commit**

```bash
git add kn-cloud/kn-cloud-api/src/test/
git commit -m "test: add unit tests (JUnit 5) and integration tests (Testcontainers)"
```

---

### Task 7: CI/CD + systemd 部署

**Files:**
- Create: `.github/workflows/deploy-cloud.yml`
- Create: `kn-cloud/deploy/kn-cloud-api.service`
- Create: `kn-cloud/deploy/kn-cloud-ws.service`

- [ ] **Step 1: GitHub Actions workflow**

创建 `.github/workflows/deploy-cloud.yml`:

```yaml
name: Deploy kn-cloud

on:
  push:
    branches: [main]
    paths:
      - 'kn-cloud/**'

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Set up JDK 21
        uses: actions/setup-java@v4
        with:
          java-version: '21'
          distribution: 'temurin'

      - name: Test + Build
        working-directory: kn-cloud
        run: mvn clean package  # 不加 -DskipTests，先跑测试再打包

      - name: Copy jars to server
        uses: appleboy/scp-action@v0
        with:
          host: ${{ secrets.SERVER_HOST }}
          username: ${{ secrets.SERVER_USER }}
          key: ${{ secrets.SSH_PRIVATE_KEY }}
          source: "kn-cloud/kn-cloud-api/target/*.jar,kn-cloud/kn-cloud-ws/target/*.jar"
          target: "/opt/kn-cloud/"
          strip_components: 3

      - name: Restart services
        uses: appleboy/ssh-action@v1
        with:
          host: ${{ secrets.SERVER_HOST }}
          username: ${{ secrets.SERVER_USER }}
          key: ${{ secrets.SSH_PRIVATE_KEY }}
          script: |
            # 初始化（首次部署时用）
            if [ ! -f /etc/systemd/system/kn-cloud-api.service ]; then
              sudo cp /opt/kn-cloud/deploy/kn-cloud-api.service /etc/systemd/system/
              sudo cp /opt/kn-cloud/deploy/kn-cloud-ws.service /etc/systemd/system/
              sudo systemctl daemon-reload
              sudo systemctl enable kn-cloud-api kn-cloud-ws
            fi
            sudo systemctl restart kn-cloud-api kn-cloud-ws
            echo "Deploy done. Status:"
            sudo systemctl status kn-cloud-api kn-cloud-ws --no-pager
```

**GitHub Secrets 需配置**: `SERVER_HOST`, `SERVER_USER`, `SSH_PRIVATE_KEY`。

**生产环境变量文件**（`/opt/kn-cloud/kn-cloud.env`，权限 `0600`，`kn` 用户只读）：

```bash
# kn-cloud 生产环境变量 — 首次部署时手动创建
# 权限: sudo chmod 600 /opt/kn-cloud/kn-cloud.env && sudo chown kn:kn /opt/kn-cloud/kn-cloud.env

# 数据库
DB_HOST=127.0.0.1
DB_USER=kn_prod
DB_PASS=<生成强密码>

# Redis
REDIS_HOST=127.0.0.1
REDIS_PORT=6379
REDIS_PASS=<生成强密码>

# JWT (openssl rand -base64 32)
JWT_SECRET=<至少 256-bit 随机字符串>

# 卡密 AES 密钥 (openssl rand -base64 32)
REDEEM_AES_KEY=<Base64 编码的 256-bit key>

# APNs (从 Apple Developer Console 获取)
APNS_TEAM_ID=<10 位 Team ID>
APNS_KEY_ID=<10 位 Key ID>
APNS_KEY=<p8 文件内容，去掉换行符>
```

- [ ] **Step 2: systemd service 文件**

创建 `kn-cloud/deploy/kn-cloud-api.service`:

```ini
[Unit]
Description=kn Cloud API (HTTP)
After=network.target mysql.service redis.service

[Service]
User=kn
WorkingDirectory=/opt/kn-cloud
# prod profile: 生产配置在 JAR 内 application-prod.yml，敏感值从环境变量注入
ExecStart=/usr/bin/java -jar /opt/kn-cloud/kn-cloud-api.jar --spring.profiles.active=prod
# 环境变量从外部文件加载（0600 权限，含 DB 密码 / JWT secret 等）
EnvironmentFile=-/opt/kn-cloud/kn-cloud.env
Restart=on-failure
RestartSec=5
StandardOutput=append:/opt/kn-cloud/logs/api.log
StandardError=append:/opt/kn-cloud/logs/api-error.log

[Install]
WantedBy=multi-user.target
```

创建 `kn-cloud/deploy/kn-cloud-ws.service`:

```ini
[Unit]
Description=kn Cloud WebSocket
After=network.target mysql.service redis.service

[Service]
User=kn
WorkingDirectory=/opt/kn-cloud
ExecStart=/usr/bin/java -jar /opt/kn-cloud/kn-cloud-ws.jar --spring.profiles.active=prod
EnvironmentFile=-/opt/kn-cloud/kn-cloud.env
Restart=on-failure
RestartSec=5
StandardOutput=append:/opt/kn-cloud/logs/ws.log
StandardError=append:/opt/kn-cloud/logs/ws-error.log

[Install]
WantedBy=multi-user.target
```

- [ ] **Step 3: DB schema 迁移说明**

初期（v1）直接执行 SQL，无 Flyway/Liquibase:

```
kn-cloud/deploy/
├── init.sql          # 首次部署：建库 + 建表
├── migrate-*.sql     # 后续手动迁移脚本（按日期命名）
└── *.service         # systemd unit
```

每次 schema 变更记录到 `deploy/CHANGELOG.md`，包含 SQL 差异和执行方式。

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/deploy-cloud.yml kn-cloud/deploy/
git commit -m "feat: CI/CD with GitHub Actions → SSH → systemd + DB init SQL"
```

---

## Phase 1 完成检查点

Cloud Phase 1 完成后具备：
- [x] 用户注册/登录/JWT 签发
- [x] AuthFilter 鉴权
- [x] 设备 bind-init / bind-confirm / list / unbind
- [x] 解绑 24h 冷却期
- [x] WebSocket 连接管理（Agent + iOS 两种鉴权）
- [x] 消息中继骨架
- [x] 单元测试 (JUnit 5 + Mockito) + 集成测试 (Testcontainers)
- [x] GitHub Actions 自动构建部署到服务器 (JAR + systemd)
- [x] DB 初始化 SQL + schema 变更记录

**尚未实现（Phase 2）**：
- [ ] 会员等级与到期/缓冲期逻辑
- [ ] 卡密生成与兑换
- [ ] APNs 推送集成
- [ ] 定时任务（到期检测、session failed 标记）
- [ ] 离线消息缓存（Redis LIST）
