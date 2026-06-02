<script setup lang="ts">
defineProps<{
  type?: 'info' | 'tip' | 'warning'
}>()
</script>

<template>
  <div class="callout" :class="`callout-${type || 'info'}`">
    <div class="callout-content">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.callout {
  margin: 16px 0;
  padding: 12px 16px;
  border-radius: 0 6px 6px 0;
  font-size: 0.875rem;
  line-height: 1.7;
}

.callout-info {
  background: var(--clr-accent-dim);
  border-left: 3px solid var(--clr-accent);
  color: var(--clr-text-secondary);
}

.callout-tip {
  background: var(--clr-green-bg);
  border-left: 3px solid var(--clr-green);
  color: var(--clr-text-secondary);
}

.callout-warning {
  background: var(--clr-red-bg);
  border-left: 3px solid var(--clr-red);
  color: var(--clr-text-secondary);
}

.callout-content :deep(strong) {
  color: var(--clr-text);
}
</style>
