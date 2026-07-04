import { ref, computed, inject, provide, type Ref, type ComputedRef } from 'vue'
import { type NLocale, type NDateLocale } from 'naive-ui'
import {
  getNaiveLocale,
  getNaiveDateLocale,
  type SupportedLocale,
  DEFAULT_LOCALE,
} from './locale'
import type { I18nKey, I18nParams } from './types'
import enMessages from './messages/en.json'
import zhMessages from './messages/zh.json'

const messagesMap: Record<SupportedLocale, Record<string, string>> = {
  en: enMessages as Record<string, string>,
  zh: zhMessages as Record<string, string>,
}

export interface I18nInstance {
  locale: Ref<SupportedLocale>
  setLocale: (lang: SupportedLocale) => void
  t: (key: I18nKey, params?: I18nParams) => string
  naiveLocale: ComputedRef<NLocale>
  naiveDateLocale: ComputedRef<NDateLocale>
}

const I18N_KEY = Symbol('i18n')

export function createAppI18n(initial?: SupportedLocale): I18nInstance {
  const saved = localStorage.getItem('rustfrp-locale') as SupportedLocale | null
  const locale = ref<SupportedLocale>(
    (saved as SupportedLocale) || initial || DEFAULT_LOCALE,
  )

  function setLocale(lang: SupportedLocale) {
    locale.value = lang
    localStorage.setItem('rustfrp-locale', lang)
  }

  const t = (key: I18nKey, params?: I18nParams): string => {
    const template =
      messagesMap[locale.value]?.[key] ?? messagesMap.en[key] ?? key
    if (!params) return template
    return template.replace(/\{(\w+)\}/g, (_, k) =>
      String(params[k] ?? `{${k}}`),
    )
  }

  const naiveLocale = computed(() => getNaiveLocale(locale.value))
  const naiveDateLocale = computed(() => getNaiveDateLocale(locale.value))

  const instance: I18nInstance = {
    locale,
    setLocale,
    t,
    naiveLocale,
    naiveDateLocale,
  }

  provide(I18N_KEY, instance)
  return instance
}

export function useI18n(): I18nInstance {
  const instance = inject<I18nInstance>(I18N_KEY)
  if (!instance) {
    throw new Error(
      'useI18n() must be called inside a component tree where createAppI18n() was called',
    )
  }
  return instance
}
