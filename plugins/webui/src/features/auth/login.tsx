import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { getStatus } from "@/api/system";
import { data } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { AppIcon } from "@/components/icon/AppIcon";
import { ActionButton, ErrorState } from "@/components/common/page";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
export default function Login() {
  const { t, text } = useI18n(),
    navigate = useNavigate(),
    client = useQueryClient();
  const [token, setToken] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>();
  return (
    <div className="flex min-h-svh flex-col items-center justify-center bg-muted/40 p-6">
      <div className="w-full max-w-sm space-y-6">
        <div className="flex items-center justify-center gap-2 text-sm font-semibold">
          <span className="rounded-md bg-primary p-2 text-primary-foreground">
            <AppIcon name="proxies" />
          </span>
          RustFRP
        </div>
        <Card>
          <CardHeader className="text-center">
            <CardTitle className="text-xl">
              {text("登录控制台", "Welcome back")}
            </CardTitle>
            <CardDescription>
              {text(
                "使用 API Token 访问你的网络配置。",
                "Enter your API token to manage your network.",
              )}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form
              className="space-y-5"
              onSubmit={async (e) => {
                e.preventDefault();
                if (busy || !token.trim()) return;
                setBusy(true);
                setError(undefined);
                localStorage.setItem("api_token", token.trim());
                try {
                  await data(getStatus());
                  client.clear();
                  navigate("/", { replace: true });
                } catch (reason) {
                  localStorage.removeItem("api_token");
                  setError(reason);
                } finally {
                  setBusy(false);
                }
              }}
            >
              <ErrorState error={error} />
              <Field>
                <FieldLabel htmlFor="token">{t("auth.tokenLabel")}</FieldLabel>
                <Input
                  id="token"
                  type="password"
                  autoComplete="current-password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  required
                  disabled={busy}
                />
                <FieldDescription>
                  {text(
                    "请输入服务端配置的访问令牌。",
                    "Use the access token configured on your server.",
                  )}
                </FieldDescription>
              </Field>
              <ActionButton className="w-full" type="submit" busy={busy}>
                {t("auth.login")}
              </ActionButton>
            </form>
          </CardContent>
        </Card>
        <p className="text-center text-xs text-muted-foreground">
          {text(
            "集中管理连接，让服务触手可及。",
            "Your connections, all in one place.",
          )}
        </p>
      </div>
    </div>
  );
}
