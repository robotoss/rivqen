import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import LoadTimeline from './components/LoadTimeline.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('LoadTimeline', LoadTimeline)
  }
} satisfies Theme
