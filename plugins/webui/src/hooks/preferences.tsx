import {
  createContext,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
export type ThemeMode = "system" | "light" | "dark";
function initialMode(): ThemeMode {
  try {
    const mode = JSON.parse(
      localStorage.getItem("rustfrp-ui-preferences") || "{}",
    ).mode;
    return ["light", "dark", "system"].includes(mode) ? mode : "system";
  } catch {
    return "system";
  }
}
function useSettings() {
  const [mode, setMode] = useState<ThemeMode>(initialMode);
  const [dark, setDark] = useState(
    () =>
      mode === "dark" ||
      (mode === "system" && matchMedia("(prefers-color-scheme: dark)").matches),
  );
  const [environmentId, setEnvironment] = useState<number | null>(
    () => Number(localStorage.getItem("rustfrp-environment")) || null,
  );
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const value = mode === "dark" || (mode === "system" && media.matches);
      document.documentElement.classList.toggle("dark", value);
      setDark(value);
    };
    apply();
    localStorage.setItem(
      "rustfrp-ui-preferences",
      JSON.stringify({ version: 2, mode }),
    );
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [mode]);
  return {
    mode,
    dark,
    setMode,
    environmentId,
    selectEnvironment: (id: number) => {
      setEnvironment(id);
      localStorage.setItem("rustfrp-environment", String(id));
    },
  };
}
const SettingsContext = createContext<ReturnType<typeof useSettings> | null>(
  null,
);
export function PreferencesProvider({ children }: { children: ReactNode }) {
  return (
    <SettingsContext.Provider value={useSettings()}>
      {children}
    </SettingsContext.Provider>
  );
}
export function usePreferences() {
  const context = useContext(SettingsContext);
  if (!context) throw new Error("Missing PreferencesProvider");
  return context;
}
