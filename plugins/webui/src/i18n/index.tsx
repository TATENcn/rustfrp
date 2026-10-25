import {
  createContext,
  useContext,
  useState,
  useEffect,
  type ReactNode,
} from "react";
import type { I18nKey, I18nParams } from "./types";
import type { SupportedLocale } from "./locale";
import en from "./messages/en.json";
import zh from "./messages/zh.json";
const messages = { en, zh };
type I18n = {
  locale: SupportedLocale;
  setLocale: (locale: SupportedLocale) => void;
  t: (key: I18nKey, params?: I18nParams) => string;
  text: (zh: string, en: string) => string;
};
const Context = createContext<I18n | null>(null);
export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState<SupportedLocale>(() =>
    localStorage.getItem("rustfrp-locale") === "zh" ? "zh" : "en",
  );
  useEffect(() => {
    localStorage.setItem("rustfrp-locale", locale);
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
  }, [locale]);
  const t = (key: I18nKey, params?: I18nParams) =>
    (messages[locale][key] ?? en[key] ?? key).replace(
      /\{(\w+)\}/g,
      (_, name: string) => String(params?.[name] ?? `{${name}}`),
    );
  return (
    <Context.Provider
      value={{
        locale,
        setLocale,
        t,
        text: (cn, english) => (locale === "zh" ? cn : english),
      }}
    >
      {children}
    </Context.Provider>
  );
}
export function useI18n() {
  const context = useContext(Context);
  if (!context) throw new Error("Missing I18nProvider");
  return context;
}
