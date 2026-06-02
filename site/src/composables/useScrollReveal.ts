import { onMounted, onUnmounted, type Ref } from 'vue'

export function useScrollReveal(
  containerRef: Ref<HTMLElement | null>,
  options?: { threshold?: number; rootMargin?: string }
) {
  let observer: IntersectionObserver | null = null

  onMounted(() => {
    if (!containerRef.value) return

    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add('visible')
          }
        }
      },
      {
        threshold: options?.threshold ?? 0.1,
        rootMargin: options?.rootMargin ?? '0px 0px -40px 0px',
      }
    )

    const targets = containerRef.value.querySelectorAll('.reveal')
    targets.forEach((el) => observer!.observe(el))
  })

  onUnmounted(() => {
    observer?.disconnect()
  })

  return {}
}
