import { ref, onMounted, onUnmounted, type Ref, watch } from 'vue'

export interface TypingOptions {
  /** 每字打字间隔 (ms)，默认 60 */
  speed?: number
  /** 打完后的停顿 (ms)，默认 2000 */
  pause?: number
  /** 是否循环，默认 true */
  loop?: boolean
  /** 初始延迟 (ms)，默认 300 */
  initialDelay?: number
  /** 循环之间的删除动画：true 则逐字删除后再打 */
  backspace?: boolean
  /** 删除速度 (ms)，默认 25 */
  backspaceSpeed?: number
}

export function useTypingAnimation(
  texts: string[] | Ref<string[]>,
  options: TypingOptions = {}
) {
  const {
    speed = 50,
    pause = 2000,
    loop = true,
    initialDelay = 300,
    backspace = false,
    backspaceSpeed = 20,
  } = options

  const displayText = ref('')
  const isTyping = ref(false)
  const isPaused = ref(false)
  const currentIndex = ref(0)

  let timer: ReturnType<typeof setTimeout> | null = null

  const sourceTexts = Array.isArray(texts) ? ref(texts) : texts

  function clearTimer() {
    if (timer !== null) {
      clearTimeout(timer)
      timer = null
    }
  }

  async function typeText(text: string): Promise<void> {
    isPaused.value = false
    for (let i = 0; i <= text.length; i++) {
      if (!isTyping.value) return
      await new Promise<void>((resolve) => {
        timer = setTimeout(() => {
          displayText.value = text.slice(0, i)
          resolve()
        }, i === 0 ? 0 : speed)
      })
    }
    isPaused.value = true

    if (backspace && loop) {
      await new Promise<void>((resolve) => {
        timer = setTimeout(resolve, pause)
      })
      // Backspace
      for (let i = text.length; i >= 0; i--) {
        if (!isTyping.value) return
        await new Promise<void>((resolve) => {
          timer = setTimeout(() => {
            displayText.value = text.slice(0, i)
            resolve()
          }, backspaceSpeed)
        })
      }
    }
  }

  async function run() {
    if (!isTyping.value) return
    const texts = sourceTexts.value
    if (texts.length === 0) return

    while (isTyping.value) {
      const text = texts[currentIndex.value % texts.length]
      await typeText(text)

      if (!isTyping.value) return

      // Pause before next text
      await new Promise<void>((resolve) => {
        timer = setTimeout(resolve, backspace ? speed * 3 : pause)
      })

      if (!isTyping.value) return

      if (!backspace) {
        displayText.value = ''
      }

      currentIndex.value++
      if (!loop && currentIndex.value >= texts.length) {
        break
      }
    }
  }

  function start() {
    if (isTyping.value) return
    isTyping.value = true
    currentIndex.value = 0
    displayText.value = ''
    // Initial delay
    timer = setTimeout(() => {
      run()
    }, initialDelay)
  }

  function stop() {
    isTyping.value = false
    clearTimer()
  }

  function reset() {
    stop()
    displayText.value = ''
    currentIndex.value = 0
  }

  // Watch for text changes
  watch(sourceTexts, () => {
    if (isTyping.value) {
      stop()
      start()
    }
  })

  onMounted(() => {
    start()
  })

  onUnmounted(() => {
    stop()
  })

  return {
    displayText,
    isTyping,
    isPaused,
    currentIndex,
    start,
    stop,
    reset,
  }
}
