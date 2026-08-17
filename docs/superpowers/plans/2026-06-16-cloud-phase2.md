# Cloud Phase 2 — 会员系统 + APNs + 定时任务

> ⚠️ **权威版本在 `../kn-cloud/docs/2026-06-16-cloud-phase2.md`**。此文件为 kn monorepo 中的只读副本，修改请到 kn-cloud repo。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use `- [ ]` checkbox syntax.

**Goal:** 实现会员等级到期/缓冲期逻辑、APNs 推送集成、卡密生成、云端定时任务（到期检测、session failed 标记）。

**Prerequisites:** Cloud Phase 1 完成。

---

### Task 9: 会员等级 + 到期/缓冲期

**Files:**
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/membership/MembershipService.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/membership/MembershipScheduler.java`

- [ ] **Step 1: MembershipService — 到期检测 + 缓冲期**

```java
package dev.kn.cloud.api.membership;

import dev.kn.cloud.common.entity.KnUser;
import dev.kn.cloud.common.entity.KnSession;
import dev.kn.cloud.common.mapper.KnUserMapper;
import dev.kn.cloud.common.mapper.KnSessionMapper;
import com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper;
import com.baomidou.mybatisplus.core.conditions.update.LambdaUpdateWrapper;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Service;
import java.time.LocalDate;
import java.time.LocalDateTime;

@Service
public class MembershipService {
    private final KnUserMapper userMapper;
    private final KnSessionMapper sessionMapper;
    private final StringRedisTemplate redis;

    public MembershipService(KnUserMapper um, KnSessionMapper sm, StringRedisTemplate r) {
        this.userMapper = um; this.sessionMapper = sm; this.redis = r;
    }

    /** 检查是否可以创建新 session */
    public boolean canCreateSession(Long userId) {
        var user = userMapper.selectById(userId);
        if (user == null) return false;

        // 缓冲期内：禁止新建 session，但已有 session 继续
        if (isInGracePeriod(user)) return false;

        // 完全到期：也不行
        if (isExpired(user)) return false;

        return true;
    }

    // 缓冲期：24h（初期硬编码，后期迁到 membership-config.yml）
    private static final int GRACE_PERIOD_HOURS = 24;

    /** 缓冲期判断：到期日当天 + 24h 缓冲 */
    private boolean isInGracePeriod(KnUser user) {
        LocalDate expiry = getExpiryDate(user);
        if (expiry == null) return false;
        LocalDate today = LocalDate.now();
        // 缓冲期 24h = 1 天
        return !today.isBefore(expiry) && today.isBefore(expiry.plusDays(1));
    }

    /** 完全到期（缓冲期过后） */
    private boolean isExpired(KnUser user) {
        LocalDate expiry = getExpiryDate(user);
        if (expiry == null) return false;
        return !LocalDate.now().isBefore(expiry.plusDays(1));
    }

    private LocalDate getExpiryDate(KnUser user) {
        if ("trial".equals(user.getMembership()) && user.getTrialExpiresAt() != null) {
            return user.getTrialExpiresAt();
        }
        if (("pro".equals(user.getMembership()) || "enterprise".equals(user.getMembership()))
            && user.getMembershipExpiresAt() != null) {
            return user.getMembershipExpiresAt();
        }
        return null;
    }
}
```

- [ ] **Step 2: MembershipScheduler — 定时任务**

```java
package dev.kn.cloud.api.membership;

import dev.kn.cloud.common.entity.KnUser;
import dev.kn.cloud.common.entity.KnSession;
import dev.kn.cloud.common.entity.KnDevice;
import dev.kn.cloud.common.mapper.KnUserMapper;
import dev.kn.cloud.common.mapper.KnSessionMapper;
import dev.kn.cloud.common.mapper.KnDeviceMapper;
import com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper;
import com.baomidou.mybatisplus.core.conditions.update.LambdaUpdateWrapper;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.util.List;

@Component
public class MembershipScheduler {
    private final KnUserMapper userMapper;
    private final KnSessionMapper sessionMapper;
    private final KnDeviceMapper deviceMapper;
    private final KnMessageMapper messageMapper;  // Task 9.5 清理使用
    private final MembershipService membershipService;
    private final StringRedisTemplate redis;
    private final ObjectMapper mapper = new ObjectMapper();

    public MembershipScheduler(KnUserMapper um, KnSessionMapper sm, KnDeviceMapper dm,
                               KnMessageMapper mm, MembershipService ms, StringRedisTemplate r) {
        this.userMapper = um; this.sessionMapper = sm; this.deviceMapper = dm;
        this.messageMapper = mm; this.membershipService = ms; this.redis = r;
    }

    /** 每天 00:05 检查到期用户 */
    @Scheduled(cron = "0 5 0 * * ?")
    public void checkExpirations() {
        LocalDate today = LocalDate.now();
        LocalDate warnDate = today.plusDays(1); // 明天到期的用户

        // 1. 提前 1 天警告
        List<KnUser> warnUsers = userMapper.selectList(
            new LambdaQueryWrapper<KnUser>()
                .eq(KnUser::getStatus, "active")
                .and(w -> w.eq(KnUser::getTrialExpiresAt, warnDate)
                         .or().eq(KnUser::getMembershipExpiresAt, warnDate)));
        for (var user : warnUsers) {
            // TODO: APNs 推送 + App 内横幅 "您的试用/会员即将到期"
            System.out.println("WARN: user " + user.getId() + " expires tomorrow");
        }

        // 2. 缓冲期过后 → 标记 expired + 强制断开 WSS
        List<KnUser> expiredUsers = userMapper.selectList(
            new LambdaQueryWrapper<KnUser>()
                .eq(KnUser::getStatus, "active")
                .and(w -> w.lt(KnUser::getTrialExpiresAt, today.minusDays(1))
                         .or().lt(KnUser::getMembershipExpiresAt, today.minusDays(1))));
        for (var user : expiredUsers) {
            // 1. 先踢 WSS 连接（防止标记 expired 后仍能通过存量连接创建 session）
            var devices = deviceMapper.selectList(
                new LambdaQueryWrapper<KnDevice>().eq(KnDevice::getUserId, user.getId()));
            for (var device : devices) {
                var kickMsg = mapper.createObjectNode();
                kickMsg.put("action", "kick_device");
                kickMsg.put("device_id", device.getId());
                redis.convertAndSend("ws:control", kickMsg.toString());
                redis.delete("device:online:" + device.getId());
                redis.delete("device:conn:" + device.getId());
            }

            // 2. 标记 user 为 expired（WSS 已断，无法再创建新 session）
            user.setStatus("expired");
            userMapper.updateById(user);

            // 3. 标记该用户所有 running session 为 failed
            sessionMapper.update(null,
                new LambdaUpdateWrapper<KnSession>()
                    .eq(KnSession::getUserId, user.getId())
                    .eq(KnSession::getStatus, "running")
                    .set(KnSession::getStatus, "failed")
                    .set(KnSession::getEndedAt, LocalDateTime.now()));

            System.out.println("EXPIRED: user " + user.getId() + " — WSS 已强制断开");
        }
    }
}
```

- [ ] **Step 3: 在 WsHandler 中集成 canCreateSession 检查 + user.status 兜底**

在收到 `start_session` 消息时，双重检查：

```java
// 1. 业务层检查（缓冲期/到期逻辑）
if (!membershipService.canCreateSession(userId)) {
    sendError(session, "membership_expired", "用户会员已过期，无法创建新 session");
    return;
}

// 2. DB 层兜底检查：防止 Redis Pub/Sub 踢连接消息丢失后，
//    MembershipScheduler 已标记 expired 但 WSS 连接尚未断开
var user = userMapper.selectById(userId);
if (user == null || !"active".equals(user.getStatus())) {
    sendError(session, "membership_expired", "用户会员已过期，无法创建新 session");
    return;
}
```

**为什么需要双重检查**：`MembershipScheduler.checkExpirations()` 通过 Redis Pub/Sub 异步通知 WsHandler 踢连接。如果 Pub/Sub 消息丢失（网络抖动），WSS 连接不会断开。此时 `canCreateSession()` 在 API 侧能拦截，但 WsHandler 侧也需要兜底——直接查 `kn_user.status` 确认用户状态。

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: membership expiry, grace period, scheduled expiration check"
```

---

### Task 9.5: 消息保留清理

在 `MembershipScheduler` 同文件中新增定时任务：

```java
/** 每天凌晨 3:00 清理 90 天前的消息记录 */
@Scheduled(cron = "0 0 3 * * ?")
public void cleanupOldMessages() {
    LocalDateTime cutoff = LocalDateTime.now().minusDays(90);
    int deleted = messageMapper.delete(
        new LambdaQueryWrapper<KnMessage>()
            .lt(KnMessage::getCreatedAt, cutoff));
    if (deleted > 0) {
        log.info("清理过期消息: {} 条", deleted);
    }
}
```

说明：v1 硬删除，不需要 `deleted_at` 软删除。`kn_message.created_at` 已有索引，DELETE 走索引高效。等消息量到百万级再考虑分区表。

**同步清理 `kn_session` 旧记录**（同一任务中实现）：

```java
/** 每天凌晨 3:30 清理 180 天前已结束的 session 记录 */
@Scheduled(cron = "0 30 3 * * ?")
public void cleanupOldSessions() {
    LocalDateTime cutoff = LocalDateTime.now().minusDays(180);
    // 先删关联的 message（已在上面的 90 天清理中处理，此处兜底）
    // 再删 session
    int deleted = sessionMapper.delete(
        new LambdaQueryWrapper<KnSession>()
            .in(KnSession::getStatus, "completed", "failed", "cancelled")
            .lt(KnSession::getEndedAt, cutoff));
    if (deleted > 0) {
        log.info("清理过期 session: {} 条", deleted);
    }
}
```

说明：message 保留 90 天，session 保留 180 天（方便用户查看最近半年的会话历史）。session 删除时其关联 message 已在 90 天清理中删除（CASCADE 或先删 message 再删 session）。

- [ ] **Commit**

```bash
git commit -m "feat: message (90d) + session (180d) retention cleanup daily at 3am"
```

---

### Task 9.6: session:pending 超时清理 + 通知调用方

`session:pending:{session_id}` Redis key TTL 到期后自动删除，但调用方（iOS）不知道创建失败的原因。需要定时扫描过期的 pending session，通过 WSS 通知调用方。

在 `MembershipScheduler` 中新增：

```java
/** 每分钟清理过期的 session:pending，通知调用方 */
@Scheduled(fixedRate = 60_000)
public void cleanupStalePendingSessions() {
    // 扫描所有 session:pending:* key
    var keys = redis.keys("session:pending:*");
    for (String key : keys) {
        if (!redis.hasKey(key)) {
            continue; // 已过期，但 keys() 可能有延迟
        }
        var data = redis.opsForHash().entries(key);
        if (data.isEmpty()) {
            redis.delete(key);
            continue;
        }
        // 检查创建时间是否超过 120s（ack 后的最大 TTL）
        Long ts = data.containsKey("ts") ? Long.parseLong(data.get("ts").toString()) : 0;
        if (System.currentTimeMillis() - ts > 120_000) {
            // 超时未确认 → 回查 session:state 确认 session 是否已被创建
            // （Agent 可能刚好上线并在清理扫描前创建了 session，防止误报"超时"）
            String pendingSid = key.replace("session:pending:", "");
            boolean sessionCreated = redis.hasKey("session:state:" + pendingSid);
            if (sessionCreated) {
                // session 已创建，无需通知，仅清理 pending key
                redis.delete(key);
                continue;
            }
            // 确认未创建 → 通知调用方（如果 iOS 还在线）
            String userId = (String) data.get("user_id");
            if (userId != null && redis.hasKey("ws:user:" + userId)) {
                String targetNode = redis.opsForValue().get("ws:user:" + userId);
                if (targetNode != null) {
                    var notifyMsg = mapper.createObjectNode();
                    notifyMsg.put("type", "error_notify");
                    notifyMsg.put("code", "session_create_timeout");
                    notifyMsg.put("message", "会话创建超时，Agent 未响应");
                    redis.convertAndSend("ws:relay:" + targetNode, notifyMsg.toString());
                }
            }
            redis.delete(key);
        }
    }
}
```

- [ ] **Commit**

```bash
git commit -m "feat: stale session:pending cleanup with caller notification"
```

---

### Task 10: APNs 推送集成

**Files:**
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/push/ApnsService.java`
- Create: `kn-cloud-api/src/main/java/dev/kn/cloud/api/push/PushController.java`

- [ ] **Step 1: ApnsService — HTTP/2 APNs 推送**

```java
package dev.kn.cloud.api.push;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.stereotype.Service;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.security.KeyFactory;
import java.security.PrivateKey;
import java.security.spec.PKCS8EncodedKeySpec;
import java.util.Base64;
import java.util.Date;
import java.util.Map;

import io.jsonwebtoken.Jwts;
import lombok.extern.slf4j.Slf4j;

@Slf4j
@Service
public class ApnsService {
    private final String teamId;
    private final String keyId;
    private final String apnsKey;  // p8 文件内容
    private final boolean production;
    private final HttpClient client = HttpClient.newHttpClient();
    private final ObjectMapper mapper = new ObjectMapper();

    public ApnsService(@Value("${kn.apns.team-id}") String teamId,
                       @Value("${kn.apns.key-id}") String keyId,
                       @Value("${kn.apns.key}") String apnsKey,
                       @Value("${kn.apns.production:false}") boolean production) {
        this.teamId = teamId; this.keyId = keyId; this.apnsKey = apnsKey;
        this.production = production;
    }

    private String getBaseUrl() {
        return production
            ? "https://api.push.apple.com/3/device/"
            : "https://api.development.push.apple.com/3/device/";
    }

    /** 发推送通知 
     *  @param knType 推送类型: ai_complete / ai_confirm / agent_crash / trial_expiring */
    public void send(String deviceToken, String title, String body, String knType) throws Exception {
        String jwt = generateApnsJwt();
        var payload = mapper.writeValueAsString(Map.of(
            "aps", Map.of("alert", Map.of("title", title, "body", body),
                          "sound", "default"),
            "kn_type", knType   // iOS PushManager 据此判断推送类型
        ));

        var request = HttpRequest.newBuilder()
            .uri(URI.create(getBaseUrl() + deviceToken))
            .header("authorization", "bearer " + jwt)
            .header("apns-topic", "dev.kn.ios")
            .header("apns-push-type", "alert")
            .POST(HttpRequest.BodyPublishers.ofString(payload))
            .build();

        var response = client.send(request, HttpResponse.BodyHandlers.ofString());
        if (response.statusCode() == 403) {
            // p8 key 被 revoke → 记录告警，降级（推送不可用但不影响核心功能）
            log.error("APNs 403 Forbidden — p8 key 可能已被 revoke，请在 Apple Developer 更新 key");
            return;  // 降级而非抛异常
        }
        if (response.statusCode() != 200) {
            log.warn("APNs push failed: status={}, body={}", response.statusCode(), response.body());
        }
    }

    /**
     * 签发 APNs 认证 token (ES256 JWT)。
     * 使用 Java 17+ 内置 KeyFactory 解析 p8 私钥（EC SECP256R1），
     * 无需 BouncyCastle 额外依赖。
     *
     * 参考: Apple docs "Establishing a token-based connection to APNs"
     */
    private String generateApnsJwt() {
        try {
            // 1. 解析 p8 PEM 私钥 → EC PrivateKey
            String keyContent = apnsKey
                .replace("-----BEGIN PRIVATE KEY-----", "")
                .replace("-----END PRIVATE KEY-----", "")
                .replaceAll("\\s", "");
            byte[] keyBytes = Base64.getDecoder().decode(keyContent);
            PKCS8EncodedKeySpec spec = new PKCS8EncodedKeySpec(keyBytes);
            KeyFactory keyFactory = KeyFactory.getInstance("EC");
            PrivateKey privateKey = keyFactory.generatePrivate(spec);

            // 2. 签发 JWT (jjwt 自动检测 EC 密钥 → ES256 算法)
            long now = System.currentTimeMillis() / 1000;
            return Jwts.builder()
                .header()
                    .add("kid", keyId)
                    .and()
                .issuer(teamId)
                .issuedAt(new Date(now * 1000))
                .expiration(new Date((now + 3600) * 1000))  // APNs 要求 1 小时内过期
                .signWith(privateKey)
                .compact();
        } catch (Exception e) {
            log.error("APNs JWT 签名失败 — p8 key 可能无效或已过期", e);
            throw new RuntimeException("APNs JWT generation failed", e);
        }
    }
}
```

- [ ] **Step 1.5: KnPushToken Entity + Mapper**

在 `kn-cloud-common` 中新增（与 Task 2 的 Entity 并列）：

```java
// common/src/.../entity/KnPushToken.java
@Data
@TableName("kn_push_token")
public class KnPushToken {
    @TableId(type = IdType.AUTO)
    private Long id;
    private Long userId;
    private String deviceToken;
    private Boolean isActive;
    private LocalDateTime updatedAt;
    private LocalDateTime createdAt;
}

// common/src/.../mapper/KnPushTokenMapper.java
@Mapper
public interface KnPushTokenMapper extends BaseMapper<KnPushToken> {
    @Update("INSERT INTO kn_push_token (user_id, device_token, is_active) VALUES (#{userId}, #{deviceToken}, true) " +
            "ON DUPLICATE KEY UPDATE is_active=true, updated_at=NOW()")
    void upsert(@Param("userId") Long userId, @Param("deviceToken") String deviceToken);
}
```

- [ ] **Step 2: PushController — device token 注册**

```java
package dev.kn.cloud.api.push;

import dev.kn.cloud.common.dto.ApiResponse;
import dev.kn.cloud.common.mapper.KnPushTokenMapper;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/v1/push")
public class PushController {
    private final KnPushTokenMapper knPushTokenMapper;
    private final StringRedisTemplate redis;

    public PushController(KnPushTokenMapper m, StringRedisTemplate r) {
        this.knPushTokenMapper = m; this.redis = r;
    }

    @PostMapping("/register")
    public ApiResponse<Void> register(@RequestBody PushRegisterRequest body,
                                      @RequestAttribute("userId") String userId) {
        Long uid = Long.parseLong(userId);
        knPushTokenMapper.upsert(uid, body.deviceToken());
        redis.opsForSet().add("push:token:" + userId, body.deviceToken());
        return ApiResponse.ok();
    }

    public record PushRegisterRequest(String deviceToken) {}
}
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat: APNs push service and device token registration"
```

---

### Task 11: 卡密生成工具 + 验证服务

**Files:**
- Create: `kn-cloud/tools/GenerateCodes.java`（生成工具）
- Create: `kn-cloud-api/.../redeem/RedeemService.java`（验证服务）

**原则**：
- 卡密使用 **AES-256-GCM** 加密，自包含 plan + days + timestamp，无法伪造
- 生成工具不连数据库，输出 `.sql` 文件手动导入
- AES 密钥通过环境变量 `REDEEM_AES_KEY` 传入（Base64 编码的 256-bit key）

- [ ] **Step 1: 卡密批量生成（AES-256-GCM 加密）**

```java
package dev.kn.cloud.tools;

import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.SecretKeySpec;
import java.io.*;
import java.nio.file.*;
import java.security.SecureRandom;
import java.time.LocalDate;
import java.time.format.DateTimeFormatter;
import java.util.Base64;

/**
 * 批量生成卡密 INSERT SQL 文件（AES-256-GCM 加密），手动导入。
 *
 * 用法: REDEEM_AES_KEY=<base64key> java GenerateCodes.java <plan> <count> <duration_days> [platform_source]
 * 生成 AES-256 密钥: openssl rand -base64 32
 */
public class GenerateCodes {

    private static final int GCM_IV_LEN = 12;   // 96-bit IV
    private static final int GCM_TAG_LEN = 128; // 128-bit auth tag
    private static final SecureRandom RANDOM = new SecureRandom();

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("用法: REDEEM_AES_KEY=<key> java GenerateCodes.java <plan> <count> <days> [source]");
            System.err.println("  plan: pro_monthly | pro_yearly");
            System.err.println("  生成密钥: openssl rand -base64 32");
            System.exit(1);
        }

        String keyB64 = System.getenv("REDEEM_AES_KEY");
        if (keyB64 == null) { System.err.println("请设置 REDEEM_AES_KEY 环境变量"); System.exit(1); }
        SecretKeySpec key = new SecretKeySpec(Base64.getDecoder().decode(keyB64), "AES");

        String plan = args[0];
        int count = Integer.parseInt(args[1]);
        int days = Integer.parseInt(args[2]);
        String source = args.length >= 4 ? args[3] : "manual";

        String ts = LocalDate.now().format(DateTimeFormatter.ofPattern("yyyyMMdd"));
        Path outFile = Paths.get("redeem_codes_" + plan + "_" + count + "_" + ts + ".sql");

        try (var writer = Files.newBufferedWriter(outFile)) {
            writer.write("-- kn 卡密批量导入 (AES-256-GCM 加密)\n");
            writer.write("-- 生成时间: " + java.time.LocalDateTime.now() + "\n");
            writer.write("-- plan: " + plan + ", count: " + count + ", days: " + days + "\n");
            writer.write("-- 用法: mysql -u root -p kn_cloud < " + outFile.getFileName() + "\n\n");

            for (int i = 0; i < count; i++) {
                // 明文: plan|days|timestamp|nonce
                long nonce = RANDOM.nextLong() & Long.MAX_VALUE;
                String plaintext = plan + "|" + days + "|" + System.currentTimeMillis() + "|" + nonce;

                // AES-256-GCM 加密
                byte[] iv = new byte[GCM_IV_LEN];
                RANDOM.nextBytes(iv);
                Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
                cipher.init(Cipher.ENCRYPT_MODE, key, new GCMParameterSpec(GCM_TAG_LEN, iv));
                byte[] ciphertext = cipher.doFinal(plaintext.getBytes());

                // code = KN-{iv_hex}{ciphertext_hex}
                StringBuilder code = new StringBuilder("KN-");
                for (byte b : iv) code.append(String.format("%02x", b));
                for (byte b : ciphertext) code.append(String.format("%02x", b));

                String sql = String.format(
                    "INSERT INTO kn_redeem_code (code, plan, duration_days, platform_source) " +
                    "VALUES ('%s', '%s', %d, '%s');\n",
                    code, plan, days, source);
                writer.write(sql);
            }
        }

        System.out.println("✓ 已生成: " + outFile.toAbsolutePath());
        System.out.println("  卡密格式: KN-{hex} (约 52 字符, AES-256-GCM 加密)");
        System.out.println("  导入: mysql -u root -p kn_cloud < " + outFile.getFileName());
    }
}
```

- [ ] **Step 2: RedeemService — 验证卡密**

```java
package dev.kn.cloud.api.redeem;

import dev.kn.cloud.common.entity.KnRedeemCode;
import dev.kn.cloud.common.entity.KnUser;
import dev.kn.cloud.common.mapper.KnRedeemCodeMapper;
import dev.kn.cloud.common.mapper.KnUserMapper;
import com.baomidou.mybatisplus.core.conditions.query.LambdaQueryWrapper;
import org.springframework.stereotype.Service;
import javax.crypto.Cipher;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.SecretKeySpec;
import java.security.SecureRandom;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.util.Base64;

@Service
public class RedeemService {
    private final KnRedeemCodeMapper codeMapper;
    private final KnUserMapper userMapper;
    private final SecretKeySpec aesKey;

    public RedeemService(KnRedeemCodeMapper cm, KnUserMapper um) {
        this.codeMapper = cm; this.userMapper = um;
        String keyB64 = System.getenv("REDEEM_AES_KEY");
        if (keyB64 == null) throw new BizException(ErrorCode.INTERNAL_ERROR, "REDEEM_AES_KEY 未设置");
        this.aesKey = new SecretKeySpec(Base64.getDecoder().decode(keyB64), "AES");
    }

    /** 验证并激活卡密，成功返回 plan 名称，失败抛 BizException */
    public RedeemResult redeem(String code, Long userId, String source) {
        // 1. 验证格式
        if (!code.startsWith("KN-") || code.length() < 50)
            throw new BizException(ErrorCode.INVALID_CODE_FORMAT);

        // 2. AES-256-GCM 解密
        String hex = code.substring(3);
        int ivLen = 24;
        byte[] iv = hexToBytes(hex.substring(0, ivLen));
        byte[] ct = hexToBytes(hex.substring(ivLen));

        String plaintext;
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, aesKey, new GCMParameterSpec(128, iv));
            plaintext = new String(cipher.doFinal(ct));
        } catch (Exception e) {
            throw new BizException(ErrorCode.CODE_NOT_FOUND, "解密失败，卡密无效");
        }

        // 3. 解析明文: plan|days|timestamp|nonce
        String[] parts = plaintext.split("\\|");
        if (parts.length != 4) throw new BizException(ErrorCode.CODE_NOT_FOUND);
        String plan = parts[0];
        int days = Integer.parseInt(parts[1]);

        // 4. 查 DB：code 存在
        var codeRow = codeMapper.selectOne(
            new LambdaQueryWrapper<KnRedeemCode>().eq(KnRedeemCode::getCode, code));
        if (codeRow == null) throw new BizException(ErrorCode.CODE_NOT_FOUND);

        // 5. 原子标记卡密已用（WHERE used_by IS NULL 防并发双花）
        var updateWrapper = new LambdaUpdateWrapper<KnRedeemCode>()
            .eq(KnRedeemCode::getCode, code)
            .isNull(KnRedeemCode::getUsedBy)
            .set(KnRedeemCode::getUsedBy, userId)
            .set(KnRedeemCode::getUsedAt, LocalDateTime.now())
            .set(KnRedeemCode::getRedeemSource, source);
        int rows = codeMapper.update(null, updateWrapper);
        if (rows == 0) throw new BizException(ErrorCode.CODE_ALREADY_USED);

        // 6. 更新用户会员（卡密已确认归属当前用户，安全写入）
        var user = userMapper.selectById(userId);
        user.setMembership(plan.contains("pro") ? "pro" : plan);
        user.setMembershipExpiresAt(LocalDate.now().plusDays(days));
        userMapper.updateById(user);

        return new RedeemResult(plan, days);
    }

    public record RedeemResult(String plan, int days) {}

    private static byte[] hexToBytes(String hex) {
        byte[] bytes = new byte[hex.length() / 2];
        for (int i = 0; i < bytes.length; i++) {
            bytes[i] = (byte) Integer.parseInt(hex.substring(i*2, i*2+2), 16);
        }
        return bytes;
    }
}
```

- [ ] **Step 2: 运行生成**

```bash
cd kn-cloud/tools
java GenerateCodes.java pro_monthly 100 365
# 输出: redeem_codes_pro_monthly_100_20260616.sql
```

- [ ] **Step 3: 手动导入**

```bash
mysql -u root -p kn_cloud < redeem_codes_pro_monthly_100_20260616.sql
```

- [ ] **Step 4: Commit**

```bash
git add tools/GenerateCodes.java
git commit -m "feat: redeem code generator — outputs INSERT SQL file"
```

---

## Cloud Phase 2 完成检查点

- [x] 会员到期/缓冲期逻辑 (canCreateSession + 24h grace period)
- [x] 定时任务 (每天 00:05 检查到期 + 提前 1 天警告)
- [x] APNs HTTP/2 推送服务
- [x] Push device token 注册 API
- [x] 卡密批量生成工具（AES-256-GCM 加密） + 验证服务（RedeemService）
