import enMessages from './messages/en.json'

export type I18nKey = keyof typeof enMessages

export interface I18nParams {
  [key: string]: string | number
}
