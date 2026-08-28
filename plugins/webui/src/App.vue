<script setup lang="ts">
import {
  NConfigProvider,
  NMessageProvider,
  NDialogProvider,
  NNotificationProvider,
  NLoadingBarProvider,
  darkTheme,
} from 'naive-ui'
import { createAppI18n } from '@/i18n'
import { useThemeStore } from '@/stores/theme'

const i18n = createAppI18n()
const { naiveLocale, naiveDateLocale } = i18n
const themeStore = useThemeStore()
</script>

<template>
  <NConfigProvider
    :locale="naiveLocale"
    :date-locale="naiveDateLocale"
    :theme="themeStore.isDark ? darkTheme : undefined"
    :theme-overrides="themeStore.naiveThemeOverrides"
  >
    <NLoadingBarProvider>
      <NDialogProvider>
        <NNotificationProvider>
          <NMessageProvider>
            <RouterView />
          </NMessageProvider>
        </NNotificationProvider>
      </NDialogProvider>
    </NLoadingBarProvider>
  </NConfigProvider>
</template>
