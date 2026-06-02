<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'

const props = defineProps<{
  content: string
}>()

interface TocItem {
  id: string
  text: string
  level: number
}

const headings = computed<TocItem[]>(() => {
  const items: TocItem[] = []
  const lines = props.content.split('\n')
  for (const line of lines) {
    const h2 = line.match(/^## (.+)/)
    if (h2) {
      const id = h2[1].toLowerCase().replace(/[^a-z0-9一-鿿]+/g, '-').replace(/^-|-$/g, '')
      items.push({ id, text: h2[1], level: 2 })
      continue
    }
    const h3 = line.match(/^### (.+)/)
    if (h3) {
      const id = h3[1].toLowerCase().replace(/[^a-z0-9一-鿿]+/g, '-').replace(/^-|-$/g, '')
      items.push({ id, text: h3[1], level: 3 })
    }
  }
  return items
})

const activeId = ref<string | null>(null)
let observer: IntersectionObserver | null = null

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          activeId.value = entry.target.id
        }
      }
    },
    { rootMargin: '-80px 0px -60% 0px' }
  )

  for (const h of headings.value) {
    const el = document.getElementById(h.id)
    if (el) observer.observe(el)
  }
})

onUnmounted(() => {
  observer?.disconnect()
})

function scrollTo(id: string) {
  const el = document.getElementById(id)
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' })
    activeId.value = id
  }
}
</script>

<template>
  <nav v-if="headings.length > 0" class="toc">
    <div class="toc-title">本页目录</div>
    <ul class="toc-list">
      <li
        v-for="h in headings"
        :key="h.id"
        class="toc-item"
        :class="{
          'toc-h3': h.level === 3,
          'toc-active': activeId === h.id,
        }"
      >
        <a
          :href="'#' + h.id"
          class="toc-link"
          @click.prevent="scrollTo(h.id)"
        >
          {{ h.text }}
        </a>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.toc {
  width: 180px;
  min-width: 180px;
  position: sticky;
  top: 80px;
  align-self: flex-start;
  max-height: calc(100vh - 120px);
  overflow-y: auto;
  padding-left: 0;
}

.toc-title {
  font-family: var(--font-body);
  font-size: 0.6875rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--clr-text-muted);
  margin-bottom: 12px;
}

.toc-list {
  list-style: none;
  padding: 0;
  margin: 0;
}

.toc-item {
  margin-bottom: 2px;
}

.toc-link {
  display: block;
  padding: 4px 0;
  font-size: 0.8125rem;
  color: var(--clr-text-muted);
  text-decoration: none;
  line-height: 1.5;
  transition: color 0.2s;
  border-left: 2px solid transparent;
  padding-left: 10px;
}

.toc-link:hover {
  color: var(--clr-text);
}

.toc-active .toc-link {
  color: var(--clr-accent);
  border-left-color: var(--clr-accent);
}

.toc-h3 .toc-link {
  padding-left: 22px;
  font-size: 0.75rem;
}

@media (max-width: 1280px) {
  .toc {
    display: none;
  }
}
</style>
