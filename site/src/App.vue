<script setup lang="ts">
import { ref } from 'vue'
import { useScrollReveal } from './composables/useScrollReveal'
import NavBar from './components/NavBar.vue'
import HeroSection from './components/HeroSection.vue'
import FeaturesSection from './components/FeaturesSection.vue'

import ComparisonSection from './components/ComparisonSection.vue'
import DownloadSection from './components/DownloadSection.vue'
import SiteFooter from './components/SiteFooter.vue'

const appRef = ref<HTMLElement | null>(null)
useScrollReveal(appRef)
</script>

<template>
  <div ref="appRef" class="app">
    <NavBar />
    <HeroSection />
    <FeaturesSection />
    <ComparisonSection />
    <DownloadSection />
    <SiteFooter />
  </div>
</template>

<style scoped>
.app {
  min-height: 100vh;
}
</style>
