<script setup lang="ts">
const features = [
  {
    num: '01',
    title: '运行配置与会话隔离',
    desc: '统一管理 Claude Code / Codex / Qoder 的 API Key、Model 和 Base URL。每个终端会话独立注入，退出自动清除。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>`,
  },
  {
    num: '02',
    title: '内置终端与项目启动',
    desc: '电脑端内置真实 PTY 终端，支持多标签页、项目目录启动、命令运行和会话历史恢复。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>`,
  },
  {
    num: '03',
    title: '手机远程控制',
    desc: '手机 App 绑定电脑端后，可选择项目、启动远程会话、发送输入，并实时查看 PTY 输出和工作结果。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="7" y="2" width="10" height="20" rx="2"/><path d="M11 18h2"/><path d="M10 6h4"/></svg>`,
  },
  {
    num: '04',
    title: '项目与会话工作流',
    desc: 'Quick Switcher 快速定位项目和运行配置，项目级 profile 自动绑定，历史会话随时回到上下文。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/><path d="M8 11h6"/></svg>`,
  },
  {
    num: '05',
    title: 'Hook / Skills / Agent 管理',
    desc: '可视化管理 Hook、Skills、Agents、Plugins 和 Commands，按 CLI 与来源分组，内置资源只读保护。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 22 8.5 12 15 2 8.5 12 2"/><line x1="12" y1="15" x2="12" y2="22"/></svg>`,
  },
  {
    num: '06',
    title: '用量追踪与配置安全',
    desc: '按模型和项目统计 Token 用量，运行配置自动备份、导入导出、文件锁写入，减少误改和丢失。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></svg>`,
  },
]
</script>

<template>
  <section class="feat section" id="features">
    <div class="container">
      <div class="feat-header reveal">
        <span class="section-label">核心特性</span>
        <h2 class="section-title">专为 AI CLI 多端工作流打造</h2>
        <p class="section-desc">
          从桌面配置、内置终端到手机远程控制，统一管理 Claude Code / Codex / Qoder 的运行环境。
        </p>
      </div>

      <div class="feat-grid">
        <div
          v-for="(f, i) in features"
          :key="f.num"
          class="feat-card glass-card reveal"
          :style="{ transitionDelay: `${i * 80}ms` }"
        >
          <div class="feat-icon" v-html="f.icon"></div>
          <span class="feat-num">{{ f.num }}</span>
          <h3 class="feat-title">{{ f.title }}</h3>
          <p class="feat-desc" v-html="f.desc"></p>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.feat-header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
  margin-bottom: 44px;
}

.feat-header :deep(.section-desc) {
  max-width: 720px;
}

.feat-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 20px;
}

.feat-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 34px 28px;
}

.feat-card:hover {
  border-color: var(--clr-border-accent);
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.3), 0 0 40px rgba(0, 198, 255, 0.04);
}

.feat-icon {
  color: var(--clr-accent);
  margin-bottom: 2px;
  display: flex;
  align-items: center;
}

.feat-num {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--clr-text-muted);
  letter-spacing: 0.06em;
}

.feat-title {
  font-family: var(--font-display);
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--clr-text);
}

.feat-desc {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.65;
}

.feat-desc :deep(code) {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  background: var(--clr-raised);
  padding: 1px 6px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--clr-border);
  color: var(--clr-accent-text);
}

@media (max-width: 768px) {
  .feat-grid {
    grid-template-columns: 1fr;
  }
}
</style>
