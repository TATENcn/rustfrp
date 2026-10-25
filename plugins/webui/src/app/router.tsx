import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { Toaster } from "sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { PageLoading } from "@/components/common/page";
import { usePreferences } from "@/hooks/preferences";
import Shell from "./shell";
const Overview = lazy(() => import("@/features/overview/page"));
const Resources = lazy(() => import("@/features/resources/page"));
const Bindings = lazy(() => import("@/features/bindings/page"));
const Logs = lazy(() => import("@/features/logs/page"));
const Settings = lazy(() => import("@/features/settings/page"));
const Login = lazy(() => import("@/features/auth/login"));
export default function Router() {
  const { dark } = usePreferences();
  return (
    <TooltipProvider>
      <Suspense
        fallback={
          <div className="p-8">
            <PageLoading />
          </div>
        }
      >
        <Routes>
          <Route path="login" element={<Login />} />
          <Route path="/" element={<Shell />}>
            <Route index element={<Overview />} />
            {(["profiles", "proxies", "visitors"] as const).map((kind) => (
              <Route
                key={kind}
                path={`${kind}/*`}
                element={<Resources key={kind} kind={kind} />}
              />
            ))}
            <Route path="bindings" element={<Bindings />} />
            <Route path="logs" element={<Logs />} />
            <Route path="status" element={<Settings />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </Suspense>
      <Toaster theme={dark ? "dark" : "light"} closeButton />
    </TooltipProvider>
  );
}
