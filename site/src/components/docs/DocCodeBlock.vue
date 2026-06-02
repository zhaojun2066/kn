<script setup lang="ts">
import { ref } from 'vue'

const props = defineProps<{
  lang?: string
  code: string
}>()

const copied = ref(false)

async function copy() {
  try {
    await navigator.clipboard.writeText(props.code)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = props.code
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  }
}
</script>

<template>
  <div class="code-block-wrapper">
    <div class="code-block-header">
      <span class="code-lang">{{ lang || 'bash' }}</span>
      <button class="code-copy-btn" @click="copy">
        {{ copied ? '✓ 已复制' : '复制' }}
      </button>
    </div>
    <pre class="code-block-body"><code>{{ code }}</code></pre>
  </div>
</template>

<style scoped>
.code-block-wrapper {
  background: #0d1117;
  border: 1px solid var(--clr-border);
  border-radius: var(--radius-md);
  overflow: hidden;
  margin: 16px 0;
}

.code-block-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 14px;
  background: #161b22;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
}

.code-lang {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  color: var(--clr-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.code-copy-btn {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 0.6875rem;
  font-family: var(--font-body);
  color: var(--clr-text-muted);
  background: rgba(255,255,255,0.04);
  border: 1px solid var(--clr-border);
  cursor: pointer;
  transition: all 0.2s;
}

.code-copy-btn:hover {
  color: var(--clr-text);
  border-color: var(--clr-border-strong);
}

.code-block-body {
  padding: 14px 18px;
  margin: 0;
  font-family: var(--font-mono);
  font-size: 0.8rem;
  line-height: 1.8;
  color: #c9d1d9;
  overflow-x: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
