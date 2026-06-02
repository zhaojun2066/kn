<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { APP_VERSION } from '../config'
import { useTheme } from '../composables/useTheme'

const { theme, toggle: toggleTheme } = useTheme()

const scrolled = ref(false)
const mobileOpen = ref(false)

function onScroll() {
  scrolled.value = window.scrollY > 20
}

function scrollToTop() {
  window.scrollTo({ top: 0, behavior: 'smooth' })
}

onMounted(() => {
  window.addEventListener('scroll', onScroll, { passive: true })
  onScroll()
})

onUnmounted(() => {
  window.removeEventListener('scroll', onScroll)
})
</script>

<template>
  <nav class="nav" :class="{ 'nav-scrolled': scrolled }">
    <div class="nav-inner">
      <a href="#" class="nav-brand" @click.prevent="scrollToTop">
        <img src="/icon-128.png" alt="" class="nav-icon" />
        <span class="nav-name">AI Profile Manager</span>
      </a>

      <!-- Desktop links -->
      <div class="nav-links">
        <a href="#features">特性</a>
        <a href="#docs">文档</a>
        <button class="theme-toggle" @click="toggleTheme" :aria-label="theme === 'dark' ? '切换亮色主题' : '切换暗色主题'">
          <svg v-if="theme === 'dark'" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
          <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
        </button>
        <a href="#download" class="nav-cta">下载 v{{ APP_VERSION }}</a>
      </div>

      <!-- Mobile toggle -->
      <button class="nav-toggle" @click="mobileOpen = !mobileOpen" :aria-label="mobileOpen ? '关闭菜单' : '打开菜单'">
        <span :class="{ open: mobileOpen }"></span>
        <span :class="{ open: mobileOpen }"></span>
        <span :class="{ open: mobileOpen }"></span>
      </button>
    </div>

    <!-- Mobile menu -->
    <div class="nav-mobile" :class="{ open: mobileOpen }">
      <a href="#features" @click="mobileOpen = false">特性</a>
      <a href="#docs" @click="mobileOpen = false">文档</a>
      <a href="#download" class="nav-cta-mobile" @click="mobileOpen = false">下载 v{{ APP_VERSION }}</a>
    </div>
  </nav>
</template>

<style scoped>
.nav {
  position: fixed;
  top: 0; left: 0; right: 0;
  z-index: 100;
  padding: 14px 0;
  transition: background 0.35s, box-shadow 0.35s, padding 0.35s;
}

.nav-scrolled {
  background: rgba(10, 14, 20, 0.82);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  box-shadow: 0 1px 0 var(--clr-border);
  padding: 10px 0;
}

.nav-inner {
  max-width: var(--max-w);
  margin: 0 auto;
  padding: 0 28px;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

/* ── Brand ──────────────────── */
.nav-brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.nav-icon {
  width: 28px;
  height: 28px;
  border-radius: 6px;
}

.nav-name {
  font-family: var(--font-display);
  font-weight: 600;
  font-size: 1rem;
  color: var(--clr-text);
  letter-spacing: -0.01em;
}

/* ── Links ──────────────────── */
.nav-links {
  display: flex;
  align-items: center;
  gap: 32px;
}

.nav-links a {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--clr-text-secondary);
  transition: color 0.2s;
}

.nav-links a:hover {
  color: var(--clr-text);
}

/* ── Theme Toggle ───────────── */
.theme-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: 1px solid var(--clr-border);
  background: transparent;
  color: var(--clr-text-secondary);
  cursor: pointer;
  transition: all 0.2s;
}

.theme-toggle:hover {
  color: var(--clr-text);
  border-color: var(--clr-border-strong);
  background: var(--clr-raised);
}

.nav-cta {
  padding: 8px 20px !important;
  background: var(--clr-accent) !important;
  color: #0a0e14 !important;
  border-radius: var(--radius-md);
  font-weight: 600 !important;
  font-size: 0.8125rem !important;
  transition: all 0.25s ease !important;
}

.nav-cta:hover {
  background: var(--clr-accent-hover) !important;
  box-shadow: var(--shadow-glow) !important;
  color: #0a0e14 !important;
}

/* ── Mobile toggle ──────────── */
.nav-toggle {
  display: none;
  flex-direction: column;
  gap: 5px;
  background: none;
  border: none;
  cursor: pointer;
  padding: 4px;
}

.nav-toggle span {
  display: block;
  width: 20px;
  height: 2px;
  background: var(--clr-text);
  border-radius: 2px;
  transition: all 0.3s ease;
}

.nav-toggle span.open:nth-child(1) {
  transform: translateY(7px) rotate(45deg);
}
.nav-toggle span.open:nth-child(2) {
  opacity: 0;
}
.nav-toggle span.open:nth-child(3) {
  transform: translateY(-7px) rotate(-45deg);
}

/* ── Mobile menu ────────────── */
.nav-mobile {
  display: none;
  flex-direction: column;
  gap: 4px;
  padding: 16px 28px;
  background: rgba(10, 14, 20, 0.96);
  backdrop-filter: blur(16px);
  border-top: 1px solid var(--clr-border);
}

.nav-mobile.open {
  display: flex;
}

.nav-mobile a {
  font-size: 0.9375rem;
  font-weight: 500;
  color: var(--clr-text-secondary);
  padding: 10px 0;
  transition: color 0.2s;
}

.nav-mobile a:hover {
  color: var(--clr-text);
}

.nav-cta-mobile {
  color: var(--clr-accent) !important;
  font-weight: 600 !important;
}

@media (max-width: 768px) {
  .nav-inner { padding: 0 16px; }
  .nav-links { display: none; }
  .nav-toggle { display: flex; }
  .nav-name { font-size: 0.875rem; }
}
</style>
