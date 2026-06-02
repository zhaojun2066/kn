<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { docGroups, docPages } from '../../data/docs'

const props = defineProps<{
  currentPage: string
  mobileOpen: boolean
}>()

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'navigate'): void
}>()

const router = useRouter()
const searchQuery = ref('')
const collapsedGroups = ref<Set<string>>(new Set())

function toggleGroup(groupId: string) {
  if (collapsedGroups.value.has(groupId)) {
    collapsedGroups.value.delete(groupId)
  } else {
    collapsedGroups.value.add(groupId)
  }
}

function navigateTo(pageId: string) {
  router.push(`/docs/${pageId}`)
  emit('navigate')
}

const filteredGroups = computed(() => {
  if (!searchQuery.value.trim()) return docGroups

  const q = searchQuery.value.toLowerCase()
  return docGroups
    .map((g) => ({
      ...g,
      pages: g.pages.filter((pid) => {
        const page = docPages[pid]
        if (!page) return false
        return (
          page.title.toLowerCase().includes(q) ||
          page.content.toLowerCase().includes(q)
        )
      }),
    }))
    .filter((g) => g.pages.length > 0)
})

const isActive = (pageId: string) => props.currentPage === pageId
</script>

<template>
  <aside class="sidebar" :class="{ 'sidebar-open': mobileOpen }">
    <!-- Search -->
    <div class="sidebar-search">
      <input
        v-model="searchQuery"
        type="text"
        placeholder="搜索文档..."
        class="search-input"
      />
    </div>

    <!-- Menu -->
    <nav class="sidebar-nav">
      <div
        v-for="group in filteredGroups"
        :key="group.id"
        class="nav-group"
      >
        <button
          class="nav-group-header"
          @click="toggleGroup(group.id)"
        >
          <span class="group-icon">{{ group.icon }}</span>
          <span class="group-label">{{ group.label }}</span>
          <svg
            class="group-chevron"
            :class="{ rotated: !collapsedGroups.has(group.id) }"
            width="12" height="12" viewBox="0 0 24 24"
            fill="none" stroke="currentColor" stroke-width="2"
            stroke-linecap="round"
          >
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </button>
        <div
          v-show="!collapsedGroups.has(group.id)"
          class="nav-group-pages"
        >
          <button
            v-for="pageId in group.pages"
            :key="pageId"
            class="nav-page"
            :class="{ active: isActive(pageId) }"
            @click="navigateTo(pageId)"
          >
            {{ docPages[pageId]?.title }}
          </button>
        </div>
      </div>
    </nav>

    <!-- No results -->
    <div v-if="filteredGroups.length === 0" class="sidebar-empty">
      未找到相关文档
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 260px;
  min-width: 260px;
  background: #0d1117;
  border-right: 1px solid var(--clr-border);
  display: flex;
  flex-direction: column;
  height: calc(100vh - 52px);
  position: sticky;
  top: 52px;
}

/* Search */
.sidebar-search {
  padding: 12px 16px;
  border-bottom: 1px solid var(--clr-border);
}

.search-input {
  width: 100%;
  padding: 7px 12px;
  background: #14191f;
  border: 1px solid var(--clr-border);
  border-radius: 6px;
  font-family: var(--font-body);
  font-size: 0.8125rem;
  color: var(--clr-text);
  outline: none;
  transition: border-color 0.2s;
}

.search-input::placeholder {
  color: var(--clr-text-muted);
}

.search-input:focus {
  border-color: var(--clr-border-strong);
}

/* Nav */
.sidebar-nav {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

.nav-group {
  padding: 0;
}

.nav-group-header {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 16px;
  background: none;
  border: none;
  cursor: pointer;
  font-family: var(--font-body);
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--clr-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  transition: color 0.2s;
  margin-top: 8px;
}

.nav-group-header:hover {
  color: var(--clr-text-secondary);
}

.group-chevron {
  margin-left: auto;
  transition: transform 0.2s ease;
  color: var(--clr-text-muted);
}

.group-chevron.rotated {
  transform: rotate(90deg);
}

.nav-group-pages {
  padding: 2px 0;
}

.nav-page {
  display: block;
  width: 100%;
  padding: 4px 16px 4px 32px;
  background: none;
  border: none;
  border-left: 2px solid transparent;
  cursor: pointer;
  font-family: var(--font-body);
  font-size: 0.8125rem;
  color: var(--clr-text-secondary);
  text-align: left;
  transition: all 0.15s ease;
  line-height: 1.8;
}

.nav-page:hover {
  color: var(--clr-text);
}

.nav-page.active {
  color: var(--clr-accent);
  background: var(--clr-accent-dim);
  border-left-color: var(--clr-accent);
}

.sidebar-empty {
  padding: 24px 16px;
  font-size: 0.875rem;
  color: var(--clr-text-muted);
  text-align: center;
}

/* Mobile: overlay drawer */
@media (max-width: 768px) {
  .sidebar {
    position: fixed;
    top: 52px;
    left: 0;
    bottom: 0;
    z-index: 200;
    transform: translateX(-100%);
    transition: transform 0.3s ease;
    background: var(--clr-bg-elevated);
  }

  .sidebar.sidebar-open {
    transform: translateX(0);
    box-shadow: var(--shadow-lg);
  }
}
</style>
