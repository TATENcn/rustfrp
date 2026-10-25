import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { getMetricsHistory } from "@/api/metrics";
import { data, useEnvironments, useStatus } from "@/hooks/api";
import { usePreferences } from "@/hooks/preferences";
import { useI18n } from "@/i18n";
import { formatBytes, formatDuration, formatPercent } from "@/i18n/format";
import { extrapolateUptime } from "@/stores/uptime";
import { AppIcon } from "@/components/icon/AppIcon";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  Choice,
  ErrorState,
  PageHeading,
  PageLoading,
} from "@/components/common/page";
import { ProcessSidebar } from "./process-sidebar";
import { MetricsChart, type ChartPoint } from "./metrics-chart";
import { TrafficPanel } from "./traffic-panel";
export default function Overview() {
  const { t, text, locale } = useI18n(),
    status = useStatus(),
    prefs = usePreferences(),
    environments = useEnvironments();
  const [period, setPeriod] = useState("60"),
    [metric, setMetric] = useState("cpu"),
    [now, setNow] = useState(Date.now());
  const environment =
    environments.data?.find((e) => e.id === prefs.environmentId) ??
    environments.data?.find((e) => e.is_default);
  const metrics = useQuery({
    queryKey: ["metrics", environment?.id],
    queryFn: () => data(getMetricsHistory(environment?.id)),
    refetchInterval: (q) => (q.state.fetchFailureCount ? 30000 : 5000),
  });
  useEffect(() => {
    const timer = setInterval(() => {
      if (!document.hidden) setNow(Date.now());
    }, 1000);
    return () => clearInterval(timer);
  }, []);
  const samples = metrics.data?.resources ?? [];
  const latest = samples.at(-1);
  const cutoff =
    (latest ? Date.parse(latest.timestamp) : now) - Number(period) * 60000;
  const resourcePoints: ChartPoint[] = samples
    .filter((s) => Date.parse(s.timestamp) >= cutoff)
    .map((s) => ({
      timestamp: Date.parse(s.timestamp),
      host:
        metric === "cpu" ? s.system_cpu_percent : s.system_memory_used_bytes,
      daemon: metric === "cpu" ? s.daemon_cpu_percent : s.daemon_memory_bytes,
    }));
  const refresh = () => {
    void status.refetch();
    void metrics.refetch();
    void environments.refetch();
  };
  return (
    <div className="space-y-6">
      <PageHeading
        title={t("nav.dashboard")}
        description={text(
          "查看连接、资源占用和网络流量。",
          "Monitor your connections, resources, and network traffic.",
        )}
      >
        <Button
          variant="outline"
          disabled={status.isFetching || metrics.isFetching}
          onClick={refresh}
        >
          <AppIcon name="refresh" />
          {t("common.refresh")}
        </Button>
        <Button asChild>
          <Link to="/profiles/new">
            <AppIcon name="plus" />
            {t("profile.create")}
          </Link>
        </Button>
      </PageHeading>
      <ErrorState
        error={status.error || metrics.error || environments.error}
        retry={refresh}
      />
      {status.isPending ? (
        <PageLoading />
      ) : (
        <>
          <div className="grid min-w-0 items-start gap-6 xl:grid-cols-[minmax(0,1fr)_320px]">
            <aside
              aria-label={text("运行进程", "Processes")}
              className="min-w-0 xl:order-last xl:sticky xl:top-6"
            >
              <ProcessSidebar processes={status.data?.processes ?? []} />
            </aside>
            <div className="min-w-0 space-y-6">
              <div className="grid grid-cols-2 gap-3 lg:gap-4 2xl:grid-cols-4">
                {[
                  {
                    label: text("运行中的实例", "Active instances"),
                    value: String(status.data?.active_frpc_instances ?? "—"),
                    hint: text(
                      `共 ${status.data?.total_profiles ?? 0} 个服务器配置`,
                      `Across ${status.data?.total_profiles ?? 0} profiles`,
                    ),
                    icon: "activity" as const,
                  },
                  {
                    label: text("代理规则", "Proxy rules"),
                    value: String(status.data?.total_proxies ?? "—"),
                    hint: text(
                      `${status.data?.total_bindings ?? 0} 条分配关系`,
                      `${status.data?.total_bindings ?? 0} assignments configured`,
                    ),
                    icon: "proxies" as const,
                  },
                  {
                    label: text("系统 CPU", "System CPU"),
                    value: latest
                      ? formatPercent(latest.system_cpu_percent, locale)
                      : "—",
                    hint: latest
                      ? text(
                          `内存 ${formatBytes(latest.system_memory_used_bytes, locale)}`,
                          `${formatBytes(latest.system_memory_used_bytes, locale)} memory in use`,
                        )
                      : text("等待采样", "Waiting for samples"),
                    icon: "cpu" as const,
                  },
                  {
                    label: text("运行时间", "Uptime"),
                    value: status.data
                      ? formatDuration(
                          extrapolateUptime(
                            status.data.uptime_secs,
                            status.dataUpdatedAt,
                            now,
                          ),
                          locale,
                          { style: "narrow" },
                        )
                      : "—",
                    hint: text("自服务启动以来", "Since the service started"),
                    icon: "clock" as const,
                  },
                ].map((item) => (
                  <Card key={item.label} className="gap-3 py-5 shadow-none">
                    <CardHeader className="flex flex-row items-center justify-between space-y-0 px-4 sm:px-6">
                      <CardDescription>{item.label}</CardDescription>
                      <AppIcon
                        name={item.icon}
                        className="text-muted-foreground"
                      />
                    </CardHeader>
                    <CardContent className="px-4 sm:px-6">
                      <p className="text-xl font-semibold tracking-tight tabular-nums sm:text-2xl">
                        {item.value}
                      </p>
                      <p className="mt-2 text-xs text-muted-foreground">
                        {item.hint}
                      </p>
                    </CardContent>
                  </Card>
                ))}
              </div>
              <Card className="shadow-none">
                <CardHeader className="gap-4 sm:flex sm:flex-row sm:flex-wrap sm:items-start sm:justify-between">
                  <div className="space-y-1">
                    <CardTitle>
                      {text("资源趋势", "Resource activity")}
                    </CardTitle>
                    <CardDescription>
                      {text(
                        "对比主机与 RustFRP 服务的资源使用情况。",
                        "Host and RustFRP resource usage over time.",
                      )}
                    </CardDescription>
                  </div>
                  <div className="flex flex-wrap gap-3">
                    <Tabs value={metric} onValueChange={setMetric}>
                      <TabsList>
                        <TabsTrigger value="cpu">CPU</TabsTrigger>
                        <TabsTrigger value="memory">
                          {text("内存", "Memory")}
                        </TabsTrigger>
                      </TabsList>
                    </Tabs>
                    <Choice
                      label={text("时间范围", "Time range")}
                      value={period}
                      options={[
                        {
                          value: "15",
                          label: text("最近 15 分钟", "Last 15 minutes"),
                        },
                        {
                          value: "60",
                          label: text("最近 1 小时", "Last hour"),
                        },
                        {
                          value: "5",
                          label: text("最近 5 分钟", "Last 5 minutes"),
                        },
                      ]}
                      onChange={setPeriod}
                      className="w-40"
                    />
                  </div>
                </CardHeader>
                <CardContent>
                  <MetricsChart
                    points={resourcePoints}
                    config={{
                      host: {
                        label: text("主机", "Host"),
                        color: "var(--chart-1)",
                      },
                      daemon: { label: "RustFRP", color: "var(--chart-3)" },
                    }}
                    format={(n) =>
                      metric === "cpu"
                        ? formatPercent(n, locale)
                        : formatBytes(n, locale)
                    }
                    domain={metric === "cpu" ? [0, 100] : undefined}
                    label={text("资源使用趋势", "Resource usage chart")}
                  />
                </CardContent>
              </Card>
              <div className="grid min-w-0 gap-6">
                <TrafficPanel
                  key={environment?.id}
                  history={metrics.data}
                  cutoff={cutoff}
                  period={period}
                  onPeriodChange={setPeriod}
                  processes={status.data?.processes ?? []}
                />
              </div>
            </div>
          </div>
          <p className="text-xs text-muted-foreground">
            {text("每 5 秒更新指标", "Metrics update every 5 seconds")} ·{" "}
            {status.dataUpdatedAt
              ? new Date(status.dataUpdatedAt).toLocaleTimeString(locale)
              : "—"}
          </p>
        </>
      )}
    </div>
  );
}
