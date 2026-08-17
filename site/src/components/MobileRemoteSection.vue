<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'

type SceneType = 'device' | 'project' | 'terminal' | 'inbox'

interface Scene {
  id: SceneType
  label: string
  title: string
  desc: string
  stat: string
  accent: string
}

const scenes: Scene[] = [
  {
    id: 'device',
    label: '设备',
    title: '绑定电脑端，确认 Agent 状态',
    desc: '扫码或输入 6 位码完成绑定，查看电脑端在线状态、Agent 版本和是否可启动远程会话。',
    stat: '2 台电脑端在线',
    accent: '#4ade80',
  },
  {
    id: 'project',
    label: '项目',
    title: '按电脑端浏览项目工作区',
    desc: '项目按设备分组，直接打开远程终端、查看会话历史，把当前项目上下文发送给 AI。',
    stat: '8 个项目可启动',
    accent: '#00c6ff',
  },
  {
    id: 'terminal',
    label: '终端',
    title: '手机上的真实远程 PTY',
    desc: 'xterm.js 渲染电脑端输出，多会话 Tab 切换，支持输入、快捷键、命令面板和语音输入。',
    stat: '3 个会话运行中',
    accent: '#f0c674',
  },
  {
    id: 'inbox',
    label: '消息',
    title: '工作结果和需关注事件',
    desc: '消息中心聚合构建、测试、提交、PR、AI 回复和电脑端状态，快速回到需要处理的工作。',
    stat: '5 条未读消息',
    accent: '#f87171',
  },
]

const current = ref(0)
const progress = ref(0)
let timer: ReturnType<typeof setInterval> | null = null
let progressTimer: ReturnType<typeof setInterval> | null = null

const scene = computed(() => scenes[current.value])

function selectScene(index: number) {
  current.value = index
  progress.value = 0
}

function nextScene() {
  current.value = (current.value + 1) % scenes.length
  progress.value = 0
}

onMounted(() => {
  timer = setInterval(nextScene, 5200)
  progressTimer = setInterval(() => {
    progress.value = progress.value >= 100 ? 0 : progress.value + 100 / 104
  }, 50)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
  if (progressTimer) clearInterval(progressTimer)
})
</script>

<template>
  <section class="mobile section" id="mobile">
    <div class="container">
      <div class="mobile-layout">
        <div class="mobile-copy reveal">
          <span class="section-label">手机 App 遥控器</span>
          <h2 class="section-title">手机远程遥控</h2>
          <p class="section-desc">
            KN 手机 App 不是缩小版开发工具，而是电脑端 Claude Code / Codex / Qoder 的遥控器：绑定设备、选择项目、启动终端、跟进结果，全都围绕远程 AI 工作流。
          </p>

          <div class="mobile-highlights">
            <button
              v-for="(item, index) in scenes"
              :key="item.id"
              class="mobile-highlight"
              :class="{ active: index === current }"
              type="button"
              @click="selectScene(index)"
            >
              <span class="highlight-index">0{{ index + 1 }}</span>
              <span class="highlight-text">
                <strong>{{ item.label }}</strong>
                <span>{{ item.title }}</span>
              </span>
            </button>
          </div>

          <div class="mobile-flow" aria-label="移动端核心流程">
            <span>绑定电脑端</span>
            <span class="flow-arrow">→</span>
            <span>选择项目</span>
            <span class="flow-arrow">→</span>
            <span>启动会话</span>
            <span class="flow-arrow">→</span>
            <span>接收结果</span>
          </div>
        </div>

        <div class="mobile-stage reveal">
          <div class="stage-backdrop"></div>

          <div class="phone-frame" :style="{ '--scene-accent': scene.accent }">
            <div class="phone-hardware-glow"></div>
            <div class="phone-notch"></div>
            <div class="phone-screen">
              <div class="phone-status">
                <span>9:41</span>
                <span class="status-icons">5G 100%</span>
              </div>

              <div class="app-topbar">
                <div>
                  <span class="app-kicker">KN App</span>
                  <h3>{{ scene.label }}</h3>
                </div>
                <span class="live-pill">LIVE</span>
              </div>

              <div class="screen-stack">
                <transition name="screen-fade" mode="out-in">
                  <div v-if="scene.id === 'device'" key="device" class="screen-content">
                    <div class="hero-mini">
                      <span class="mini-icon">⌁</span>
                      <div>
                        <strong>开发电脑</strong>
                        <span>Agent 1.0.7 · 在线 · 可启动</span>
                      </div>
                    </div>
                    <div class="bind-card">
                      <div class="qr-grid" aria-hidden="true">
                        <i v-for="n in 49" :key="n" :class="{ filled: [1,2,3,7,8,9,10,13,15,18,20,22,23,25,28,31,34,36,38,40,43,44,47,49].includes(n) }"></i>
                      </div>
                      <div class="bind-copy">
                        <span>绑定码</span>
                        <strong>492 817</strong>
                        <small>手机确认后建立安全连接</small>
                      </div>
                    </div>
                    <div class="device-list">
                      <div class="device-row active">
                        <span class="device-dot"></span>
                        <div><strong>主力电脑</strong><span>8 个项目 · 3 个 profile</span></div>
                      </div>
                      <div class="device-row">
                        <span class="device-dot idle"></span>
                        <div><strong>办公电脑</strong><span>最近在线 2 分钟前</span></div>
                      </div>
                    </div>
                  </div>

                  <div v-else-if="scene.id === 'project'" key="project" class="screen-content">
                    <div class="search-pill">搜索项目或电脑端</div>
                    <div class="project-summary">
                      <div><strong>8</strong><span>项目</span></div>
                      <div><strong>3</strong><span>运行中</span></div>
                      <div><strong>5</strong><span>待处理</span></div>
                    </div>
                    <div class="project-card featured">
                      <div class="folder-icon"></div>
                      <div>
                        <strong>kn-ios</strong>
                        <span>~/workspace/me/shark/kn-ios</span>
                        <small>default · Claude Code</small>
                      </div>
                      <b>运行</b>
                    </div>
                    <div class="workspace-actions">
                      <span>终端</span>
                      <span>会话</span>
                      <span>发送给 AI</span>
                    </div>
                    <div class="status-feed">
                      <div><span class="pulse"></span>项目状态已刷新</div>
                      <div><span class="pulse caution"></span>2 个工作项需关注</div>
                    </div>
                  </div>

                  <div v-else-if="scene.id === 'terminal'" key="terminal" class="screen-content terminal-screen">
                    <div class="session-tabs">
                      <span class="selected">kn-ios</span>
                      <span>kn-cloud</span>
                      <span>site</span>
                    </div>
                    <div class="terminal-window">
                      <span class="term-line dim">$ ai claude default</span>
                      <span class="term-line ok">✓ profile default injected</span>
                      <span class="term-line">⏺ Read App/KnApp.swift</span>
                      <span class="term-line">⏺ Bash xcodebuild test</span>
                      <span class="term-line warn">⚠ 1 snapshot needs review</span>
                      <span class="term-line ok">✓ 42 tests passed</span>
                      <span class="term-line cursor">❯ 总结这次改动</span>
                    </div>
                    <div class="terminal-tools">
                      <span>ESC</span>
                      <span>/</span>
                      <span>Tab</span>
                      <span>语音</span>
                    </div>
                    <div class="input-bar">输入 prompt 或命令...</div>
                  </div>

                  <div v-else key="inbox" class="screen-content">
                    <div class="message-tabs">
                      <span class="active">需关注 2</span>
                      <span>工作结果</span>
                      <span>电脑端</span>
                    </div>
                    <div class="message-list">
                      <div class="message-row danger">
                        <i></i>
                        <div>
                          <span>构建验证</span>
                          <strong>kn-ios snapshot 需要确认</strong>
                          <small>项目 kn-ios · 2 分钟前</small>
                        </div>
                      </div>
                      <div class="message-row success">
                        <i></i>
                        <div>
                          <span>工作结果</span>
                          <strong>PR 草稿已生成</strong>
                          <small>Token 18,420 · 耗时 7 分钟</small>
                        </div>
                      </div>
                      <div class="message-row">
                        <i></i>
                        <div>
                          <span>电脑端状态</span>
                          <strong>主力电脑在线，3 个会话运行中</strong>
                          <small>Agent 1.0.7 · 刚刚同步</small>
                        </div>
                      </div>
                    </div>
                    <div class="message-summary">
                      <div>
                        <strong>今日远程工作</strong>
                        <span>3 个会话 · 1 项需关注 · 2 项已完成</span>
                      </div>
                    </div>
                  </div>
                </transition>
              </div>

              <div class="app-tabbar">
                <span :class="{ active: scene.id === 'terminal' }">终端</span>
                <span :class="{ active: scene.id === 'project' }">项目</span>
                <span :class="{ active: scene.id === 'inbox' }">消息</span>
                <span :class="{ active: scene.id === 'device' }">我的</span>
              </div>
            </div>
          </div>

          <div class="scene-controls" aria-label="手机 App 演示场景">
            <button
              v-for="(item, index) in scenes"
              :key="item.id"
              type="button"
              class="scene-dot"
              :class="{ active: index === current }"
              @click="selectScene(index)"
            >
              <span :style="{ width: index === current ? progress + '%' : '0%' }"></span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.mobile {
  position: relative;
  overflow: hidden;
}

.mobile-layout {
  display: grid;
  grid-template-columns: 0.86fr 1.14fr;
  gap: 56px;
  align-items: center;
}

.mobile-copy {
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.mobile-copy .section-desc {
  max-width: 640px;
}

.mobile-highlights {
  display: grid;
  grid-template-columns: 1fr;
  gap: 10px;
  margin-top: 4px;
}

.mobile-highlight {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  min-height: 58px;
  padding: 12px 14px;
  border: 1px solid var(--clr-border);
  border-radius: var(--radius-md);
  background: var(--clr-surface);
  color: var(--clr-text-secondary);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.22s ease, background 0.22s ease, transform 0.22s ease;
}

.mobile-highlight:hover,
.mobile-highlight.active {
  border-color: var(--clr-border-accent);
  background: linear-gradient(90deg, var(--clr-accent-dim), var(--clr-surface));
  transform: translateX(4px);
}

.highlight-index {
  font-family: var(--font-mono);
  font-size: 0.75rem;
  color: var(--clr-accent);
}

.highlight-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.highlight-text strong {
  color: var(--clr-text);
  font-size: 0.875rem;
}

.highlight-text span {
  font-size: 0.8125rem;
  line-height: 1.45;
}

.mobile-flow {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding-top: 4px;
  color: var(--clr-text-muted);
  font-family: var(--font-mono);
  font-size: 0.75rem;
}

.flow-arrow {
  color: var(--clr-accent);
}

.mobile-stage {
  position: relative;
  min-height: 620px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.stage-backdrop {
  position: absolute;
  width: min(86%, 560px);
  height: 78%;
  border-radius: 42px;
  background:
    radial-gradient(circle at 50% 16%, rgba(0, 198, 255, 0.18), transparent 44%),
    linear-gradient(180deg, rgba(0, 198, 255, 0.08), rgba(255, 255, 255, 0.02));
  border: 1px solid rgba(0, 198, 255, 0.1);
  box-shadow: 0 36px 120px rgba(0, 198, 255, 0.14);
}

.phone-frame {
  --scene-accent: var(--clr-accent);
  position: relative;
  width: min(286px, 74vw);
  aspect-ratio: 9 / 18.8;
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 38px;
  padding: 10px;
  background:
    linear-gradient(140deg, rgba(255, 255, 255, 0.22), transparent 18%, rgba(255, 255, 255, 0.08) 74%),
    #070a0f;
  box-shadow:
    0 28px 76px rgba(0, 0, 0, 0.44),
    0 0 70px color-mix(in srgb, var(--scene-accent) 18%, transparent);
  z-index: 2;
}

.phone-hardware-glow {
  position: absolute;
  inset: -18px;
  border-radius: 54px;
  background: radial-gradient(circle at 50% 8%, color-mix(in srgb, var(--scene-accent) 18%, transparent), transparent 60%);
  pointer-events: none;
  opacity: 0.7;
}

.phone-notch {
  position: absolute;
  top: 16px;
  left: 50%;
  width: 92px;
  height: 24px;
  border-radius: 0 0 16px 16px;
  background: #05070b;
  transform: translateX(-50%);
  z-index: 5;
}

.phone-screen {
  position: relative;
  height: 100%;
  overflow: hidden;
  border-radius: 30px;
  padding: 13px 14px 12px;
  display: flex;
  flex-direction: column;
  color: #f4f7fb;
  background:
    radial-gradient(circle at 18% 8%, color-mix(in srgb, var(--scene-accent) 28%, transparent), transparent 30%),
    linear-gradient(180deg, #111824, #070a10 72%);
}

.phone-screen::after {
  content: '';
  position: absolute;
  inset: 0;
  border-radius: inherit;
  pointer-events: none;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
}

.phone-status {
  display: flex;
  justify-content: space-between;
  align-items: center;
  height: 24px;
  padding: 0 4px;
  font-family: var(--font-mono);
  font-size: 0.625rem;
  color: rgba(244, 247, 251, 0.72);
}

.status-icons {
  letter-spacing: 0;
}

.app-topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 2px 10px;
}

.app-kicker {
  font-family: var(--font-mono);
  font-size: 0.625rem;
  color: color-mix(in srgb, var(--scene-accent) 76%, white);
}

.app-topbar h3 {
  font-family: var(--font-display);
  font-size: 1.35rem;
  line-height: 1.15;
  color: #fff;
}

.live-pill {
  font-family: var(--font-mono);
  font-size: 0.625rem;
  color: #061014;
  padding: 4px 8px;
  border-radius: 999px;
  background: var(--scene-accent);
  box-shadow: 0 0 18px color-mix(in srgb, var(--scene-accent) 52%, transparent);
}

.screen-stack {
  position: relative;
  flex: 1;
  min-height: 0;
}

.screen-content {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.hero-mini,
.bind-card,
.device-row,
.project-card,
.message-summary {
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(255, 255, 255, 0.07);
  border-radius: 14px;
}

.hero-mini {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 12px;
}

.mini-icon,
.folder-icon {
  width: 36px;
  height: 36px;
  border-radius: 11px;
  display: grid;
  place-items: center;
  color: #061014;
  background: var(--scene-accent);
  box-shadow: 0 0 20px color-mix(in srgb, var(--scene-accent) 44%, transparent);
}

.hero-mini div,
.device-row div,
.project-card div,
.message-row div,
.message-summary div {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.hero-mini strong,
.device-row strong,
.project-card strong,
.message-row strong,
.message-summary strong {
  font-size: 0.875rem;
  color: #fff;
}

.hero-mini span:last-child,
.device-row span,
.project-card span,
.project-card small,
.message-row small,
.message-summary span {
  font-size: 0.6875rem;
  color: rgba(244, 247, 251, 0.58);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bind-card {
  display: grid;
  grid-template-columns: 94px 1fr;
  gap: 12px;
  padding: 12px;
  align-items: center;
}

.qr-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 3px;
  padding: 7px;
  border-radius: 12px;
  background: #fff;
}

.qr-grid i {
  aspect-ratio: 1;
  border-radius: 2px;
  background: #d7dde8;
}

.qr-grid i.filled {
  background: #07111c;
}

.bind-copy {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.bind-copy span,
.bind-copy small {
  color: rgba(244, 247, 251, 0.58);
  font-size: 0.6875rem;
}

.bind-copy strong {
  color: #fff;
  font-family: var(--font-mono);
  font-size: 1.25rem;
  letter-spacing: 0;
}

.device-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.device-row {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 11px;
}

.device-dot,
.pulse {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #4ade80;
  box-shadow: 0 0 14px rgba(74, 222, 128, 0.55);
  flex: 0 0 auto;
}

.device-dot.idle {
  background: #f0c674;
  box-shadow: 0 0 14px rgba(240, 198, 116, 0.42);
}

.search-pill,
.input-bar {
  height: 36px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  padding: 0 12px;
  color: rgba(244, 247, 251, 0.48);
  background: rgba(255, 255, 255, 0.07);
  border: 1px solid rgba(255, 255, 255, 0.08);
  font-size: 0.75rem;
}

.project-summary {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 8px;
}

.project-summary div {
  min-height: 64px;
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  background: rgba(255, 255, 255, 0.07);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.project-summary strong {
  color: #fff;
  font-size: 1.15rem;
}

.project-summary span {
  color: rgba(244, 247, 251, 0.5);
  font-size: 0.6875rem;
}

.project-card {
  display: grid;
  grid-template-columns: 38px 1fr auto;
  gap: 10px;
  align-items: center;
  padding: 12px;
}

.project-card b {
  color: #061014;
  background: var(--scene-accent);
  border-radius: 999px;
  padding: 5px 8px;
  font-size: 0.6875rem;
}

.workspace-actions {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 7px;
}

.workspace-actions span,
.terminal-tools span,
.message-tabs span {
  min-height: 30px;
  display: grid;
  place-items: center;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.07);
  color: rgba(244, 247, 251, 0.72);
  font-size: 0.6875rem;
}

.status-feed {
  margin-top: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.status-feed div {
  display: flex;
  align-items: center;
  gap: 8px;
  color: rgba(244, 247, 251, 0.66);
  font-size: 0.75rem;
}

.pulse.caution {
  background: #f0c674;
  box-shadow: 0 0 14px rgba(240, 198, 116, 0.42);
}

.terminal-screen {
  gap: 8px;
}

.session-tabs,
.terminal-tools,
.message-tabs {
  display: flex;
  gap: 7px;
}

.session-tabs span {
  padding: 6px 10px;
  border-radius: 999px;
  color: rgba(244, 247, 251, 0.56);
  background: rgba(255, 255, 255, 0.07);
  font-size: 0.6875rem;
  white-space: nowrap;
}

.session-tabs .selected,
.message-tabs .active {
  color: #061014;
  background: var(--scene-accent);
}

.terminal-window {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 12px;
  border-radius: 14px;
  background: #05080d;
  border: 1px solid rgba(255, 255, 255, 0.09);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);
  overflow: hidden;
}

.term-line {
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  color: #c7edf8;
  white-space: nowrap;
}

.term-line.dim {
  color: #6d7686;
}

.term-line.ok {
  color: #78dfa0;
}

.term-line.warn {
  color: #f0c674;
}

.term-line.cursor::after {
  content: '▌';
  color: var(--scene-accent);
  animation: cursorBlink 1s step-end infinite;
}

.terminal-tools span {
  flex: 1;
}

.message-tabs {
  overflow: hidden;
}

.message-tabs span {
  padding: 0 9px;
  white-space: nowrap;
}

.message-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.message-row {
  min-height: 76px;
  display: grid;
  grid-template-columns: 10px 1fr;
  gap: 10px;
  align-items: center;
  padding: 11px 12px;
  border-radius: 14px;
  background: linear-gradient(90deg, rgba(255, 255, 255, 0.075), rgba(255, 255, 255, 0.045));
  border: 1px solid rgba(255, 255, 255, 0.07);
}

.message-row i {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--scene-accent);
  box-shadow: 0 0 14px color-mix(in srgb, var(--scene-accent) 45%, transparent);
}

.message-row span {
  font-size: 0.6875rem;
  color: var(--scene-accent);
}

.message-row.danger {
  border-color: rgba(248, 113, 113, 0.28);
}

.message-row.success {
  border-color: rgba(74, 222, 128, 0.24);
}

.message-summary {
  margin-top: auto;
  display: flex;
  gap: 10px;
  align-items: center;
  justify-content: flex-start;
  padding: 12px;
  background:
    radial-gradient(circle at 92% 0, color-mix(in srgb, var(--scene-accent) 16%, transparent), transparent 46%),
    rgba(255, 255, 255, 0.06);
}

.app-tabbar {
  height: 54px;
  margin-top: 12px;
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.app-tabbar span {
  display: grid;
  place-items: center;
  color: rgba(244, 247, 251, 0.46);
  font-size: 0.6875rem;
}

.app-tabbar span.active {
  color: var(--scene-accent);
}

.scene-controls {
  position: absolute;
  bottom: 22px;
  display: flex;
  gap: 8px;
  z-index: 4;
}

.scene-dot {
  position: relative;
  width: 42px;
  height: 5px;
  border: 0;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.14);
  overflow: hidden;
  cursor: pointer;
}

.scene-dot span {
  position: absolute;
  inset: 0 auto 0 0;
  background: var(--clr-accent);
  border-radius: inherit;
}

.scene-dot.active {
  background: rgba(0, 198, 255, 0.18);
}

.screen-fade-enter-active,
.screen-fade-leave-active {
  transition: opacity 0.28s ease, transform 0.28s ease;
}

.screen-fade-enter-from {
  opacity: 0;
  transform: translateY(12px) scale(0.98);
}

.screen-fade-leave-to {
  opacity: 0;
  transform: translateY(-10px) scale(0.98);
}

@keyframes cursorBlink {
  50% { opacity: 0; }
}

@media (max-width: 1100px) {
  .mobile-layout {
    grid-template-columns: 1fr;
  }

  .mobile-stage {
    min-height: 620px;
  }
}

@media (max-width: 768px) {
  .mobile-highlights {
    grid-template-columns: 1fr;
  }

  .mobile-stage {
    min-height: 580px;
  }

  .phone-frame {
    width: min(278px, 86vw);
  }

  .scene-controls {
    bottom: 8px;
  }
}
</style>
