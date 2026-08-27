import { createApp } from 'vue'
import { createPinia } from 'pinia'
import router from '@/router'
import App from '@/App.vue'
import { createAppI18n } from '@/i18n'
import '@/styles/app.css'

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.mount('#app')

// Initialize i18n after mount (needs app context for provide/inject)
// Called inside App.vue setup()
