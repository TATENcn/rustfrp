<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  NNotificationProvider,
  darkTheme,
} from 'naive-ui'
import { createAppI18n } from '@/i18n'

const i18n = createAppI18n()
const { naiveLocale, naiveDateLocale } = i18n

const isDark = ref(localStorage.getItem('rustfrp-theme') === 'dark')
watch(isDark, (v) =>
  localStorage.setItem('rustfrp-theme', v ? 'dark' : 'light'),
)
</script>

<template>
  <NConfigProvider
    :locale="naiveLocale"
    :date-locale="naiveDateLocale"
    :theme="isDark ? darkTheme : undefined"
  >
    <NMessageProvider>
      <NDialogProvider>
        <NNotificationProvider>
          <RouterView />
        </NNotificationProvider>
      </NDialogProvider>
    </NMessageProvider>
  </NConfigProvider>
</template>
