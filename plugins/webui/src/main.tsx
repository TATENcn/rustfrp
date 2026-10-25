import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { HashRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { I18nProvider } from "@/i18n";
import { PreferencesProvider } from "@/hooks/preferences";
import Router from "@/app/router";
import "@/styles/app.css";
const client = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 1500, refetchIntervalInBackground: false },
    mutations: { retry: false },
  },
});
createRoot(document.getElementById("app")!).render(
  <StrictMode>
    <QueryClientProvider client={client}>
      <I18nProvider>
        <PreferencesProvider>
          <HashRouter>
            <Router />
          </HashRouter>
        </PreferencesProvider>
      </I18nProvider>
    </QueryClientProvider>
  </StrictMode>,
);
