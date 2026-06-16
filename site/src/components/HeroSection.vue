<script setup lang="ts">
import DesktopDemo from './DesktopDemo.vue'
import { DOWNLOAD_URL_ARM, APP_VERSION } from '../config'
</script>

<template>
  <section class="hero relative">
    <!-- Background glow -->
    <div class="glow-orb hero-glow"></div>

    <div class="container">
      <div class="hero-grid">
        <!-- Left -->
        <div class="hero-text reveal">
          <span class="section-label">AI CLI 工具的统一管理中心</span>
          <h1 class="hero-title">
            告别手动改配置，<span class="hero-highlight">一行命令</span>切换 AI 环境
          </h1>

          <!-- Supported CLI tools -->
          <div class="hero-tools">
            <span class="tool-badge">🤖 Claude Code</span>
            <span class="tool-badge">📟 Codex CLI</span>
            <span class="tool-badge">🇨🇳 Qoder CN</span>
          </div>

          <div class="hero-features-wrap">
            <ul class="hero-features">
              <li>多 Profile 管理，一键切换 API Key / Model / Base URL</li>
              <li>会话级隔离，终端之间互不干扰，退出自动清除</li>
              <li>Quick Switcher (⌘K) 全局快速启动，按使用频率排序</li>
              <li>自动追踪 Token 用量，按模型 / 项目统计成本</li>
              <li>可视化 Hook 管理 + Agent 管理 + Shell 补全</li>
            </ul>
          </div>

          <div class="hero-actions">
            <a :href="DOWNLOAD_URL_ARM" class="btn btn-primary">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              下载 macOS 版
            </a>
            <a href="#/docs" class="btn btn-outline">查看文档</a>
          </div>
          <p class="hero-meta">v{{ APP_VERSION }} · macOS 12+ · 开源免费</p>
        </div>

        <!-- Right: Desktop GUI Demo -->
        <div class="hero-demo reveal">
          <DesktopDemo />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.hero {
  padding: 120px 28px 56px;
  min-height: auto;
  display: flex;
  align-items: flex-start;
  overflow: hidden;
}

.hero-glow {
  width: 700px;
  height: 700px;
  background: var(--clr-accent);
  top: -250px;
  right: -200px;
  opacity: 0.06;
  filter: blur(140px);
}

.hero-grid {
  max-width: var(--max-w);
  margin: 0 auto;
  width: 100%;
  display: grid;
  grid-template-columns: 1fr 1.3fr;
  gap: 56px;
  align-items: start;
  position: relative;
  z-index: 1;
}

/* ── Left text ─────────────── */
.hero-text {
  display: flex;
  flex-direction: column;
  gap: 16px;
  align-self: stretch;
}

.hero-title {
  font-family: var(--font-display);
  font-size: clamp(1.625rem, 3.2vw, 2.25rem);
  font-weight: 700;
  line-height: 1.25;
  letter-spacing: -0.02em;
  color: var(--clr-text);
}

.hero-highlight {
  color: var(--clr-accent);
  position: relative;
}

.hero-highlight::after {
  content: '';
  position: absolute;
  bottom: 2px;
  left: 0;
  right: 0;
  height: 6px;
  background: var(--clr-accent);
  opacity: 0.15;
  border-radius: 3px;
}

/* ── Tool badges ───────────── */
.hero-tools {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

.tool-badge {
  display: inline-flex;
  align-items: center;
  padding: 4px 12px;
  border-radius: 100px;
  font-size: 0.75rem;
  font-weight: 500;
  background: var(--clr-raised);
  border: 1px solid var(--clr-border-strong);
  color: var(--clr-text-secondary);
}

/* ── Feature bullets ────────── */
.hero-features-wrap {
  flex: 1;
  display: flex;
  align-items: center;
}

.hero-features {
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 0;
}

.hero-features li {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.5;
  padding-left: 22px;
  position: relative;
}

.hero-features li::before {
  content: '';
  position: absolute;
  left: 0;
  top: 10px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--clr-accent);
  opacity: 0.7;
}

.hero-actions {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  padding-top: 8px;
}

.hero-meta {
  font-size: 0.8125rem;
  color: var(--clr-text-muted);
  font-family: var(--font-mono);
}

/* ── Demo panel ─────────────── */
.hero-demo {
  position: relative;
}

/* ── Responsive ─────────────── */
@media (max-width: 768px) {
  .hero {
    padding: 110px 16px 56px;
    min-height: auto;
  }
  .hero-grid {
    grid-template-columns: 1fr;
    gap: 40px;
  }
}
</style>
