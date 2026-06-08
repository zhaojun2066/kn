<script setup lang="ts">
import { APP_NAME, APP_VERSION } from '../config'

function scrollToDownload() {
  const el = document.getElementById('download');
  if (el) {
    el.scrollIntoView({ behavior: 'smooth' });
  } else {
    window.location.hash = '#/';
    setTimeout(() => {
      const el = document.getElementById('download');
      if (el) el.scrollIntoView({ behavior: 'smooth' });
    }, 100);
  }
}
</script>

<template>
  <footer class="footer">
    <div class="section-divider"></div>
    <div class="footer-inner">
      <div class="footer-left">
        <div class="footer-brand">
          <img src="/icon-128.png" alt="" class="footer-icon" />
          <span>{{ APP_NAME }}</span>
        </div>
        <span class="footer-ver">v{{ APP_VERSION }}</span>
      </div>

      <div class="footer-links">
        <a href="#/">特性</a>
        <a href="#/docs">文档</a>
        <a href="#/" @click.prevent="scrollToDownload">下载</a>
      </div>

      <div class="footer-right">
        <p class="footer-copy">&copy; {{ new Date().getFullYear() }} AI Profile Manager</p>
      </div>
    </div>
  </footer>
</template>

<style scoped>
.footer {
  padding: 0 0 48px;
}

.footer-inner {
  max-width: var(--max-w);
  margin: 0 auto;
  padding: 40px 28px 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 16px;
}

.footer-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.footer-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: var(--font-display);
  font-weight: 600;
  font-size: 0.9375rem;
  color: var(--clr-text);
}

.footer-icon {
  width: 20px;
  height: 20px;
  border-radius: 4px;
}

.footer-ver {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--clr-text-muted);
  padding-left: 12px;
  border-left: 1px solid var(--clr-border);
}

.footer-links {
  display: flex;
  gap: 24px;
}

.footer-links a {
  font-size: 0.8125rem;
  color: var(--clr-text-secondary);
  transition: color 0.2s;
}

.footer-links a:hover {
  color: var(--clr-text);
}

.footer-copy {
  font-size: 0.75rem;
  color: var(--clr-text-muted);
}

@media (max-width: 768px) {
  .footer-inner {
    flex-direction: column;
    text-align: center;
    padding: 32px 16px 0;
  }
  .footer-left {
    flex-direction: column;
    gap: 6px;
  }
  .footer-ver {
    border-left: none;
    padding-left: 0;
  }
}
</style>
