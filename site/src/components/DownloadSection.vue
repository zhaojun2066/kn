<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { APP_VERSION, IOS_APP_URL, RELEASE_API_URL } from '../config'

interface Release { available: boolean; version?: string; url?: string; sha256?: string }
const arm = ref<Release | null>(null)
const intel = ref<Release | null>(null)
const loading = ref(true)
async function loadRelease(platform: string): Promise<Release> {
  const response = await fetch(`${RELEASE_API_URL}?platform=${platform}&currentVersion=0.0.0`)
  const payload = await response.json() as { code: number; data: Release }
  if (!response.ok || payload.code !== 0 || !payload.data.available || !payload.data.url) throw new Error('release unavailable')
  return payload.data
}
onMounted(async () => {
  try { [arm.value, intel.value] = await Promise.all([loadRelease('darwin-aarch64'), loadRelease('darwin-x86_64')]) }
  finally { loading.value = false }
})
</script>

<template>
  <section class="dl section" id="download">
    <div class="container">
      <div class="dl-header reveal">
        <span class="section-label">下载</span>
        <h2 class="section-title">获取 KN</h2>
        <p class="section-desc">先安装电脑端应用，再用手机 App 绑定设备和远程控制 AI CLI 会话。</p>
      </div>

      <div class="dl-grid reveal">
        <div class="dl-card glass-card">
          <div class="dl-platform-icon">
            <svg width="36" height="42" viewBox="0 0 24 28" fill="none" xmlns="http://www.w3.org/2000/svg">
              <path d="M18.73 14.84c0 3.74 2.98 5.06 3.04 5.08-.02.08-.47 1.62-1.56 3.2-.94 1.41-1.94 2.82-3.48 2.85-1.52.03-2.02-.9-3.76-.9-1.74 0-2.28.87-3.72.93-1.5.05-2.63-1.53-3.58-2.94C4.02 21.16 2 16.23 4.5 12.49c1.24-2.2 3.44-3.59 5.84-3.63 1.53-.03 2.97 1.05 3.9 1.05.93 0 2.66-1.3 4.5-1.11.76.03 2.91.31 4.28 2.37-.1.07-2.56 1.53-2.53 4.28.03 2.32 2.04 3.14 2.06 3.15-.02.06-.3 1.07-1.04 2.14-.65.94-1.27 1.88-1.75 1.88-.52 0-1.04-.9-1.9-.9" fill="currentColor"/>
            </svg>
          </div>
          <h3 class="dl-platform-title">macOS</h3>
          <p class="dl-platform-desc">Apple Silicon & Intel</p>
          <div class="dl-links">
            <a v-if="arm?.url" :href="arm.url" class="btn btn-outline dl-link-btn">
              下载 .dmg (ARM)
            </a>
            <a v-if="intel?.url" :href="intel.url" class="btn btn-outline dl-link-btn">
              下载 .dmg (Intel)
            </a>
            <span v-if="!loading && !arm && !intel" class="dl-platform-desc">当前没有可下载的正式版本</span>
          </div>
        </div>

        <div class="dl-card glass-card">
          <div class="dl-platform-icon">
            <svg width="30" height="44" viewBox="0 0 24 36" fill="none" xmlns="http://www.w3.org/2000/svg">
              <rect x="5" y="1.5" width="14" height="33" rx="3" stroke="currentColor" stroke-width="2"/>
              <path d="M10 5h4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
              <circle cx="12" cy="30" r="1.2" fill="currentColor"/>
            </svg>
          </div>
          <h3 class="dl-platform-title">手机 App</h3>
          <p class="dl-platform-desc">绑定电脑端，远程发起和跟进 AI 会话</p>
          <div class="dl-links">
            <a v-if="IOS_APP_URL" :href="IOS_APP_URL" target="_blank" rel="noopener" class="btn btn-outline dl-link-btn">
              打开 App Store
            </a>
            <span v-else class="dl-platform-desc">手机 App 暂未开放公开下载</span>
            <a href="#mobile" class="btn btn-outline dl-link-btn">
              查看移动端功能
            </a>
          </div>
        </div>
      </div>

      <div class="dl-actions reveal" style="margin-top: 32px;">
        <p class="dl-ver">{{ arm?.version ? `当前正式版 v${arm.version}` : `版本 v${APP_VERSION}` }} · macOS 12 或更高版本</p>
        <p v-if="arm?.sha256" class="dl-ver">ARM SHA-256: {{ arm.sha256 }}</p>
        <p v-if="intel?.sha256" class="dl-ver">Intel SHA-256: {{ intel.sha256 }}</p>
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
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 20px;
  max-width: 780px;
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
