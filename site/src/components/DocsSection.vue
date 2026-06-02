<script setup lang="ts">
import { ref } from 'vue'

type Tab = 'quickstart' | 'reference' | 'faq'

const activeTab = ref<Tab>('quickstart')

const tabs: { key: Tab; label: string; icon: string }[] = [
  { key: 'quickstart', label: '快速上手', icon: '🚀' },
  { key: 'reference', label: '命令参考', icon: '📋' },
  { key: 'faq', label: '常见问题', icon: '💡' },
]

// ── Quick Start ────────────────────
const quickStartSteps = [
  {
    title: '1. 安装',
    desc: '克隆仓库，执行安装脚本，自动配置 Shell 环境。',
    code: `cd ~/workspace/me/shark/kn
bash install.sh
source ~/.zshrc`,
  },
  {
    title: '2. 创建 Profile',
    desc: '交互式引导，填入 API Key、Base URL 和模型配置。',
    code: `profile add deepseek -i`,
  },
  {
    title: '3. 启动 Claude Code',
    desc: '一行命令，自动注入环境变量，退出自动清除。',
    code: `ai claude deepseek`,
  },
]

// ── Command Reference ──────────────
const commands = [
  { cmd: 'profile list', desc: '列出所有 profile' },
  { cmd: 'profile show <name>', desc: '查看 profile 详情（key 自动打码）' },
  { cmd: 'profile add <name> [desc]', desc: '新增 profile，-i 为交互式' },
  { cmd: 'profile remove <name>', desc: '删除 profile' },
  { cmd: 'profile set <name> <K>=<V>', desc: '设置环境变量' },
  { cmd: 'profile unset <name> <K>', desc: '删除环境变量' },
  { cmd: 'profile default [name]', desc: '查看/设置默认 profile' },
  { cmd: 'profile env <name>', desc: '输出 env 变量（shell eval 格式）' },
  { cmd: 'profile names', desc: '输出 profile 名列表（供 fzf 使用）' },
  { cmd: 'profile init', desc: '从 ~/.claude/settings.json 导入' },
  { cmd: 'ai claude <profile>', desc: '用指定 profile 启动 Claude Code' },
  { cmd: 'ai codex <profile>', desc: '用指定 profile 启动 Codex' },
]

// ── FAQ ────────────────────────────
const faqs = [
  {
    q: '多个终端同时改 profile 会冲突吗？',
    a: '不会。写操作通过文件锁（fcntl.flock）保护，同时写入会排队等待，不会损坏数据。',
  },
  {
    q: 'API key 安全吗？',
    a: 'key 明文存储在 ~/.claude-profiles/config.yaml 中。建议确保目录权限为 700：chmod 700 ~/.claude-profiles',
  },
  {
    q: '怎么让 profile 对整个项目目录生效？',
    a: '可以使用 direnv。在项目根目录创建 .envrc，通过 profile env 命令获取环境变量并导出。',
  },
  {
    q: 'Desktop App 和 CLI 的数据如何同步？',
    a: '两者共用同一份 ~/.claude-profiles/config.yaml 文件，通过文件锁保证并发安全，任何一方的修改另一方立即可见。',
  },
  {
    q: '支持哪些 CLI 工具？',
    a: '目前完整支持 Claude Code 和 Codex CLI。profile 系统本身是通用的，可以管理任意环境变量组合。',
  },
]

// ── Copy to clipboard ─────────────
const copiedIdx = ref<number | null>(null)

async function copyCode(text: string, idx: number) {
  try {
    await navigator.clipboard.writeText(text)
    copiedIdx.value = idx
    setTimeout(() => { copiedIdx.value = null }, 2000)
  } catch {
    // fallback
    const ta = document.createElement('textarea')
    ta.value = text
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
    copiedIdx.value = idx
    setTimeout(() => { copiedIdx.value = null }, 2000)
  }
}

// FAQ accordion
const expandedFaq = ref<number | null>(null)

function toggleFaq(idx: number) {
  expandedFaq.value = expandedFaq.value === idx ? null : idx
}
</script>

<template>
  <section class="docs section" id="docs">
    <div class="container">
      <div class="docs-header reveal">
        <span class="section-label">文档</span>
        <h2 class="section-title">快速了解全部功能</h2>
      </div>

      <!-- Tabs -->
      <div class="docs-tabs reveal">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          class="docs-tab"
          :class="{ active: activeTab === tab.key }"
          @click="activeTab = tab.key"
        >
          <span class="tab-icon">{{ tab.icon }}</span>
          {{ tab.label }}
        </button>
      </div>

      <!-- ── Tab: Quick Start ────────── -->
      <div v-if="activeTab === 'quickstart'" class="docs-content">
        <div class="quickstart-grid">
          <div
            v-for="(step, idx) in quickStartSteps"
            :key="idx"
            class="qs-card glass-card reveal"
            :style="{ transitionDelay: `${idx * 100}ms` }"
          >
            <h3 class="qs-title">{{ step.title }}</h3>
            <p class="qs-desc">{{ step.desc }}</p>
            <div class="code-block qs-code">
              <button class="copy-btn" @click="copyCode(step.code, idx)">
                {{ copiedIdx === idx ? '✓ 已复制' : '复制' }}
              </button>
              <pre><code>{{ step.code }}</code></pre>
            </div>
          </div>
        </div>
      </div>

      <!-- ── Tab: Command Reference ──── -->
      <div v-if="activeTab === 'reference'" class="docs-content">
        <div class="ref-table glass-card reveal">
          <div class="ref-header">
            <span class="ref-col-cmd">命令</span>
            <span class="ref-col-desc">说明</span>
          </div>
          <div
            v-for="(item, idx) in commands"
            :key="idx"
            class="ref-row"
          >
            <code class="ref-cmd">{{ item.cmd }}</code>
            <span class="ref-desc">{{ item.desc }}</span>
          </div>
        </div>
      </div>

      <!-- ── Tab: FAQ ────────────────── -->
      <div v-if="activeTab === 'faq'" class="docs-content">
        <div class="faq-list">
          <div
            v-for="(item, idx) in faqs"
            :key="idx"
            class="faq-item glass-card reveal"
            :class="{ expanded: expandedFaq === idx }"
            :style="{ transitionDelay: `${idx * 60}ms` }"
            @click="toggleFaq(idx)"
          >
            <div class="faq-q">
              <span class="faq-q-text">{{ item.q }}</span>
              <span class="faq-arrow" :class="{ open: expandedFaq === idx }">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><polyline points="6 9 12 15 18 9"/></svg>
              </span>
            </div>
            <div class="faq-a" v-show="expandedFaq === idx">
              <p>{{ item.a }}</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.docs-header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 44px;
}

/* ── Tabs ─────────────────────── */
.docs-tabs {
  display: flex;
  gap: 4px;
  justify-content: center;
  margin-bottom: 44px;
  background: var(--clr-raised);
  padding: 4px;
  border-radius: var(--radius-md);
  border: 1px solid var(--clr-border);
  width: fit-content;
  margin-left: auto;
  margin-right: auto;
}

.docs-tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 9px 20px;
  border-radius: var(--radius-sm);
  font-family: var(--font-body);
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--clr-text-secondary);
  background: transparent;
  border: none;
  cursor: pointer;
  transition: all 0.2s ease;
}

.docs-tab:hover {
  color: var(--clr-text);
}

.docs-tab.active {
  background: var(--clr-surface);
  color: var(--clr-text);
  box-shadow: var(--shadow-sm);
}

.tab-icon {
  font-size: 0.875rem;
}

.docs-content {
  animation: fadeIn 0.3s ease-out;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}

/* ── Quick Start ──────────────── */
.quickstart-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 20px;
}

.qs-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 30px 26px;
}

.qs-title {
  font-family: var(--font-display);
  font-size: 1.0625rem;
  font-weight: 600;
  color: var(--clr-text);
}

.qs-desc {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.6;
}

.qs-code {
  margin-top: 6px;
  position: relative;
  padding: 14px 18px;
}

.qs-code pre {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  line-height: 1.8;
  color: #c9d1d9;
  white-space: pre-wrap;
  word-break: break-all;
}

.copy-btn {
  position: absolute;
  top: 8px;
  right: 10px;
  padding: 3px 10px;
  border-radius: var(--radius-sm);
  font-size: 0.6875rem;
  font-family: var(--font-body);
  color: var(--clr-text-muted);
  background: var(--clr-raised);
  border: 1px solid var(--clr-border);
  cursor: pointer;
  transition: all 0.2s;
  z-index: 1;
}

.copy-btn:hover {
  color: var(--clr-text);
  border-color: var(--clr-border-strong);
}

/* ── Command Reference ────────── */
.ref-table {
  max-width: 760px;
  margin: 0 auto;
  padding: 0;
  overflow: hidden;
}

.ref-header {
  display: flex;
  padding: 14px 24px;
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--clr-text-muted);
  border-bottom: 1px solid var(--clr-border);
  background: var(--clr-raised);
}

.ref-row {
  display: flex;
  padding: 13px 24px;
  border-bottom: 1px solid var(--clr-border);
  align-items: baseline;
  transition: background 0.15s;
}

.ref-row:last-child {
  border-bottom: none;
}

.ref-row:hover {
  background: rgba(255,255,255,0.015);
}

.ref-col-cmd { flex: 1; }
.ref-col-desc { flex: 1.2; }

.ref-cmd {
  flex: 1;
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  color: var(--clr-accent);
}

.ref-desc {
  flex: 1.2;
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
}

/* ── FAQ ───────────────────────── */
.faq-list {
  max-width: 700px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.faq-item {
  padding: 0;
  cursor: pointer;
  transition: border-color 0.3s;
}

.faq-item:hover {
  border-color: var(--clr-border-strong);
}

.faq-q {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 18px 24px;
  gap: 16px;
}

.faq-q-text {
  font-weight: 500;
  font-size: 0.9375rem;
  color: var(--clr-text);
}

.faq-arrow {
  color: var(--clr-text-muted);
  transition: transform 0.3s ease;
  flex-shrink: 0;
}

.faq-arrow.open {
  transform: rotate(180deg);
  color: var(--clr-accent);
}

.faq-a {
  padding: 0 24px 20px;
  animation: slideDown 0.25s ease-out;
}

.faq-a p {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.7;
}

@keyframes slideDown {
  from { opacity: 0; transform: translateY(-6px); }
  to { opacity: 1; transform: translateY(0); }
}

/* ── Responsive ───────────────── */
@media (max-width: 768px) {
  .quickstart-grid {
    grid-template-columns: 1fr;
  }
  .ref-table {
    max-width: 100%;
  }
  .ref-row, .ref-header {
    padding: 12px 16px;
  }
  .ref-cmd {
    font-size: 0.75rem;
  }
  .docs-tabs {
    gap: 2px;
    padding: 3px;
  }
  .docs-tab {
    padding: 8px 14px;
    font-size: 0.8125rem;
  }
}
</style>
