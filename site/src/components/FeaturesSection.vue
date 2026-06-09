<script setup lang="ts">
const features = [
  {
    num: '01',
    title: '会话级隔离',
    desc: '每个终端窗口独立注入环境变量，退出后自动清除。不同项目用不同 API Key，零干扰。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>`,
  },
  {
    num: '02',
    title: '可视化 Profile 管理',
    desc: '告别手写 YAML。图形界面创建、编辑、复制、导入导出 Profile，所见即所得。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2"/><line x1="8" y1="21" x2="16" y2="21"/><line x1="12" y1="17" x2="12" y2="21"/></svg>`,
  },
  {
    num: '03',
    title: '内置终端',
    desc: '基于 xterm.js + WebGL，右侧面板一键运行命令，无需离开 App。支持多标签页与会话历史。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg>`,
  },
  {
    num: '04',
    title: '智能扫描导入',
    desc: '自动检测 <code>~/.claude/settings.json</code> 和 <code>~/.codex/</code> 现有配置，一键导入为 Profile。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>`,
  },
  {
    num: '05',
    title: 'CLI + GUI 共享配置',
    desc: '桌面 App 和命令行共用同一份 <code>config.yaml</code>，两边操作实时同步，文件锁保证并发安全。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M16 18l6-6-6-6"/><path d="M8 6l-6 6 6 6"/></svg>`,
  },
  {
    num: '06',
    title: '会话历史回溯',
    desc: '自动记录终端运行历史，随时一键恢复到之前的会话状态。配色主题独立切换，6 套终端主题可选。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>`,
  },
  {
    num: '07',
    title: 'Quick Switcher (⌘K)',
    desc: '全局快速启动器，模糊搜索 Profile 和项目，按使用频率排序，一键直达目标会话。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/><path d="M8 11h6"/></svg>`,
  },
  {
    num: '08',
    title: 'Token 用量追踪',
    desc: '自动记录每次调用的 token 消耗，按模型和项目维度统计，可配置成本估算，用量趋势一目了然。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/></svg>`,
  },
  {
    num: '09',
    title: 'Hook 可视化管理',
    desc: '图形界面创建 / 编辑 Hook，支持 Stop、PreTool、PostTool 等事件类型，执行日志可追溯。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>`,
  },
  {
    num: '10',
    title: 'Skills 管理',
    desc: '浏览、创建、编辑 Claude Code Skills。支持用户级、项目级和系统内置三种来源，一目了然。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 22 8.5 12 15 2 8.5 12 2"/><line x1="12" y1="15" x2="12" y2="22"/></svg>`,
  },
  {
    num: '11',
    title: 'Plugins & Commands',
    desc: 'Marketplace 插件安装与更新，自定义 Commands 管理。按 CLI 类型分组，一键启用 / 禁用。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><circle cx="6" cy="6" r="1" fill="currentColor"/><circle cx="6" cy="18" r="1" fill="currentColor"/></svg>`,
  },
  {
    num: '12',
    title: 'Agent 管理',
    desc: '浏览 Claude Code / Codex / Qoder 的 Agent 配置。内置 Agent 只读保护，用户和项目 Agent 可编辑。',
    icon: `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="10" r="3"/><path d="M12 2a8 8 0 0 0-8 8c0 5.4 8 12 8 12s8-6.6 8-12a8 8 0 0 0-8-8z"/></svg>`,
  },
]
</script>

<template>
  <section class="feat section" id="features">
    <div class="container">
      <div class="feat-header reveal">
        <span class="section-label">核心特性</span>
        <h2 class="section-title">专为多会话 API Key 管理场景打造</h2>
        <p class="section-desc">
          不论是 DeepSeek 中转、官方 Anthropic、OpenAI 兼容 API——一套 Profile 全管理。
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
  max-width: none;
  white-space: nowrap;
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
