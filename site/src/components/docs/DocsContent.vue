<script setup lang="ts">
import { computed } from 'vue'
import DocCodeBlock from './DocCodeBlock.vue'
import DocCallout from './DocCallout.vue'
import { docPages } from '../../data/docs'
import type { DocPage } from '../../types/docs'

const props = defineProps<{
  page: DocPage
}>()

const GROUP_LABELS: Record<string, string> = {
  'getting-started': '入门指南',
  'cli-reference': 'CLI 使用',
  'desktop': 'Desktop 应用',
  'scenarios': '场景示例',
  'more': '更多',
}

// ── Simple markdown parser ──────
interface ParsedNode {
  type: string
  [key: string]: any
}

function parseMarkdown(md: string): ParsedNode[] {
  const nodes: ParsedNode[] = []
  const lines = md.split('\n')
  let i = 0

  while (i < lines.length) {
    const line = lines[i]

    // Code fence
    if (line.startsWith('```')) {
      const lang = line.slice(3).trim()
      const codeLines: string[] = []
      i++
      while (i < lines.length && !lines[i].startsWith('```')) {
        codeLines.push(lines[i])
        i++
      }
      i++
      nodes.push({ type: 'code', lang: lang || undefined, code: codeLines.join('\n') })
      continue
    }

    // Callout :::tip / :::info / :::warning
    const calloutMatch = line.match(/^:::(tip|info|warning)/)
    if (calloutMatch) {
      const calloutType = calloutMatch[1]
      const calloutLines: string[] = []
      i++
      while (i < lines.length && !lines[i].startsWith(':::')) {
        calloutLines.push(lines[i])
        i++
      }
      i++
      nodes.push({ type: 'callout', calloutType, content: calloutLines.join('\n') })
      continue
    }

    // Heading
    const h2Match = line.match(/^## (.+)/)
    if (h2Match) {
      const id = h2Match[1].toLowerCase().replace(/[^a-z0-9一-鿿]+/g, '-').replace(/^-|-$/g, '')
      nodes.push({ type: 'h2', id, text: h2Match[1] })
      i++
      continue
    }

    const h3Match = line.match(/^### (.+)/)
    if (h3Match) {
      const id = h3Match[1].toLowerCase().replace(/[^a-z0-9一-鿿]+/g, '-').replace(/^-|-$/g, '')
      nodes.push({ type: 'h3', id, text: h3Match[1] })
      i++
      continue
    }

    // Table
    if (line.startsWith('|')) {
      const tableLines: string[] = []
      while (i < lines.length && lines[i].startsWith('|')) {
        tableLines.push(lines[i])
        i++
      }
      const rows = tableLines
        .filter((l, idx) => idx !== 1 || !l.match(/^\|[\s\-:|]+\|$/))
        .map(r => r.split('|').filter(c => c !== '').map(c => c.trim()))
      nodes.push({ type: 'table', rows })
      continue
    }

    // Ordered list
    const olMatch = line.match(/^(\d+)\. (.+)/)
    if (olMatch) {
      const items: string[] = []
      while (i < lines.length) {
        const m = lines[i].match(/^(\d+)\. (.+)/)
        if (!m) break
        items.push(m[2])
        i++
      }
      nodes.push({ type: 'ol', items })
      continue
    }

    // Unordered list
    const ulMatch = line.match(/^[-*] (.+)/)
    if (ulMatch) {
      const items: string[] = []
      while (i < lines.length) {
        const m = lines[i].match(/^[-*] (.+)/)
        if (!m) break
        items.push(m[1])
        i++
      }
      nodes.push({ type: 'ul', items })
      continue
    }

    // Blockquote
    if (line.startsWith('> ')) {
      const quoteLines: string[] = []
      while (i < lines.length && lines[i].startsWith('> ')) {
        quoteLines.push(lines[i].slice(2))
        i++
      }
      nodes.push({ type: 'blockquote', text: quoteLines.join('\n') })
      continue
    }

    // Empty line
    if (line.trim() === '') {
      i++
      continue
    }

    // Paragraph
    const paraLines: string[] = []
    while (
      i < lines.length &&
      lines[i].trim() !== '' &&
      !lines[i].startsWith('```') &&
      !lines[i].startsWith(':::') &&
      !lines[i].startsWith('##') &&
      !lines[i].startsWith('|') &&
      !lines[i].startsWith('>') &&
      !lines[i].match(/^[-*\d]\. /)
    ) {
      paraLines.push(lines[i])
      i++
    }
    if (paraLines.length > 0) {
      nodes.push({ type: 'p', text: paraLines.join(' ') })
    }
  }

  return nodes
}

const parsedNodes = computed(() => parseMarkdown(props.page.content))

function renderInline(text: string): string {
  let html = text.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>')
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  return html
}
</script>

<template>
  <div class="docs-content">
    <!-- Breadcrumb -->
    <div class="breadcrumb">
      <span class="breadcrumb-group">{{ page.groupIcon }} {{ GROUP_LABELS[page.group] || page.group }}</span>
      <span class="breadcrumb-sep">/</span>
      <span class="breadcrumb-current">{{ page.title }}</span>
    </div>

    <!-- Title -->
    <h1 class="content-title">{{ page.title }}</h1>

    <!-- Body -->
    <div class="content-body">
      <template v-for="(node, idx) in parsedNodes" :key="idx">
        <!-- Code block -->
        <DocCodeBlock v-if="node.type === 'code'" :lang="node.lang" :code="node.code" />

        <!-- Callout -->
        <DocCallout v-else-if="node.type === 'callout'" :type="node.calloutType">
          <div v-html="renderInline(node.content)" />
        </DocCallout>

        <!-- Heading 2 -->
        <h2 v-else-if="node.type === 'h2'" :id="node.id" class="content-h2">
          <a :href="'#' + node.id" class="heading-anchor">#</a>
          {{ node.text }}
        </h2>

        <!-- Heading 3 -->
        <h3 v-else-if="node.type === 'h3'" :id="node.id" class="content-h3">
          <a :href="'#' + node.id" class="heading-anchor">#</a>
          {{ node.text }}
        </h3>

        <!-- Table -->
        <div v-else-if="node.type === 'table'" class="content-table-wrap">
          <table class="content-table">
            <thead v-if="node.rows.length > 0">
              <tr>
                <th v-for="(cell, ci) in node.rows[0]" :key="ci">{{ cell }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(row, ri) in node.rows.slice(1)" :key="ri">
                <td v-for="(cell, ci) in row" :key="ci" v-html="renderInline(cell)" />
              </tr>
            </tbody>
          </table>
        </div>

        <!-- Ordered list -->
        <ol v-else-if="node.type === 'ol'" class="content-ol">
          <li v-for="(item, li) in node.items" :key="li" v-html="renderInline(item)" />
        </ol>

        <!-- Unordered list -->
        <ul v-else-if="node.type === 'ul'" class="content-ul">
          <li v-for="(item, li) in node.items" :key="li" v-html="renderInline(item)" />
        </ul>

        <!-- Blockquote -->
        <blockquote v-else-if="node.type === 'blockquote'" class="content-blockquote">
          <p v-html="renderInline(node.text)" />
        </blockquote>

        <!-- Paragraph -->
        <p v-else-if="node.type === 'p'" class="content-p" v-html="renderInline(node.text)" />
      </template>
    </div>

    <!-- Prev/Next navigation -->
    <nav class="content-footer-nav">
      <router-link v-if="page.prev" :to="`/docs/${page.prev}`" class="footer-nav-link prev">
        ← {{ docPages[page.prev]?.title }}
      </router-link>
      <span v-else class="footer-nav-link prev placeholder"></span>

      <router-link v-if="page.next" :to="`/docs/${page.next}`" class="footer-nav-link next">
        {{ docPages[page.next]?.title }} →
      </router-link>
      <span v-else class="footer-nav-link next placeholder"></span>
    </nav>
  </div>
</template>

<style scoped>
.docs-content {
  flex: 1;
  min-width: 0;
  padding: 40px 48px;
  max-width: 760px;
}

/* Breadcrumb */
.breadcrumb {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.75rem;
  color: var(--clr-text-muted);
  margin-bottom: 12px;
}

.breadcrumb-sep {
  color: var(--clr-text-dim);
}

.breadcrumb-current {
  color: var(--clr-text-secondary);
}

/* Title */
.content-title {
  font-family: var(--font-display);
  font-size: 1.75rem;
  font-weight: 700;
  color: var(--clr-text);
  letter-spacing: -0.02em;
  margin-bottom: 28px;
}

/* Headings */
.content-h2 {
  font-family: var(--font-display);
  font-size: 1.25rem;
  font-weight: 600;
  color: var(--clr-text);
  margin: 32px 0 12px;
  padding-top: 8px;
  border-top: 1px solid var(--clr-border);
}

.content-h3 {
  font-family: var(--font-display);
  font-size: 1.0625rem;
  font-weight: 600;
  color: var(--clr-text);
  margin: 24px 0 8px;
}

.heading-anchor {
  opacity: 0;
  color: var(--clr-text-muted);
  font-size: 0.875rem;
  margin-right: 6px;
  transition: opacity 0.2s;
  text-decoration: none;
}

.content-h2:hover .heading-anchor,
.content-h3:hover .heading-anchor {
  opacity: 1;
}

/* Paragraph */
.content-p {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.75;
  margin: 10px 0;
}

/* Inline code */
:deep(.inline-code) {
  font-family: var(--font-mono);
  font-size: 0.8rem;
  background: var(--clr-raised);
  padding: 1px 6px;
  border-radius: 4px;
  border: 1px solid var(--clr-border);
  color: var(--clr-accent);
}

/* Lists */
.content-ol,
.content-ul {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.9;
  padding-left: 22px;
  margin: 8px 0;
}

/* Table */
.content-table-wrap {
  overflow-x: auto;
  margin: 16px 0;
}

.content-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.875rem;
}

.content-table th {
  padding: 10px 14px;
  text-align: left;
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  font-weight: 500;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--clr-text-muted);
  border-bottom: 1px solid var(--clr-border);
  background: var(--clr-raised);
}

.content-table td {
  padding: 9px 14px;
  color: var(--clr-text-secondary);
  border-bottom: 1px solid var(--clr-border);
}

.content-table tr:hover td {
  background: rgba(255, 255, 255, 0.015);
}

/* Blockquote */
.content-blockquote {
  margin: 16px 0;
  padding: 12px 16px;
  border-left: 3px solid var(--clr-border-strong);
  background: var(--clr-raised);
  border-radius: 0 6px 6px 0;
}

.content-blockquote p {
  font-size: 0.875rem;
  color: var(--clr-text-secondary);
  line-height: 1.7;
}

/* Footer nav */
.content-footer-nav {
  display: flex;
  justify-content: space-between;
  margin-top: 48px;
  padding-top: 20px;
  border-top: 1px solid var(--clr-border);
}

.footer-nav-link {
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--clr-accent);
  text-decoration: none;
  transition: color 0.2s;
}

.footer-nav-link:hover {
  color: var(--clr-accent-hover);
}

.footer-nav-link.placeholder {
  visibility: hidden;
}

/* Responsive */
@media (max-width: 768px) {
  .docs-content {
    padding: 24px 16px;
  }

  .content-title {
    font-size: 1.5rem;
  }
}
</style>
