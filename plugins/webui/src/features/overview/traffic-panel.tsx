import { useState } from "react";
import { useProfiles } from "@/hooks/api";
import { Choice } from "@/components/common/page";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Link } from "react-router-dom";
import type { MetricsHistory, ProcessInfo } from "@/api/types";
import { useI18n } from "@/i18n";
import { formatBytes } from "@/i18n/format";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/common/page";
import { MetricsChart } from "./metrics-chart";
import { trafficAnalysis } from "./traffic-data";

export function TrafficPanel({
  history,
  cutoff,
  processes,
  period,
  onPeriodChange,
}: {
  history?: MetricsHistory;
  cutoff: number;
  period: string;
  onPeriodChange: (value: string) => void;
  processes: ProcessInfo[];
}) {
  const { text, locale } = useI18n();
  const [scope, setScope] = useState("server");
  const [server, setServer] = useState("all");
  const [detail, setDetail] = useState("all");
  const configurations = useProfiles();
  const collection = history?.traffic_collection;
  const profiles =
    (scope === "proxy" ? collection?.proxies : collection?.profiles)?.filter(
      (p) => server === "all" || String(p.profile_id) === server,
    ) ?? [];
  const collecting = profiles.filter((p) => p.state === "collecting");
  const issues = profiles.filter(
    (p) =>
      !["stopped", "collecting"].includes(p.state) &&
      (detail === "all" || !("proxy_id" in p) || String(p.proxy_id) === detail),
  );
  const raw =
    scope === "host"
      ? (history?.host_traffic ?? [])
      : scope === "proxy"
        ? (history?.proxy_traffic ?? [])
        : (history?.traffic ?? []);
  const scoped = raw.filter(
    (s) =>
      scope === "host" || server === "all" || String(s.profile_id) === server,
  );
  const detailKey = (s: (typeof raw)[number]) =>
    scope === "host"
      ? (s.interface ?? "")
      : scope === "proxy"
        ? String(s.proxy_id)
        : String(s.profile_id);
  const selected = scoped.filter(
    (s) => detail === "all" || detailKey(s) === detail,
  );
  const { points, receivedTotal, sentTotal } = trafficAnalysis(
    selected,
    cutoff,
  );
  const serverName = (id: number) =>
    configurations.data?.find((p) => p.id === id)?.name ??
    processes.find((p) => p.profile_id === id)?.profile_name ??
    `#${id}`;
  const nameFor = (key: string) =>
    scope === "host"
      ? key
      : scope === "proxy"
        ? (collection?.proxies?.find((p) => String(p.proxy_id) === key)?.name ??
          `#${key}`)
        : serverName(Number(key));
  const keys = [
    ...new Set([
      ...scoped.map(detailKey),
      ...(scope === "proxy"
        ? profiles
            .filter((p) => "proxy_id" in p)
            .map((p) => String("proxy_id" in p ? p.proxy_id : ""))
        : []),
    ]),
  ];
  const rows = keys
    .filter((key) => detail === "all" || key === detail)
    .map((key) => ({
      key,
      ...trafficAnalysis(
        scoped.filter((s) => detailKey(s) === key),
        cutoff,
      ),
    }));
  const first = selected.filter((s) => Date.parse(s.timestamp) >= cutoff).at(0);
  const serverIds = [
    ...new Set([
      ...(collection?.profiles ?? []).map((p) => p.profile_id),
      ...(history?.traffic ?? []).map((s) => s.profile_id),
    ]),
  ];
  const labels: Record<string, string> = {
    ambiguous_endpoint: text(
      "多个配置共用目标地址，无法准确归属",
      "Shared endpoint: attribution is ambiguous",
    ),
    unsupported_proxy: text(
      "UDP 或插件代理暂不支持本地统计",
      "UDP and plugin proxies are not supported",
    ),
    unsupported_transport: text(
      "该传输协议或上游代理暂不支持本地采集",
      "Transport or upstream proxy is not supported",
    ),
    collector_unavailable: text(
      "系统不允许读取 TCP 计数",
      "TCP counters are unavailable on this system",
    ),
    process_unavailable: text(
      "暂时无法读取进程连接",
      "Process connections are temporarily unavailable",
    ),
    resolve_failed: text(
      "无法解析服务器地址",
      "Server address could not be resolved",
    ),
    no_connection: text(
      "未检测到连接服务器的 TCP 会话",
      "No TCP connection to the server detected",
    ),
  };
  const unavailable =
    collection && !["ready", "initializing"].includes(collection.state);
  const stale =
    collection?.updated_at &&
    Date.now() - Date.parse(collection.updated_at) > 15000;
  const title = unavailable
    ? text("流量采集不可用", "Traffic collection unavailable")
    : stale
      ? text("流量采集已暂停", "Traffic collection paused")
      : collecting.length
        ? text("正在采集流量", "Collecting traffic")
        : issues.length
          ? text("尚未采集到流量", "No traffic samples available")
          : profiles.length
            ? text(
                "启动连接后采集流量",
                "Start a connection to collect traffic",
              )
            : text("暂无可监控的连接", "No connections to monitor");
  return (
    <Card className="min-w-0 shadow-none">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle>{text("网络流量", "Network traffic")}</CardTitle>
          <Badge variant="outline">
            {scope === "host" ? (
              text(
                "整机 / 当前网络命名空间",
                "Host / current network namespace",
              )
            ) : (
              <>
                {collecting.length
                  ? text(
                      `${collecting.length} 个实例采集中`,
                      `${collecting.length} collecting`,
                    )
                  : text("未采集", "Not collecting")}
              </>
            )}
          </Badge>
        </div>
        <CardDescription>
          {scope === "host"
            ? text(
                "网卡收发包含此主机所有应用，不受工作环境筛选限制。",
                "Interface traffic includes all applications, independent of the selected environment.",
              )
            : scope === "proxy"
              ? text(
                  "入站为送达本地服务的数据，出站为本地服务返回的数据。",
                  "Inbound is delivered to the local service; outbound is returned by it.",
                )
              : text(
                  "到 FRP 服务器的 TCP 收发速率（含心跳与加密开销）。",
                  "TCP transfer rate to FRP servers, including heartbeats and encryption overhead.",
                )}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
          <Choice
            label={text("流量时间范围", "Traffic time range")}
            value={period}
            onChange={onPeriodChange}
            options={[
              { value: "5", label: text("最近 5 分钟", "Last 5 minutes") },
              { value: "15", label: text("最近 15 分钟", "Last 15 minutes") },
              { value: "60", label: text("最近 1 小时", "Last hour") },
            ]}
          />
          <Choice
            label={text("统计范围", "Traffic scope")}
            value={scope}
            options={[
              {
                value: "server",
                label: text("上游服务器", "Upstream servers"),
              },
              { value: "proxy", label: text("本地代理", "Local proxies") },
              { value: "host", label: text("系统网卡", "Host interfaces") },
            ]}
            onChange={(value) => {
              setScope(value);
              setDetail("all");
            }}
          />
          {scope !== "host" && (
            <Choice
              label={text("服务器筛选", "Traffic server")}
              value={server}
              onChange={(value) => {
                setServer(value);
                setDetail("all");
              }}
              options={[
                { value: "all", label: text("全部服务器", "All servers") },
                ...serverIds.map((id) => ({
                  value: String(id),
                  label: serverName(id),
                })),
              ]}
            />
          )}
          {scope !== "server" && (
            <Choice
              label={text("明细筛选", "Traffic detail")}
              value={detail}
              onChange={setDetail}
              options={[
                { value: "all", label: text("全部", "All") },
                ...keys.map((key) => ({ value: key, label: nameFor(key) })),
              ]}
            />
          )}
        </div>
        <div className="grid grid-cols-2 gap-3 rounded-lg border p-3">
          <div>
            <p className="text-xs text-muted-foreground">
              {text("区间入站", "Window inbound")}
            </p>
            <p className="text-lg font-semibold tabular-nums">
              {points.length ? formatBytes(receivedTotal, locale) : "—"}
            </p>
          </div>
          <div>
            <p className="text-xs text-muted-foreground">
              {text("区间出站", "Window outbound")}
            </p>
            <p className="text-lg font-semibold tabular-nums">
              {points.length ? formatBytes(sentTotal, locale) : "—"}
            </p>
          </div>
        </div>
        {first && (
          <p className="text-xs text-muted-foreground">
            {text("当前缓存起点：", "Available samples from: ")}
            {new Date(first.timestamp).toLocaleString(locale)}
          </p>
        )}
        {scope === "host" && collection?.host_state !== "collecting" && (
          <p role="status" className="text-sm text-muted-foreground">
            {text(
              "网卡采集不可用或正在初始化",
              "Interface collection unavailable or initializing",
            )}
          </p>
        )}
        {points.length ? (
          <MetricsChart
            points={points}
            config={{
              received: {
                label: text("接收", "Received"),
                color: "var(--chart-1)",
              },
              sent: { label: text("发送", "Sent"), color: "var(--chart-3)" },
            }}
            format={(n) => formatBytes(n, locale) + "/s"}
            label={text("网络流量趋势", "Network traffic chart")}
          />
        ) : (
          <EmptyState
            icon="activity"
            title={
              scope === "host"
                ? text("暂无网卡采样", "No interface samples")
                : title
            }
            description={
              collecting.length
                ? text(
                    "首个采样作为基线，下一次采样后显示速率；也可扩大时间范围。",
                    "The first sample establishes a baseline. Rates appear with the next sample; you can also widen the time range.",
                  )
                : unavailable
                  ? text(
                      "自动采集需要 Linux TCP 诊断接口；不支持的平台不会显示虚假的零流量。",
                      "Automatic collection requires Linux TCP diagnostics. Unsupported systems do not report false zero traffic.",
                    )
                  : text(
                      "请检查代理分配页的连接状态。",
                      "Check connection status on the assignments page.",
                    )
            }
          >
            {!collecting.length && (
              <Button asChild variant="outline">
                <Link to="/bindings">
                  {text("查看连接", "View connections")}
                </Link>
              </Button>
            )}
          </EmptyState>
        )}
        {(unavailable || stale) && points.length > 0 && (
          <p role="status" className="text-sm text-muted-foreground">
            {title}
          </p>
        )}
        {!!rows.length && (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{text("统计对象", "Source")}</TableHead>
                <TableHead className="text-right">
                  {text("区间入站", "Inbound")}
                </TableHead>
                <TableHead className="text-right">
                  {text("区间出站", "Outbound")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((row) => (
                <TableRow key={row.key}>
                  <TableCell>{nameFor(row.key)}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {row.points.length
                      ? formatBytes(row.receivedTotal, locale)
                      : "—"}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {row.points.length
                      ? formatBytes(row.sentTotal, locale)
                      : "—"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        {scope !== "host" && !!issues.length && (
          <ul className="space-y-1 text-xs text-muted-foreground">
            {issues.map((p) => (
              <li
                key={`${p.profile_id}-${"proxy_id" in p ? p.proxy_id : "server"}`}
              >
                {processes.find(
                  (process) => process.profile_id === p.profile_id,
                )?.profile_name ?? `#${p.profile_id}`}
                {"name" in p ? ` / ${p.name}` : ""}：
                {labels[p.state] ?? p.state}
              </li>
            ))}
          </ul>
        )}
        <p className="text-xs leading-5 text-muted-foreground">
          {text(
            "每秒采样，5 秒刷新；汇总仅覆盖有效缓存区间，重启清空。三种统计口径不可相加。",
            "Sampled every second; refreshed every 5 seconds. Totals cover valid cached intervals and reset on restart. Do not add scopes together.",
          )}{" "}
          {scope === "host"
            ? text(
                "排除 lo；虚拟网卡可能重复计数，建议选择出口网卡。",
                "Excludes lo; virtual interfaces may double-count. Select your egress interface.",
              )
            : text(
                "TCP 短连接可能漏采，不用于精确计费。",
                "Short TCP connections may be missed; not for billing.",
              )}
        </p>
      </CardContent>
    </Card>
  );
}
