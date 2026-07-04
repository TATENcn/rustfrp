import {
  zhCN,
  dateZhCN,
  enUS,
  dateEnUS,
  type NLocale,
  type NDateLocale,
} from 'naive-ui'

export type SupportedLocale = 'zh' | 'en'

export const DEFAULT_LOCALE: SupportedLocale = 'en'

export function getNaiveLocale(lang: SupportedLocale): NLocale {
  switch (lang) {
    case 'zh':
      return zhCN
    case 'en':
      return enUS
  }
}

export function getNaiveDateLocale(lang: SupportedLocale): NDateLocale {
  switch (lang) {
    case 'zh':
      return dateZhCN
    case 'en':
      return dateEnUS
  }
}
