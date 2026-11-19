/**
 * Certificate ingest endpoint settings.
 *
 * The endpoint ships enabled on loopback: it only accepts a token, returns no
 * certificate material, and can only write a certificate for domains that token
 * was issued for. So the default that "just works" for a local Certd is on, and
 * the switch that widens it beyond this machine is explicit.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { apiGet, apiPut } from "@/api/client";
import type { ApiEnvelope } from "@/api/types";
import { extractApiError } from "@/api/errors";
import { data } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";

export interface IngestSettings {
  enabled: boolean;
  expose: boolean;
  port: number;
  listen: string;
  /** Address actually bound right now, or null when not listening. */
  address: string | null;
  /** Last bind failure, e.g. the port is taken. */
  error: string | null;
  /** Ready-to-copy endpoint when the listener is bound. */
  url: string | null;
}

export function getIngestSettings(): Promise<ApiEnvelope<IngestSettings>> {
  return apiGet<IngestSettings>("/cert-ingest");
}

export function updateIngestSettings(body: {
  enabled?: boolean;
  expose?: boolean;
  port?: number;
}): Promise<ApiEnvelope<IngestSettings>> {
  return apiPut<IngestSettings>("/cert-ingest", body);
}

export function IngestSettingsCard() {
  const { text } = useI18n();
  const client = useQueryClient();

  const settings = useQuery({
    queryKey: ["cert-ingest"],
    queryFn: () => data(getIngestSettings()),
  });

  // Applies immediately: the daemon rebinds the listener in place, so there is
  // no "restart the daemon to apply" step to get wrong.
  const update = useMutation({
    mutationFn: (body: { enabled?: boolean; expose?: boolean; port?: number }) =>
      data(updateIngestSettings(body)),
    onSuccess: (result) => {
      if (result.error) toast.error(result.error);
      else toast.success(text("接收端点已更新", "Ingest endpoint updated"));
      void client.invalidateQueries({ queryKey: ["cert-ingest"] });
    },
    onError: (error) => toast.error(extractApiError(error).message),
  });

  const current = settings.data;
  if (!current) return null;

  const bound = current.address;
  const origin = window.location.hostname;
  const url = bound
    ? `http://${current.expose ? origin : bound.split(":")[0]}:${current.port}/ingest/v1/certs`
    : null;

  return (
    <div className="space-y-4 rounded-lg border p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="space-y-1">
          <h3 className="flex items-center gap-2 text-sm font-semibold">
            {text("接收端点（推送）", "Ingest endpoint (push)")}
            <Badge variant={bound ? "secondary" : "outline"}>
              {bound ? text("监听中", "Listening") : text("已停止", "Stopped")}
            </Badge>
          </h3>
          <p className="text-sm text-muted-foreground">
            {text(
              "只有两条路由：推送证书与健康检查。没有 token 一律拒绝，且证书只能覆盖该令牌允许的域名。",
              "Two routes only: push a certificate and a health check. Every request needs a token, and a certificate may only cover the domains that token allows.",
            )}
          </p>
        </div>
      </div>

      {current.error && (
        <p className="rounded-md border border-destructive/40 bg-destructive/5 p-2 text-xs text-destructive">
          {current.error}
        </p>
      )}

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="flex items-center justify-between gap-3 rounded-md border p-3">
          <div className="space-y-0.5">
            <Label htmlFor="ingest-enabled">{text("启用", "Enabled")}</Label>
            <p className="text-xs text-muted-foreground">
              {text("默认开启，仅本机可访问。", "On by default; loopback only.")}
            </p>
          </div>
          <Switch
            id="ingest-enabled"
            checked={current.enabled}
            disabled={update.isPending}
            onCheckedChange={(enabled) => update.mutate({ enabled })}
          />
        </div>
        <div className="flex items-center justify-between gap-3 rounded-md border p-3">
          <div className="space-y-0.5">
            <Label htmlFor="ingest-expose">
              {text("允许其它主机访问", "Reachable from other hosts")}
            </Label>
            <p className="text-xs text-muted-foreground">
              {text(
                "开启后监听 0.0.0.0，Certd 才能真正连进来。",
                "Binds 0.0.0.0 so Certd can actually connect.",
              )}
            </p>
          </div>
          <Switch
            id="ingest-expose"
            checked={current.expose}
            disabled={update.isPending}
            onCheckedChange={(expose) => update.mutate({ expose })}
          />
        </div>
      </div>

      <div className="flex flex-wrap items-end gap-2">
        <div className="space-y-1.5">
          <Label htmlFor="ingest-port">{text("端口", "Port")}</Label>
          <Input
            id="ingest-port"
            type="number"
            min={1}
            max={65535}
            className="h-9 w-28"
            defaultValue={current.port}
            disabled={update.isPending}
            onBlur={(event) => {
              const port = Number(event.target.value);
              if (port && port !== current.port) update.mutate({ port });
            }}
          />
        </div>
        {!current.expose && (
          <p className="pb-2 text-xs text-muted-foreground">
            {text(
              "当前只在本机监听，Docker / 桌面端外部访问需打开上面的开关。",
              "Loopback only; enable the switch above for Docker or an external Certd.",
            )}
          </p>
        )}
      </div>

      {url && (
        <div className="space-y-1">
          <Label className="text-xs">{text("推送地址", "Push URL")}</Label>
          <code className="block break-all rounded bg-muted p-2 font-mono text-xs">{url}</code>
        </div>
      )}

      <Button
        variant="outline"
        size="sm"
        disabled={settings.isFetching}
        onClick={() => void settings.refetch()}
      >
        {text("刷新状态", "Refresh")}
      </Button>
    </div>
  );
}
