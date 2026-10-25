import { useSearchParams } from "react-router-dom";
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { data, useAction, useStatus } from "@/hooks/api";
import { getReloadStatus, triggerReload } from "@/api/system";
import { useI18n } from "@/i18n";
import { formatDuration } from "@/i18n/format";
import { AppIcon } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert";
import {
  ActionButton,
  ErrorState,
  PageHeading,
} from "@/components/common/page";
import { ProcessTable } from "@/components/common/process-table";
import { Versions } from "./versions";
import { Transfer } from "./transfer";
export default function Settings() {
  const [params, setParams] = useSearchParams();
  const { t, text, locale } = useI18n(),
    status = useStatus();
  const [task, setTask] = useState<string | null>(null);
  const reload = useAction(async () => {
    const result = await data(triggerReload());
    setTask(result.task_id);
  });
  const progress = useQuery({
    queryKey: ["reload", task],
    queryFn: () => data(getReloadStatus(task!)),
    enabled: !!task,
    refetchInterval: (q) =>
      q.state.data?.status === "completed" || q.state.data?.status === "failed"
        ? false
        : 2000,
  });
  const pending =
    reload.isPending ||
    (!!task &&
      (!progress.data || progress.data.status === "running") &&
      !progress.error);
  return (
    <div className="space-y-6">
      <PageHeading
        title={t("nav.status")}
        description={text(
          "管理服务运行、FRP 版本与配置备份。",
          "Manage your service, FRP versions, and configuration backups.",
        )}
      >
        <Button
          variant="outline"
          disabled={status.isFetching}
          onClick={() => void status.refetch()}
        >
          <AppIcon name="refresh" />
          {t("common.refresh")}
        </Button>
      </PageHeading>
      <Tabs
        value={params.get("tab") ?? "runtime"}
        onValueChange={(tab) => setParams({ tab })}
        className="gap-6"
      >
        <TabsList className="max-w-full">
          <TabsTrigger value="runtime">
            {text("运行状态", "Runtime")}
          </TabsTrigger>
          <TabsTrigger value="versions">
            {text("FRP 版本", "FRP versions")}
          </TabsTrigger>
          <TabsTrigger value="transfer">
            {text("导入导出", "Import & export")}
          </TabsTrigger>
        </TabsList>
        <TabsContent value="runtime" className="space-y-6">
          <ErrorState
            error={status.error}
            retry={() => void status.refetch()}
          />
          <Card className="shadow-none">
            <CardHeader className="flex flex-row flex-wrap items-start justify-between gap-4">
              <div className="space-y-1.5">
                <CardTitle>{text("服务概览", "Service overview")}</CardTitle>
                <CardDescription>
                  {text(
                    "查看当前配置规模及服务运行时间。",
                    "Configuration totals and current service uptime.",
                  )}
                </CardDescription>
              </div>
              <ActionButton
                variant="outline"
                busy={pending}
                onClick={() => reload.mutate()}
              >
                <AppIcon name="refresh" />
                {t("app.reload")}
              </ActionButton>
            </CardHeader>
            <CardContent>
              <dl className="grid grid-cols-2 gap-x-8 gap-y-6 lg:grid-cols-4">
                {[
                  [
                    text("运行时间", "Uptime"),
                    status.data
                      ? formatDuration(status.data.uptime_secs, locale)
                      : "—",
                  ],
                  [
                    text("服务器", "Profiles"),
                    status.data?.total_profiles ?? "—",
                  ],
                  [
                    text("代理规则", "Proxies"),
                    status.data?.total_proxies ?? "—",
                  ],
                  [
                    text("访问者", "Visitors"),
                    status.data?.total_visitors ?? "—",
                  ],
                ].map(([label, value]) => (
                  <div key={label}>
                    <dt className="text-sm text-muted-foreground">{label}</dt>
                    <dd className="mt-2 text-2xl font-semibold tracking-tight">
                      {value}
                    </dd>
                  </div>
                ))}
              </dl>
            </CardContent>
          </Card>
          {task && (
            <Alert>
              <AppIcon name="info" />
              <AlertTitle>
                {progress.data?.status === "completed"
                  ? text("配置已重新加载", "Configuration reloaded")
                  : progress.data?.status === "failed"
                    ? text("重载失败", "Reload failed")
                    : text("正在重新加载配置", "Reloading configuration")}
              </AlertTitle>
              <AlertDescription>
                {text("影响的服务器数量：", "Profiles affected: ")}
                {progress.data?.profiles_affected ?? "—"}
              </AlertDescription>
            </Alert>
          )}
          <ErrorState
            error={
              progress.error ||
              (progress.data?.errors.length
                ? new Error(progress.data.errors.join(", "))
                : null)
            }
          />
          <div className="space-y-4">
            <div>
              <h2 className="text-base font-semibold">
                {text("FRP 进程", "FRP processes")}
              </h2>
              <p className="mt-1 text-sm text-muted-foreground">
                {text(
                  "查看每个服务器的进程与连接状态。",
                  "Process and connection health for each server.",
                )}
              </p>
            </div>
            <ProcessTable processes={status.data?.processes ?? []} />
          </div>
        </TabsContent>
        <TabsContent value="versions">
          <Versions />
        </TabsContent>
        <TabsContent value="transfer">
          <Transfer />
        </TabsContent>
      </Tabs>
    </div>
  );
}
