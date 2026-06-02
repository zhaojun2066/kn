<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'

const steps = [
  {
    num: '1',
    title: '创建 Profile',
    desc: '在 GUI 中新建 Profile，填入 API Key 和 Base URL，或从现有配置一键扫描导入。',
    cmd: 'profile add deepseek "DeepSeek 中转"',
  },
  {
    num: '2',
    title: '配置环境变量',
    desc: '设置模型对应关系、自定义任意变量。支持 Claude Code 和 Codex 两种 CLI 类型。',
    cmd: 'profile set deepseek ANTHROPIC_BASE_URL=https://api.deepseek.com/anthropic',
  },
  {
    num: '3',
    title: '一行命令启动',
    desc: '终端中执行命令，环境变量自动注入当前会话，退出后自动清除，零残留。',
    cmd: 'ai claude deepseek',
  },
]

// Animated typing for each step's code block
const typedCmds = ref<string[]>(steps.map(() => ''))
const activeStep = ref(-1)
const typingTimers: ReturnType<typeof setTimeout>[] = []
let observer: IntersectionObserver | null = null

function typeStep(index: number) {
  if (activeStep.value === index) return
  activeStep.value = index
  const text = steps[index].cmd
  let i = 0
  typedCmds.value[index] = ''

  function type() {
    if (i <= text.length) {
      typedCmds.value[index] = text.slice(0, i)
      i++
      const timer = setTimeout(type, 40)
      typingTimers.push(timer)
    }
  }
  type()
}

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (entry.isIntersecting) {
          const idx = Number((entry.target as HTMLElement).dataset.step)
          if (!isNaN(idx)) {
            setTimeout(() => typeStep(idx), idx * 200)
          }
        }
      })
    },
    { threshold: 0.3 }
  )

  document.querySelectorAll('.how-card').forEach((el) => observer!.observe(el))
})

onUnmounted(() => {
  observer?.disconnect()
  typingTimers.forEach(clearTimeout)
})
</script>

<template>
  <section class="how section" id="how">
    <div class="container">
      <div class="how-header reveal">
        <span class="section-label">三步开始</span>
        <h2 class="section-title">比想象中更简单</h2>
      </div>

      <div class="how-steps">
        <div
          v-for="(s, i) in steps"
          :key="s.num"
          class="how-card glass-card reveal"
          :data-step="i"
          :style="{ transitionDelay: `${i * 100}ms` }"
        >
          <div class="how-top">
            <span class="how-num">{{ s.num }}</span>
            <h3 class="how-title">{{ s.title }}</h3>
          </div>
          <p class="how-desc">{{ s.desc }}</p>
          <div class="how-cmd code-block">
            <span class="prompt">$ </span>
            <span class="cmd">{{ typedCmds[i] }}</span>
            <span v-if="activeStep === i && typedCmds[i].length <= s.cmd.length" class="term-cursor">▌</span>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.how-header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 52px;
}

.how-steps {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 20px;
}

.how-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 34px 28px;
}

.how-top {
  display: flex;
  align-items: center;
  gap: 14px;
}

.how-num {
  font-family: var(--font-display);
  font-size: 1.5rem;
  font-weight: 700;
  color: var(--clr-accent);
  background: var(--clr-accent-dim);
  width: 40px;
  height: 40px;
  border-radius: var(--radius-md);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.how-title {
  font-family: var(--font-display);
  font-size: 1.125rem;
  font-weight: 600;
  color: var(--clr-text);
}

.how-desc {
  font-size: 0.9375rem;
  color: var(--clr-text-secondary);
  line-height: 1.6;
}

.how-cmd {
  margin-top: 6px;
  font-size: 0.8125rem;
  padding: 14px 18px;
  display: flex;
  align-items: center;
  min-height: 44px;
}

.term-cursor {
  color: var(--clr-accent);
  animation: cursor-blink 1s step-end infinite;
}

@keyframes cursor-blink {
  50% { opacity: 0; }
}

@media (max-width: 768px) {
  .how-steps {
    grid-template-columns: 1fr;
  }
}
</style>
