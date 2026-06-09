<script setup lang="ts">
import { APP_VERSION, GITHUB_RELEASES } from '../config'
</script>

<template>
  <section class="dl section" id="download">
    <div class="container">
      <div class="dl-header reveal">
        <span class="section-label">下载</span>
        <h2 class="section-title">获取 AI Profile Manager</h2>
        <p class="section-desc">支持 macOS (ARM + Intel)、Windows、Linux 全平台。</p>
      </div>

      <div class="dl-grid reveal">
        <div class="dl-card glass-card">
          <div class="dl-platform-icon">
            <svg width="36" height="42" viewBox="0 0 24 28" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M18.73 14.84c0 3.74 2.98 5.06 3.04 5.08-.02.08-.47 1.62-1.56 3.2-.94 1.41-1.94 2.82-3.48 2.85-1.52.03-2.02-.9-3.76-.9-1.74 0-2.28.87-3.72.93-1.5.05-2.63-1.53-3.58-2.94C4.02 21.16 2 16.23 4.5 12.49c1.24-2.2 3.44-3.59 5.84-3.63 1.53-.03 2.97 1.05 3.9 1.05.93 0 2.66-1.3 4.5-1.11.76.03 2.91.31 4.28 2.37-.1.07-2.56 1.53-2.53 4.28.03 2.32 2.04 3.14 2.06 3.15-.02.06-.3 1.07-1.04 2.14-.65.94-1.27 1.88-1.75 1.88-.52 0-1.04-.9-1.9-.9" fill="currentColor"/>
            </svg>
          </div>
          <h3 class="dl-platform-title">macOS</h3>
          <p class="dl-platform-desc">Apple Silicon (M1-M4) & Intel</p>
          <div class="dl-links">
            <a :href="GITHUB_RELEASES" target="_blank" rel="noopener" class="btn btn-outline dl-link-btn">
              下载 .dmg (ARM)
            </a>
            <a :href="GITHUB_RELEASES" target="_blank" rel="noopener" class="btn btn-outline dl-link-btn">
              下载 .dmg (Intel)
            </a>
          </div>
        </div>
        <div class="dl-card glass-card">
          <div class="dl-platform-icon">
            <svg width="40" height="40" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
              <rect x="2" y="3" width="9" height="9" rx="0.5" fill="currentColor"/>
              <rect x="13" y="3" width="9" height="9" rx="0.5" fill="currentColor" opacity="0.7"/>
              <rect x="2" y="13" width="9" height="9" rx="0.5" fill="currentColor" opacity="0.7"/>
              <rect x="13" y="13" width="9" height="9" rx="0.5" fill="currentColor"/>
            </svg>
          </div>
          <h3 class="dl-platform-title">Windows</h3>
          <p class="dl-platform-desc">Windows 10 / 11 x64</p>
          <div class="dl-links">
            <a :href="GITHUB_RELEASES" target="_blank" rel="noopener" class="btn btn-outline dl-link-btn">
              下载 .msi
            </a>
          </div>
        </div>
        <div class="dl-card glass-card">
          <div class="dl-platform-icon">
            <svg width="36" height="42" viewBox="0 0 24 28" fill="none" xmlns="http://www.w3.org/2000/svg">
              <ellipse cx="12" cy="12" rx="11" ry="10" stroke="currentColor" stroke-width="1.5"/>
              <ellipse cx="12" cy="10" rx="5" ry="3.5" stroke="currentColor" stroke-width="1.2"/>
              <circle cx="9.5" cy="10" r="1.2" fill="currentColor"/>
              <circle cx="14.5" cy="10" r="1.2" fill="currentColor"/>
              <path d="M8 12.5c1 2 3 3 4 3s3-1 4-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              <path d="M12 14v4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              <path d="M8 18c1 2 2.5 3 4 3s3-1 4-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
              <path d="M4 22l8-4 8 4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            </svg>
          </div>
          <h3 class="dl-platform-title">Linux</h3>
          <p class="dl-platform-desc">Ubuntu / Debian / Fedora / Arch</p>
          <div class="dl-links">
            <a :href="GITHUB_RELEASES" target="_blank" rel="noopener" class="btn btn-outline dl-link-btn">
              下载 .AppImage
            </a>
          </div>
        </div>
      </div>

      <div class="dl-actions reveal" style="margin-top: 32px;">
        <a :href="GITHUB_RELEASES" target="_blank" rel="noopener" class="btn btn-primary dl-btn">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
          GitHub Releases
        </a>
        <p class="dl-ver">最新版本 v{{ APP_VERSION }} · 全平台支持</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
.dl-header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 32px;
}

.dl-actions {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

.dl-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 20px;
  max-width: 900px;
  margin: 0 auto;
}

.dl-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 32px 24px;
  text-align: center;
}

.dl-platform-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 48px;
  margin-bottom: 4px;
  color: var(--clr-text-secondary);
  transition: color 0.3s ease;
}

.dl-card:hover .dl-platform-icon {
  color: var(--clr-accent);
}

.dl-platform-title {
  font-family: var(--font-display);
  font-size: 1.0625rem;
  font-weight: 600;
  color: var(--clr-text);
}

.dl-platform-desc {
  font-size: 0.8125rem;
  color: var(--clr-text-muted);
}

.dl-links {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  margin-top: 4px;
}

.dl-link-btn {
  justify-content: center;
  font-size: 0.8125rem;
  padding: 8px 12px !important;
}

.dl-btn {
  font-size: 0.9375rem;
  padding: 12px 32px !important;
}

.dl-ver {
  font-size: 0.75rem;
  color: var(--clr-text-muted);
  font-family: var(--font-mono);
}

@media (max-width: 768px) {
  .dl-grid {
    grid-template-columns: 1fr;
  }
}
</style>
