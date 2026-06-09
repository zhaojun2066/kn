<script setup lang="ts">
const comparisons = [
  { aspect: '切换 API Key', bad: '手动编辑 settings.json，改完重启 Claude Code', good: '一行命令：ai claude <profile>' },
  { aspect: '多账号并行', bad: '全局配置，只能同时用一个 key', good: '每个终端独立 Profile，互不干扰' },
  { aspect: '配置管理', bad: '记事本/备忘录记 key，容易丢失', good: '可视化 GUI + YAML 同步，所见即所得' },
  { aspect: '环境隔离', bad: '改完影响所有终端窗口', good: '会话级注入，退出自动清除' },
  { aspect: '团队协作', bad: '手动分享配置文件，版本混乱', good: '一键导出 JSON，对方一键导入' },
  { aspect: '错误恢复', bad: '改错格式导致 Claude Code 报错', good: 'GUI 校验 + 自动备份 + 一键恢复' },
  { aspect: '多 CLI 工具', bad: '每种工具单独配置，格式各不同', good: '统一 Profile 管理 Claude Code / Codex / Qoder' },
  { aspect: 'Shell 补全', bad: '无自动补全，全靠记忆', good: 'zsh + bash 自动配置补全，Tab 即出' },
  { aspect: '项目级配置', bad: '手动维护 .env 文件，容易泄露', good: '.ai-profile 文件自动绑定，进目录即切换' },
]
</script>

<template>
  <section class="comp section" id="comparison">
    <div class="container">
      <div class="comp-header reveal">
        <span class="section-label">为什么选择我们</span>
        <h2 class="section-title">告别手动改配置</h2>
        <p class="section-desc">
          不用再打开编辑器 → 找到 settings.json → 改 key → 保存 → 重启。一个命令，全部搞定。
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
            <span class="th-badge good">✨ AI Profile Manager</span>
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
  max-width: none;
  white-space: nowrap;
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
