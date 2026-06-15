<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'

// Scenes to cycle through
const scenes = [
  {
    title: '可视化 Profile 管理',
    desc: '图形界面创建、编辑、复制 Profile，告别手写 YAML',
    sidebar: ['deepseek', 'anthropic', 'codex-work', 'openai-dev'],
    activeProfile: 'deepseek',
    mainType: 'detail',
    envVars: [
      { key: 'ANTHROPIC_BASE_URL', val: 'api.deepseek.com' },
      { key: 'ANTHROPIC_MODEL', val: 'deepseek-v4-pro' },
      { key: 'ANTHROPIC_AUTH_TOKEN', val: 'sk-****079c' },
    ],
  },
  {
    title: '智能扫描导入',
    desc: '自动检测现有配置，一键导入为 Profile',
    sidebar: ['deepseek', 'anthropic', 'codex-work'],
    activeProfile: null,
    mainType: 'scan',
    scanItems: [
      { name: 'deepseek', source: '~/.claude/settings.json', checked: true },
      { name: 'codex-default', source: '~/.codex/config.json', checked: true },
    ],
  },
  {
    title: '一键启动 Claude Code 交互',
    desc: '点击运行，终端自动启动，即刻与 AI 对话编码',
    sidebar: ['deepseek', 'anthropic', 'codex-work', 'openai-dev'],
    activeProfile: 'deepseek',
    mainType: 'terminal',
    terminalLines: [
      { text: 'ai claude deepseek', cls: 'cmd', delay: 0 },
      { text: '✓ Profile \'deepseek\' 已激活 (deepseek-v4-pro)', cls: 'info', delay: 0 },
      { text: '', cls: 'empty', delay: 0 },
      { text: '┌─────────────────────────────────────────┐', cls: 'ui', delay: 0 },
      { text: '│  ✨ Claude Code · deepseek-v4-pro       │', cls: 'ui', delay: 0 },
      { text: '│  工作目录: ~/project                    │', cls: 'ui', delay: 0 },
      { text: '└─────────────────────────────────────────┘', cls: 'ui', delay: 0 },
      { text: '', cls: 'empty', delay: 0 },
      { text: '❯ 帮我写一个 Redis 连接池，支持自动重连', cls: 'user', delay: 400 },
      { text: '', cls: 'empty', delay: 0 },
      { text: '⏺ 我来帮你实现。先看一下项目结构...', cls: 'think', delay: 600 },
      { text: '', cls: 'empty', delay: 0 },
      { text: '⏺ Tool: Glob  ·  *.py', cls: 'tool', delay: 500 },
      { text: '  找到 3 个文件：redis_client.py, config.py,', cls: 'tool-out', delay: 300 },
      { text: '  connection.py', cls: 'tool-out', delay: 100 },
      { text: '', cls: 'empty', delay: 0 },
      { text: '⏺ Tool: Read  ·  redis_client.py', cls: 'tool', delay: 500 },
      { text: '  class RedisClient: ...', cls: 'tool-out', delay: 200 },
    ],
  },
  {
    title: 'Quick Switcher (⌘K)',
    desc: '全局快速启动器，模糊搜索，按频率排序',
    sidebar: ['deepseek', 'anthropic', 'codex-work', 'openai-dev'],
    activeProfile: null,
    mainType: 'switcher',
    switcherResults: [
      { name: 'deepseek', subtitle: 'Claude Code · 使用 23 次', icon: '🤖' },
      { name: 'codex-work', subtitle: 'Codex · 使用 15 次', icon: '📟' },
      { name: 'anthropic', subtitle: 'Claude Code · 使用 8 次', icon: '🤖' },
      { name: 'openai-dev', subtitle: 'Codex · 使用 5 次', icon: '📟' },
    ],
  },
  {
    title: 'Token 用量仪表盘',
    desc: '按模型 / 项目统计 token，可视化成本趋势',
    sidebar: ['deepseek', 'anthropic', 'codex-work', 'openai-dev'],
    activeProfile: null,
    mainType: 'usage',
    usageStats: [
      { model: 'deepseek-v4-pro', tokens: '1.2M', cost: '$4.80', pct: 45 },
      { model: 'deepseek-v4-flash', tokens: '890K', cost: '$1.78', pct: 33 },
      { model: 'claude-sonnet-4-6', tokens: '420K', cost: '$6.30', pct: 16 },
      { model: 'gpt-5', tokens: '160K', cost: '$2.40', pct: 6 },
    ],
  },
]

const currentScene = ref(0)
const progress = ref(0)
let timer: ReturnType<typeof setInterval> | null = null
let progressTimer: ReturnType<typeof setInterval> | null = null

// Terminal scene gets more time to show the full interaction
const sceneDurations = [4000, 4000, 8000, 5000, 5000]

const scene = computed(() => scenes[currentScene.value])

// Animated cursor for terminal
const typedLineIdx = ref(-1)
const typedCharCount = ref(0)
function resetTyping() {
  typedLineIdx.value = -1
  typedCharCount.value = 0
}

onMounted(() => {
  let elapsed = 0
  function advanceScene() {
    currentScene.value = (currentScene.value + 1) % scenes.length
    progress.value = 0
    resetTyping()
    elapsed = 0
  }

  // Scene rotation with variable durations
  timer = setInterval(() => {
    elapsed += 100
    if (elapsed >= sceneDurations[currentScene.value]) {
      advanceScene()
      elapsed = 0
    }
  }, 100)

  // Progress bar
  progressTimer = setInterval(() => {
    const dur = sceneDurations[currentScene.value]
    progress.value = Math.min(100, progress.value + 100 / (dur / 50))
  }, 50)

  // Terminal typing animation
  const typeInterval = setInterval(() => {
    if (scenes[currentScene.value].mainType === 'terminal') {
      const lines = scenes[currentScene.value].terminalLines!
      if (typedLineIdx.value < lines.length - 1) {
        const nextLine = lines[typedLineIdx.value + 1]
        // respect per-line delay
        const globalElapsed = (sceneDurations[currentScene.value] * progress.value / 100)
        const lineStart = nextLine.delay || 0
        if (globalElapsed < lineStart) return

        typedCharCount.value++
        if (typedCharCount.value >= nextLine.text.length) {
          typedLineIdx.value++
          typedCharCount.value = 0
        }
      }
    }
  }, 30)

  onUnmounted(() => {
    clearInterval(typeInterval)
  })
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
  if (progressTimer) clearInterval(progressTimer)
})
</script>

<template>
  <div class="demo-wrapper">
    <!-- Scene label -->
    <div class="scene-info">
      <span class="scene-title">{{ scene.title }}</span>
      <span class="scene-desc">{{ scene.desc }}</span>
    </div>

    <!-- App window -->
    <div class="app-window">
      <!-- Title bar -->
      <div class="win-titlebar">
        <div class="win-dots">
          <span class="dot red"></span>
          <span class="dot yellow"></span>
          <span class="dot green"></span>
        </div>
        <span class="win-title">KN</span>
        <div class="win-progress">
          <div class="progress-track">
            <div class="progress-fill" :style="{ width: progress + '%' }"></div>
          </div>
        </div>
      </div>

      <!-- App body -->
      <div class="win-body">
        <!-- Sidebar -->
        <div class="win-sidebar">
          <div class="sidebar-head">
            <span class="sidebar-label">Profiles</span>
            <span class="sidebar-count">{{ scene.sidebar.length }}</span>
          </div>
          <div
            v-for="name in scene.sidebar"
            :key="name"
            class="sidebar-item"
            :class="{ active: name === scene.activeProfile }"
          >
            <span class="si-icon">▶</span>
            <span class="si-name">{{ name }}</span>
          </div>
        </div>

        <!-- Main content -->
        <div class="win-main" :class="{ 'win-main-terminal': scene.mainType === 'terminal' }">
          <!-- ── Scene: Profile Detail ── -->
          <template v-if="scene.mainType === 'detail'">
            <div class="detail-head">
              <span class="detail-name">{{ scene.activeProfile }}</span>
              <span class="detail-badge">Claude Code</span>
            </div>
            <div class="detail-env">
              <div
                v-for="(v, i) in scene.envVars"
                :key="v.key"
                class="env-row"
                :style="{ animationDelay: i * 0.1 + 's' }"
              >
                <span class="env-key">{{ v.key }}</span>
                <span class="env-val">{{ v.val }}</span>
              </div>
            </div>
            <div class="detail-actions">
              <span class="action-btn primary">▶ 运行</span>
              <span class="action-btn">编辑</span>
              <span class="action-btn">导出</span>
            </div>
          </template>

          <!-- ── Scene: Scan ── -->
          <template v-if="scene.mainType === 'scan'">
            <div class="scan-head">
              <span class="scan-title">🔍 系统扫描结果</span>
              <span class="scan-sub">发现 {{ scene.scanItems?.length }} 个可导入配置</span>
            </div>
            <div class="scan-list">
              <div
                v-for="(item, i) in scene.scanItems"
                :key="item.name"
                class="scan-row"
                :style="{ animationDelay: i * 0.15 + 's' }"
              >
                <span class="scan-check">✓</span>
                <div class="scan-info">
                  <span class="scan-name">{{ item.name }}</span>
                  <span class="scan-src">{{ item.source }}</span>
                </div>
                <span class="scan-tag">Claude</span>
              </div>
            </div>
            <div class="detail-actions">
              <span class="action-btn primary">一键导入</span>
              <span class="action-btn">取消</span>
            </div>
          </template>

          <!-- ── Scene: Terminal (Claude Code interaction) ── -->
          <template v-if="scene.mainType === 'terminal'">
            <div class="term-content term-full">
              <div
                v-for="(line, i) in scene.terminalLines"
                :key="i"
                class="term-line"
                :class="`term-${line.cls}`"
                v-show="i <= typedLineIdx"
              >
                <template v-if="i === typedLineIdx && line.text">
                  {{ line.text.slice(0, typedCharCount) }}
                  <span class="term-cursor">▌</span>
                </template>
                <template v-else>{{ line.text }}</template>
              </div>
            </div>
          </template>

          <!-- ── Scene: Quick Switcher ── -->
          <template v-if="scene.mainType === 'switcher'">
            <div class="switcher-overlay">
              <div class="switcher-box">
                <div class="switcher-input-row">
                  <span class="switcher-prompt">❯</span>
                  <span class="switcher-placeholder">搜索 Profile 或项目...</span>
                  <span class="switcher-hint">⌘K</span>
                </div>
                <div class="switcher-list">
                  <div
                    v-for="(item, i) in scene.switcherResults"
                    :key="item.name"
                    class="switcher-item"
                    :class="{ selected: i === 0 }"
                    :style="{ animationDelay: i * 0.1 + 's' }"
                  >
                    <span class="switcher-icon">{{ item.icon }}</span>
                    <div class="switcher-info">
                      <span class="switcher-name">{{ item.name }}</span>
                      <span class="switcher-subtitle">{{ item.subtitle }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </template>

          <!-- ── Scene: Token Usage Dashboard ── -->
          <template v-if="scene.mainType === 'usage'">
            <div class="usage-panel">
              <div class="usage-header">
                <span class="usage-title">📊 Token 用量</span>
                <span class="usage-period">最近 7 天</span>
              </div>
              <div class="usage-summary">
                <div class="usage-stat">
                  <span class="usage-stat-val">2.67M</span>
                  <span class="usage-stat-label">总 Token</span>
                </div>
                <div class="usage-stat">
                  <span class="usage-stat-val">$15.28</span>
                  <span class="usage-stat-label">预估费用</span>
                </div>
              </div>
              <div class="usage-bars">
                <div
                  v-for="(stat, i) in scene.usageStats"
                  :key="stat.model"
                  class="usage-bar-row"
                  :style="{ animationDelay: i * 0.12 + 's' }"
                >
                  <span class="usage-bar-label">{{ stat.model }}</span>
                  <div class="usage-bar-track">
                    <div
                      class="usage-bar-fill"
                      :style="{ width: stat.pct + '%' }"
                    ></div>
                  </div>
                  <span class="usage-bar-val">{{ stat.tokens }} / {{ stat.cost }}</span>
                </div>
              </div>
              <div class="usage-chart-hint">
                <span>▁ ▂ ▃ ▅ ▄ ▆ ▇</span>
                <span class="usage-chart-label">近 7 天用量趋势 ↑</span>
              </div>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- Scene dots -->
    <div class="scene-dots">
      <span
        v-for="(_s, i) in scenes"
        :key="i"
        class="scene-dot"
        :class="{ active: i === currentScene }"
      ></span>
    </div>
  </div>
</template>

<style scoped>
.demo-wrapper {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* ── Scene Info ──────────────── */
.scene-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.scene-title {
  font-family: var(--font-display);
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--clr-text);
}

.scene-desc {
  font-size: 0.78rem;
  color: var(--clr-text-muted);
}

/* ── App Window ──────────────── */
.app-window {
  border-radius: 12px;
  overflow: hidden;
  border: 1px solid var(--clr-border-strong);
  background: var(--clr-surface);
  box-shadow:
    0 0 0 1px rgba(0, 198, 255, 0.06),
    0 8px 40px rgba(0, 0, 0, 0.4);
}

/* ── Title Bar ───────────────── */
.win-titlebar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  background: var(--clr-raised);
  border-bottom: 1px solid var(--clr-border);
}

.win-dots {
  display: flex;
  gap: 6px;
}

.dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
}
.dot.red    { background: #f87171; }
.dot.yellow { background: #facc15; }
.dot.green  { background: #4ade80; }

.win-title {
  font-family: var(--font-body);
  font-size: 0.7rem;
  color: var(--clr-text-muted);
  flex: 1;
  text-align: center;
}

.win-progress {
  width: 80px;
}

.progress-track {
  height: 2px;
  background: var(--clr-border);
  border-radius: 1px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--clr-accent);
  border-radius: 1px;
  transition: width 0.05s linear;
}

/* ── Body ────────────────────── */
.win-body {
  display: flex;
  height: 320px;
}

/* ── Sidebar ─────────────────── */
.win-sidebar {
  width: 130px;
  border-right: 1px solid var(--clr-border);
  padding: 10px 0;
  background: rgba(0,0,0,0.15);
  overflow: hidden;
  flex-shrink: 0;
}

.sidebar-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 12px 8px;
}

.sidebar-label {
  font-size: 0.65rem;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--clr-text-muted);
}

.sidebar-count {
  font-family: var(--font-mono);
  font-size: 0.625rem;
  color: var(--clr-text-dim);
  background: var(--clr-raised);
  padding: 1px 6px;
  border-radius: 8px;
}

.sidebar-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  font-size: 0.7rem;
  color: var(--clr-text-secondary);
  transition: all 0.2s;
  cursor: default;
}

.sidebar-item.active {
  background: var(--clr-accent-dim);
  color: var(--clr-accent);
}

.si-icon {
  font-size: 0.45rem;
  opacity: 0.5;
}

.si-name {
  font-family: var(--font-mono);
  font-size: 0.65rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ── Main Panel ──────────────── */
.win-main {
  flex: 1;
  padding: 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  overflow: hidden;
}

.win-main-terminal {
  padding: 0;
  gap: 0;
}

/* ── Detail Scene ────────────── */
.detail-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.detail-name {
  font-family: var(--font-display);
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--clr-text);
}

.detail-badge {
  font-family: var(--font-mono);
  font-size: 0.6rem;
  padding: 2px 8px;
  border-radius: 10px;
  background: var(--clr-accent-dim);
  color: var(--clr-accent);
}

.detail-env {
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex: 1;
}

.env-row {
  display: flex;
  align-items: center;
  padding: 7px 10px;
  border-radius: 6px;
  background: #0d1117;
  border: 1px solid var(--clr-border);
  animation: fadeSlideIn 0.4s ease-out both;
}

.env-key {
  font-family: var(--font-mono);
  font-size: 0.625rem;
  font-weight: 500;
  color: var(--clr-accent);
  min-width: 140px;
}

.env-val {
  font-family: var(--font-mono);
  font-size: 0.625rem;
  color: var(--clr-text-secondary);
}

.detail-actions {
  display: flex;
  gap: 8px;
  padding-top: 4px;
}

.action-btn {
  padding: 5px 14px;
  border-radius: 6px;
  font-size: 0.6875rem;
  font-weight: 500;
  border: 1px solid var(--clr-border);
  color: var(--clr-text-secondary);
  background: var(--clr-raised);
  cursor: default;
  transition: all 0.2s;
}

.action-btn.primary {
  background: var(--clr-accent);
  color: #0a0e14;
  border-color: var(--clr-accent);
  font-weight: 600;
}

/* ── Scan Scene ──────────────── */
.scan-head {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.scan-title {
  font-weight: 600;
  font-size: 0.9rem;
}

.scan-sub {
  font-size: 0.7rem;
  color: var(--clr-text-muted);
}

.scan-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
}

.scan-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 6px;
  background: #0d1117;
  border: 1px solid var(--clr-border);
  animation: fadeSlideIn 0.4s ease-out both;
}

.scan-check {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  background: var(--clr-accent);
  color: #0a0e14;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.6rem;
  font-weight: 700;
  flex-shrink: 0;
}

.scan-info {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.scan-name {
  font-family: var(--font-mono);
  font-size: 0.7rem;
  font-weight: 500;
  color: var(--clr-text);
}

.scan-src {
  font-size: 0.6rem;
  color: var(--clr-text-muted);
}

.scan-tag {
  font-family: var(--font-mono);
  font-size: 0.55rem;
  padding: 2px 6px;
  border-radius: 8px;
  background: var(--clr-accent-dim);
  color: var(--clr-accent);
}

/* ── Terminal Scene ──────────── */
.term-full {
  flex: 1;
  background: #0c0f14;
  padding: 14px 16px;
  font-family: var(--font-mono);
  font-size: 0.67rem;
  line-height: 1.75;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

/* syntax coloring per line type */
.term-cmd      { color: #7ee787; }       /* green: user command */
.term-info     { color: #8b8fa3; }       /* grey: profile info */
.term-empty    { min-height: 0.25em; }
.term-ui       { color: #545868; }       /* dim: box borders */
.term-user     { color: #e8e8ec; }       /* white: user question */
.term-think    { color: #d2a8ff; }       /* purple: thinking indicator */
.term-tool     { color: #f0c674; }       /* yellow: tool call header */
.term-tool-out { color: #6e7681; }       /* grey: tool output */

.term-line {
  white-space: pre-wrap;
  word-break: break-all;
  flex-shrink: 0;
}

.term-cursor {
  color: var(--clr-accent);
  animation: cursorBlink 1s step-end infinite;
}

/* ── Switcher Scene ─────────── */
.switcher-overlay {
  flex: 1;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 24px 16px;
  background: rgba(0,0,0,0.2);
}

.switcher-box {
  width: 100%;
  max-width: 380px;
  background: #0d1117;
  border: 1px solid var(--clr-border-strong);
  border-radius: 10px;
  overflow: hidden;
  box-shadow: 0 16px 48px rgba(0,0,0,0.5);
}

.switcher-input-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--clr-border);
}

.switcher-prompt {
  color: var(--clr-green);
  font-family: var(--font-mono);
  font-size: 0.75rem;
}

.switcher-placeholder {
  flex: 1;
  font-size: 0.72rem;
  color: var(--clr-text-muted);
}

.switcher-hint {
  font-family: var(--font-mono);
  font-size: 0.58rem;
  padding: 2px 6px;
  border-radius: 4px;
  background: var(--clr-raised);
  color: var(--clr-text-dim);
  border: 1px solid var(--clr-border);
}

.switcher-list {
  display: flex;
  flex-direction: column;
}

.switcher-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  animation: fadeSlideIn 0.4s ease-out both;
}

.switcher-item.selected {
  background: var(--clr-accent-dim);
}

.switcher-icon {
  font-size: 0.85rem;
  flex-shrink: 0;
}

.switcher-info {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.switcher-name {
  font-family: var(--font-mono);
  font-size: 0.7rem;
  font-weight: 500;
  color: var(--clr-text);
}

.switcher-subtitle {
  font-size: 0.6rem;
  color: var(--clr-text-muted);
}

/* ── Usage Scene ─────────────── */
.usage-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px 18px;
}

.usage-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.usage-title {
  font-weight: 600;
  font-size: 0.85rem;
  color: var(--clr-text);
}

.usage-period {
  font-size: 0.65rem;
  padding: 2px 8px;
  border-radius: 8px;
  background: var(--clr-raised);
  color: var(--clr-text-muted);
}

.usage-summary {
  display: flex;
  gap: 12px;
}

.usage-stat {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 12px;
  border-radius: 8px;
  background: #0d1117;
  border: 1px solid var(--clr-border);
}

.usage-stat-val {
  font-family: var(--font-mono);
  font-size: 1rem;
  font-weight: 600;
  color: var(--clr-accent);
}

.usage-stat-label {
  font-size: 0.6rem;
  color: var(--clr-text-muted);
}

.usage-bars {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
}

.usage-bar-row {
  display: flex;
  align-items: center;
  gap: 8px;
  animation: fadeSlideIn 0.4s ease-out both;
}

.usage-bar-label {
  font-family: var(--font-mono);
  font-size: 0.58rem;
  color: var(--clr-text-secondary);
  width: 100px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.usage-bar-track {
  flex: 1;
  height: 6px;
  background: var(--clr-raised);
  border-radius: 3px;
  overflow: hidden;
}

.usage-bar-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--clr-accent), var(--clr-accent-secondary));
  border-radius: 3px;
  transition: width 0.5s ease;
}

.usage-bar-val {
  font-family: var(--font-mono);
  font-size: 0.58rem;
  color: var(--clr-text-dim);
  white-space: nowrap;
}

.usage-chart-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--clr-accent);
  letter-spacing: 0.2em;
}

.usage-chart-label {
  font-family: var(--font-body);
  font-size: 0.6rem;
  color: var(--clr-text-muted);
  letter-spacing: normal;
}

/* ── Scene Dots ──────────────── */
.scene-dots {
  display: flex;
  gap: 6px;
  justify-content: center;
}

.scene-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--clr-border-strong);
  transition: all 0.3s;
}

.scene-dot.active {
  background: var(--clr-accent);
  box-shadow: 0 0 6px var(--clr-accent-glow);
}

/* ── Animations ──────────────── */
@keyframes fadeSlideIn {
  from { opacity: 0; transform: translateY(6px); }
  to { opacity: 1; transform: translateY(0); }
}

@keyframes cursorBlink {
  50% { opacity: 0; }
}

/* ── Responsive ──────────────── */
@media (max-width: 768px) {
  .win-body {
    height: 280px;
  }
  .win-sidebar {
    width: 100px;
  }
}
</style>
