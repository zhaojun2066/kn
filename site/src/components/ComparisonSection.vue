<script setup lang="ts">
const comparisons = [
  { aspect: '运行配置', bad: '每个 AI CLI 单独维护配置和 API Key', good: '统一管理 Claude Code / Codex / Qoder 运行配置' },
  { aspect: '会话隔离', bad: '全局环境变量互相影响，切换后容易串配置', good: '每个终端会话独立注入，退出自动清除' },
  { aspect: '项目启动', bad: '手动进目录、换配置、重新打开终端', good: '项目级 profile 自动绑定，Quick Switcher 一键启动' },
  { aspect: '远程跟进', bad: '只能守在电脑前等任务输出', good: '手机远程发起会话，实时查看 PTY 输出和工作结果' },
  { aspect: '扩展资源', bad: 'Hook、Skills、Agent、Commands 分散在文件夹里', good: '电脑端统一浏览、编辑、启用和来源过滤' },
  { aspect: '用量安全', bad: 'Token 成本靠猜，配置改错很难回滚', good: '按模型 / 项目统计用量，配置自动备份和恢复' },
]
</script>

<template>
  <section class="comp section" id="comparison">
    <div class="container">
      <div class="comp-header reveal">
        <span class="section-label">为什么选择我们</span>
        <h2 class="section-title">从手动配置到多端工作流</h2>
        <p class="section-desc">
          KN 把运行配置、项目终端、手机远程和扩展管理串成一条完整 AI CLI 工作流。
        </p>
      </div>

      <div class="comp-table reveal">
        <!-- Table header -->
        <div class="comp-thead">
          <div class="comp-th aspect-th">
            <span class="th-label">对比维度</span>
          </div>
          <div class="comp-th bad-th">
            <span class="th-badge bad">❌ 传统方式</span>
          </div>
          <div class="comp-divider-v"></div>
          <div class="comp-th good-th">
            <span class="th-badge good">✨ KN</span>
          </div>
        </div>

        <!-- Table body -->
        <div class="comp-tbody">
          <div
            v-for="(row, i) in comparisons"
            :key="i"
            class="comp-tr"
            :style="{ animationDelay: `${i * 80}ms` }"
          >
            <div class="comp-td aspect-td">
              <span class="aspect-label">{{ row.aspect }}</span>
            </div>
            <div class="comp-td bad-td">
              <span class="td-text">{{ row.bad }}</span>
            </div>
            <div class="comp-divider-v"></div>
            <div class="comp-td good-td">
              <span class="td-text">{{ row.good }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.comp-header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 40px;
}

.comp-header :deep(.section-desc) {
  max-width: 720px;
}

/* ── Table Container ─────────── */
.comp-table {
  border: 1px solid var(--clr-border-strong);
  border-radius: var(--radius-lg);
  overflow: hidden;
  background: var(--clr-surface);
}

/* ── Header ──────────────────── */
.comp-thead {
  display: grid;
  grid-template-columns: 140px 1fr 1px 1fr;
  background: var(--clr-raised);
  border-bottom: 1px solid var(--clr-border-strong);
}

.comp-th {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px 20px;
}

.th-badge {
  display: inline-flex;
  align-items: center;
  padding: 5px 16px;
  border-radius: 100px;
  font-size: 0.8125rem;
  font-weight: 600;
  white-space: nowrap;
}

.th-label {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--clr-text-muted);
}

.th-badge.bad {
  background: var(--clr-red-bg);
  color: var(--clr-red);
}

.th-badge.good {
  background: var(--clr-accent-dim);
  color: var(--clr-accent);
}

/* ── Body Rows ───────────────── */
.comp-tbody {
  display: flex;
  flex-direction: column;
}

.comp-tr {
  display: grid;
  grid-template-columns: 140px 1fr 1px 1fr;
  animation: fadeInRow 0.4s ease-out both;
}

.comp-tr:nth-child(even) {
  background: rgba(255,255,255,0.012);
}

.comp-td {
  display: flex;
  align-items: center;
  padding: 16px 24px;
  border-bottom: 1px solid var(--clr-border);
}

.comp-tr:last-child .comp-td {
  border-bottom: none;
}

/* ── Aspect Column ───────────── */
.aspect-td {
  justify-content: flex-start;
  padding-left: 24px;
}

.aspect-label {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--clr-text);
  letter-spacing: 0.03em;
  white-space: nowrap;
}

/* ── Bad / Good Columns ──────── */
.bad-td {
  background: rgba(248, 113, 113, 0.03);
}

.good-td {
  background: rgba(0, 198, 255, 0.02);
}

.td-text {
  font-size: 0.9375rem;
  line-height: 1.55;
}

.bad-td .td-text {
  color: #e89090;
}

.good-td .td-text {
  color: #9ac8d8;
}

/* ── Vertical Divider ────────── */
.comp-divider-v {
  width: 1px;
  background: var(--clr-border-strong);
}

/* ── Animations ──────────────── */
@keyframes fadeInRow {
  from { opacity: 0; transform: translateX(-8px); }
  to { opacity: 1; transform: translateX(0); }
}

/* ── Responsive ──────────────── */
@media (max-width: 768px) {
  .comp-table {
    max-width: 100%;
  }

  .comp-thead {
    grid-template-columns: 1fr 1fr;
  }

  .comp-tr {
    grid-template-columns: 1fr 1fr;
  }

  .aspect-td,
  .comp-divider-v,
  .aspect-th {
    display: none;
  }

  .comp-th {
    padding: 12px 16px;
  }

  .comp-td {
    padding: 14px 16px;
  }

  .th-badge {
    font-size: 0.75rem;
    padding: 4px 12px;
  }

  .td-text {
    font-size: 0.875rem;
  }
}
</style>
