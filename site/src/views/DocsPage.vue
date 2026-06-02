<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useRoute } from 'vue-router'
import NavBar from '../components/NavBar.vue'
import SiteFooter from '../components/SiteFooter.vue'
import DocsSidebar from '../components/docs/DocsSidebar.vue'
import DocsContent from '../components/docs/DocsContent.vue'
import DocsTOC from '../components/docs/DocsTOC.vue'
import { docPages } from '../data/docs'

const route = useRoute()
const currentPageId = computed(() => (route.params.page as string) || 'introduction')
const page = computed(() => docPages[currentPageId.value] || null)

const mobileSidebarOpen = ref(false)
const mobileOverlay = ref(false)

function openMobileSidebar() {
  mobileSidebarOpen.value = true
  mobileOverlay.value = true
}

function closeMobileSidebar() {
  mobileSidebarOpen.value = false
  mobileOverlay.value = false
}

function handleNavigate() {
  closeMobileSidebar()
  window.scrollTo({ top: 0 })
}

// Scroll to top on page change
watch(currentPageId, () => {
  window.scrollTo({ top: 0 })
})
</script>

<template>
  <div class="docs-page">
    <NavBar />

    <div class="docs-layout">
      <!-- Mobile overlay -->
      <div
        v-if="mobileOverlay"
        class="mobile-overlay"
        @click="closeMobileSidebar"
      />

      <!-- Sidebar -->
      <DocsSidebar
        :current-page="currentPageId"
        :mobile-open="mobileSidebarOpen"
        @close="closeMobileSidebar"
        @navigate="handleNavigate"
      />

      <!-- Mobile sidebar toggle -->
      <button class="mobile-sidebar-toggle" @click="openMobileSidebar">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <line x1="3" y1="6" x2="21" y2="6" />
          <line x1="3" y1="12" x2="21" y2="12" />
          <line x1="3" y1="18" x2="21" y2="18" />
        </svg>
        <span>菜单</span>
      </button>

      <!-- Content area -->
      <div class="docs-main">
        <!-- 404 -->
        <div v-if="!page" class="docs-404">
          <h2>页面不存在</h2>
          <p>你找的文档页面不存在。</p>
          <router-link to="/docs/introduction">← 返回文档首页</router-link>
        </div>

        <template v-else>
          <DocsContent :page="page" />

          <!-- Right TOC -->
          <DocsTOC :content="page.content" />
        </template>
      </div>
    </div>

    <!-- Mobile bottom nav: prev/next -->
    <div v-if="page" class="mobile-bottom-nav">
      <router-link
        v-if="page.prev"
        :to="`/docs/${page.prev}`"
        class="bottom-nav-link"
      >
        ← {{ docPages[page.prev]?.title }}
      </router-link>
      <span v-else class="bottom-nav-link disabled"></span>

      <router-link
        v-if="page.next"
        :to="`/docs/${page.next}`"
        class="bottom-nav-link"
      >
        {{ docPages[page.next]?.title }} →
      </router-link>
      <span v-else class="bottom-nav-link disabled"></span>
    </div>

    <SiteFooter />
  </div>
</template>

<style scoped>
.docs-page {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}

.docs-layout {
  display: flex;
  flex: 1;
  padding-top: 52px; /* navbar height */
}

.docs-main {
  display: flex;
  flex: 1;
  min-width: 0;
  justify-content: center;
}

/* 404 */
.docs-404 {
  padding: 80px 40px;
  text-align: center;
}

.docs-404 h2 {
  font-family: var(--font-display);
  font-size: 1.5rem;
  color: var(--clr-text);
  margin-bottom: 8px;
}

.docs-404 p {
  color: var(--clr-text-secondary);
  margin-bottom: 24px;
}

.docs-404 a {
  color: var(--clr-accent);
  font-weight: 500;
}

/* Mobile toggle button */
.mobile-sidebar-toggle {
  display: none;
}

/* Mobile overlay */
.mobile-overlay {
  display: none;
}

/* Mobile bottom nav */
.mobile-bottom-nav {
  display: none;
}

@media (max-width: 768px) {
  .docs-layout {
    position: relative;
  }

  .mobile-sidebar-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    position: fixed;
    bottom: 64px;
    left: 16px;
    z-index: 150;
    padding: 8px 14px;
    background: var(--clr-accent);
    color: #0a0e14;
    border: none;
    border-radius: 100px;
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
    box-shadow: var(--shadow-md);
  }

  .mobile-overlay {
    display: block;
    position: fixed;
    inset: 0;
    z-index: 199;
    background: rgba(0, 0, 0, 0.5);
  }

  .mobile-bottom-nav {
    display: flex;
    justify-content: space-between;
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: 100;
    padding: 10px 16px;
    background: var(--clr-bg-elevated);
    border-top: 1px solid var(--clr-border);
  }

  .bottom-nav-link {
    font-size: 0.8125rem;
    font-weight: 500;
    color: var(--clr-accent);
    text-decoration: none;
  }

  .bottom-nav-link.disabled {
    visibility: hidden;
  }
}
</style>
