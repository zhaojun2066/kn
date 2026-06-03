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
          <span class="section-label">会话级环境变量管理</span>
          <h1 class="hero-title">
            可视化切换 <span class="hero-highlight">API Key</span>，轻松管理
          </h1>
          <div class="hero-features-wrap">
            <ul class="hero-features">
              <li>多 Profile 管理，一键切换 API Key</li>
              <li>会话级隔离，终端之间互不干扰</li>
              <li>可视化界面，告别手写配置文件</li>
              <li>CLI + GUI 共享配置，实时同步</li>
            </ul>
          </div>

          <div class="hero-actions">
            <a :href="DOWNLOAD_URL_ARM" class="btn btn-primary">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              下载
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
