<script setup lang="ts">
import { computed, h, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { NButton, NDropdown, NIcon, NLayout, NLayoutContent, NLayoutHeader, NLayoutSider, NMenu, NSelect, NSpace, NTag, NTooltip, useMessage, type DropdownOption, type MenuOption } from 'naive-ui'
import { useI18n } from '@/i18n'
import { formatDuration } from '@/i18n/format'
import { useSystemStore } from '@/stores/system'
import { useEnvironmentStore } from '@/stores/environments'
import { useThemeStore, type ThemeAccent, type ThemeMode } from '@/stores/theme'
import AppIcon from '@/components/icon/AppIcon.vue'
import StatusBadge from '@/components/common/StatusBadge.vue'
import type { AppIconName } from '@/components/icon/types'

const router = useRouter()
const route = useRoute()
const i18n = useI18n()
const message = useMessage()
const systemStore = useSystemStore()
const environmentStore = useEnvironmentStore()
const themeStore = useThemeStore()
const collapsed = ref(false)
const hasToken = ref(!!localStorage.getItem('api_token'))

function renderIcon(name: AppIconName) {
  return () => h(NIcon, null, { default: () => h(AppIcon, { name, size: 19 }) })
}

const menuOptions = computed<MenuOption[]>(() => [
  { label: i18n.t('nav.dashboard'), key: 'dashboard', icon: renderIcon('dashboard') },
  { label: i18n.t('nav.profiles'), key: 'profiles', icon: renderIcon('profiles') },
  { label: i18n.t('nav.proxies'), key: 'proxies', icon: renderIcon('proxies') },
  { label: i18n.t('nav.bindings'), key: 'bindings', icon: renderIcon('bindings') },
  { label: i18n.t('nav.visitors'), key: 'visitors', icon: renderIcon('visitors') },
  { label: i18n.t('nav.logs'), key: 'logs', icon: renderIcon('logs') },
  { label: i18n.t('nav.status'), key: 'status', icon: renderIcon('status') },
])

function pathToKey(path: string) {
  for (const key of ['proxies', 'profiles', 'bindings', 'visitors', 'logs', 'status']) if (path.startsWith(`/${key}`)) return key
  return 'dashboard'
}
const menuKey = ref(pathToKey(route.path))
watch(() => route.path, (path) => { menuKey.value = pathToKey(path) })
function handleMenuClick(key: string) { menuKey.value = key; void router.push({ name: key }) }

const themeOptions = computed<DropdownOption[]>(() => [
  { type: 'group', label: i18n.locale.value === 'zh' ? '显示模式' : 'Appearance', key: 'mode-group', children: [
    { label: i18n.locale.value === 'zh' ? '跟随系统' : 'System', key: 'mode:system', icon: renderIcon('theme-system') },
    { label: i18n.locale.value === 'zh' ? '浅色' : 'Light', key: 'mode:light', icon: renderIcon('theme-light') },
    { label: i18n.locale.value === 'zh' ? '深色' : 'Dark', key: 'mode:dark', icon: renderIcon('theme-dark') },
  ] },
  { type: 'divider', key: 'divider' },
  { type: 'group', label: i18n.locale.value === 'zh' ? '主题色' : 'Accent', key: 'accent-group', children: [
    { label: i18n.locale.value === 'zh' ? '蓝色' : 'Blue', key: 'accent:blue' },
    { label: i18n.locale.value === 'zh' ? '青色' : 'Cyan', key: 'accent:cyan' },
    { label: i18n.locale.value === 'zh' ? '绿色' : 'Green', key: 'accent:green' },
    { label: i18n.locale.value === 'zh' ? '紫色' : 'Violet', key: 'accent:violet' },
    { label: i18n.locale.value === 'zh' ? '橙色' : 'Orange', key: 'accent:orange' },
  ] },
])
function handleThemeSelect(key: string) {
  const [kind, value] = key.split(':')
  if (kind === 'mode') themeStore.setMode(value as ThemeMode)
  if (kind === 'accent') themeStore.setAccent(value as ThemeAccent)
}
function toggleLocale() { i18n.setLocale(i18n.locale.value === 'zh' ? 'en' : 'zh') }
async function handleReload() {
  try { const task = await systemStore.triggerReload(); if (task) message.success(i18n.t('app.reloadStarted', { task })) }
  catch { message.error(i18n.t('error.serverError')) }
}
function handleLogout() { localStorage.removeItem('api_token'); void router.push({ name: 'login' }) }
const activeCount = computed(() => systemStore.status?.active_frpc_instances ?? 0)
const uptimeText = computed(() => systemStore.currentUptimeSecs !== null ? formatDuration(systemStore.currentUptimeSecs, i18n.locale.value, { style: 'narrow', includeSeconds: false }) : '—')
onMounted(() => { systemStore.startPolling(); void environmentStore.fetchAll() })
onUnmounted(() => systemStore.stopPolling())
</script>

<template>
  <NLayout class="h-screen bg-canvas">
    <NLayoutHeader bordered class="h-16 px-4 sm:px-6">
      <div class="flex h-full items-center justify-between gap-4">
        <div class="flex min-w-0 items-center gap-3">
          <span class="grid size-9 shrink-0 place-items-center rounded-xl bg-primary text-white shadow-sm"><AppIcon name="proxies" :size="19" /></span>
          <div class="min-w-0"><div class="truncate text-base font-semibold text-foreground">{{ i18n.t('app.title') }}</div><div class="hidden text-xs text-foreground-muted sm:block">{{ i18n.t('app.controlPlane') }}</div></div>
        </div>
        <NSpace align="center" :size="8" :wrap="false">
          <NSelect v-if="environmentStore.environments.length > 1" :value="environmentStore.activeId" :options="environmentStore.environments.map(item => ({ label: item.name, value: item.id! }))" size="small" class="hidden w-40 md:block" :aria-label="i18n.t('app.environment')" @update:value="environmentStore.select" />
          <NTag v-else-if="environmentStore.active" size="small" :bordered="false" class="hidden md:inline-flex">{{ environmentStore.active.name }}</NTag>
          <StatusBadge class="hidden sm:inline-flex" :status="systemStore.error ? 'stale' : activeCount ? 'running' : 'stopped'" :label="i18n.t('status.frpcRunning', { count: activeCount })" />
          <NTooltip><template #trigger><NButton quaternary circle :aria-label="i18n.t('app.language')" @click="toggleLocale"><template #icon><AppIcon name="language" /></template></NButton></template>{{ i18n.locale.value === 'zh' ? 'English' : '中文' }}</NTooltip>
          <NDropdown trigger="click" :options="themeOptions" @select="handleThemeSelect"><NTooltip><template #trigger><NButton quaternary circle :aria-label="i18n.t('app.appearance')"><template #icon><AppIcon name="palette" /></template></NButton></template>{{ i18n.t('app.appearance') }}</NTooltip></NDropdown>
          <NTooltip><template #trigger><NButton quaternary circle :aria-label="i18n.t('app.reload')" @click="handleReload"><template #icon><AppIcon name="reload" /></template></NButton></template>{{ i18n.t('app.reload') }}</NTooltip>
          <NTooltip v-if="hasToken"><template #trigger><NButton quaternary circle :aria-label="i18n.t('auth.logout')" @click="handleLogout"><template #icon><AppIcon name="logout" /></template></NButton></template>{{ i18n.t('auth.logout') }}</NTooltip>
        </NSpace>
      </div>
    </NLayoutHeader>
    <NLayout has-sider position="absolute" class="top-16! bottom-8!">
      <NLayoutSider bordered collapse-mode="width" :collapsed="collapsed" :width="220" :collapsed-width="64" show-trigger @collapse="collapsed = true" @expand="collapsed = false">
        <NMenu :value="menuKey" :options="menuOptions" :collapsed="collapsed" :collapsed-width="64" class="pt-3" @update:value="handleMenuClick" />
      </NLayoutSider>
      <NLayoutContent content-class="min-h-full" content-style="padding: 24px; overflow-y: auto; height: 100%; box-sizing: border-box; background: var(--ui-canvas)">
        <main class="mx-auto w-full max-w-[1500px]"><RouterView /></main>
      </NLayoutContent>
    </NLayout>
    <footer class="fixed inset-x-0 bottom-0 z-20 flex h-8 items-center justify-between border-t border-border bg-surface px-4 text-xs text-foreground-muted">
      <span class="flex items-center gap-2"><span class="size-1.5 rounded-full" :class="activeCount ? 'bg-success' : 'bg-foreground-muted'" />{{ i18n.t('app.ready') }} · {{ uptimeText }}</span><span>RustFRP v0.3.0</span>
    </footer>
  </NLayout>
</template>
