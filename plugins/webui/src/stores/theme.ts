import { computed, onScopeDispose, ref } from 'vue'
import { defineStore } from 'pinia'
import type { GlobalThemeOverrides } from 'naive-ui'

export type ThemeMode = 'light' | 'dark' | 'system'
export type ThemeAccent = 'blue' | 'cyan' | 'green' | 'violet' | 'orange'

interface ThemePreferences {
  version: 1
  mode: ThemeMode
  accent: ThemeAccent
}

const STORAGE_KEY = 'rustfrp-ui-preferences'
const accents: Record<ThemeAccent, { primary: string; hover: string; pressed: string }> = {
  blue: { primary: '#2563eb', hover: '#3b82f6', pressed: '#1d4ed8' },
  cyan: { primary: '#0891b2', hover: '#06a6c9', pressed: '#0e7490' },
  green: { primary: '#159455', hover: '#20a866', pressed: '#107a45' },
  violet: { primary: '#7c3aed', hover: '#8b5cf6', pressed: '#6d28d9' },
  orange: { primary: '#ea580c', hover: '#f97316', pressed: '#c2410c' },
}

function loadPreferences(): ThemePreferences {
  try {
    const value = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}') as Partial<ThemePreferences>
    return {
      version: 1,
      mode: value.mode && ['light', 'dark', 'system'].includes(value.mode) ? value.mode : 'system',
      accent: value.accent && value.accent in accents ? value.accent : 'blue',
    }
  } catch {
    return { version: 1, mode: 'system', accent: 'blue' }
  }
}

export const useThemeStore = defineStore('theme', () => {
  const saved = loadPreferences()
  const mode = ref<ThemeMode>(saved.mode)
  const accent = ref<ThemeAccent>(saved.accent)
  const media = window.matchMedia('(prefers-color-scheme: dark)')
  const systemDark = ref(media.matches)
  const resolvedMode = computed<'light' | 'dark'>(() => mode.value === 'system' ? (systemDark.value ? 'dark' : 'light') : mode.value)
  const isDark = computed(() => resolvedMode.value === 'dark')

  const naiveThemeOverrides = computed<GlobalThemeOverrides>(() => {
    const color = accents[accent.value]
    return {
      common: {
        primaryColor: color.primary,
        primaryColorHover: color.hover,
        primaryColorPressed: color.pressed,
        primaryColorSuppl: color.primary,
        borderRadius: '8px',
        borderRadiusSmall: '6px',
      },
      Card: { borderRadius: '12px' },
      Button: { borderRadiusMedium: '8px' },
    }
  })

  function apply(animate = true) {
    const root = document.documentElement
    if (animate && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      root.classList.add('theme-transition')
      window.setTimeout(() => root.classList.remove('theme-transition'), 200)
    }
    root.dataset.theme = resolvedMode.value
    root.dataset.accent = accent.value
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ version: 1, mode: mode.value, accent: accent.value }))
  }

  function setMode(value: ThemeMode) { mode.value = value; apply() }
  function setAccent(value: ThemeAccent) { accent.value = value; apply() }
  const onSystemTheme = (event: MediaQueryListEvent) => { systemDark.value = event.matches; if (mode.value === 'system') apply() }
  media.addEventListener('change', onSystemTheme)
  onScopeDispose(() => media.removeEventListener('change', onSystemTheme))
  apply(false)

  return { mode, accent, resolvedMode, isDark, naiveThemeOverrides, setMode, setAccent }
})
