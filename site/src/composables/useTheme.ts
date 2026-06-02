import { ref } from 'vue'

const THEME_KEY = 'kn-site-theme'
type Theme = 'dark' | 'light'

const theme = ref<Theme>('dark')
let initialized = false

function apply(t: Theme) {
  document.documentElement.setAttribute('data-theme', t)
  localStorage.setItem(THEME_KEY, t)
}

function init() {
  const stored = localStorage.getItem(THEME_KEY) as Theme | null
  if (stored === 'dark' || stored === 'light') {
    theme.value = stored
  } else if (window.matchMedia('(prefers-color-scheme: light)').matches) {
    theme.value = 'light'
  } else {
    theme.value = 'dark'
  }
  apply(theme.value)
  initialized = true
}

export function useTheme() {
  if (!initialized) init()

  function toggle() {
    theme.value = theme.value === 'dark' ? 'light' : 'dark'
    apply(theme.value)
  }

  return { theme, toggle }
}
