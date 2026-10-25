import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";
import {
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart";
import { EmptyState } from "@/components/common/page";
import { useI18n } from "@/i18n";
export type ChartPoint = { timestamp: number; [key: string]: number };
export function MetricsChart({
  points,
  config,
  format,
  label,
  domain,
}: {
  points: ChartPoint[];
  config: ChartConfig;
  format: (value: number) => string;
  label: string;
  domain?: [number, number];
}) {
  const { locale, text } = useI18n();
  if (!points.length)
    return (
      <EmptyState
        icon="activity"
        title={text("等待指标数据", "Waiting for metrics")}
        description={text(
          "收到采样后，将自动显示趋势。",
          "Trends will appear as samples arrive.",
        )}
      />
    );
  return (
    <ChartContainer
      config={config}
      className="aspect-auto h-[280px] w-full"
      aria-label={label}
    >
      <LineChart
        accessibilityLayer
        data={points}
        margin={{ left: 0, right: 16, top: 12, bottom: 0 }}
      >
        <CartesianGrid vertical={false} strokeDasharray="3 3" />
        <XAxis
          dataKey="timestamp"
          type="number"
          domain={["dataMin", "dataMax"]}
          tickFormatter={(v) =>
            new Date(v).toLocaleTimeString(locale, {
              hour: "2-digit",
              minute: "2-digit",
                second: (points.at(-1)!.timestamp - points[0]!.timestamp < 300_000) ? "2-digit" : undefined,
            })
          }
          tickLine={false}
          axisLine={false}
          minTickGap={45}
          tickMargin={12}
        />
        <YAxis
          width={72}
          domain={domain ?? [0, "auto"]}
          tickFormatter={(value) => format(value).replaceAll(" ", "\u00a0")}
          tick={{ fontSize: 11 }}
          tickLine={false}
          axisLine={false}
          tickMargin={8}
        />
        <ChartTooltip
          cursor={{ stroke: "var(--border)" }}
          content={
            <ChartTooltipContent
              labelFormatter={(_, payload) =>
                new Date(payload[0]?.payload.timestamp ?? 0).toLocaleTimeString(
                  locale,
                )
              }
              formatter={(value, name) => (
                <div className="flex min-w-40 justify-between gap-5">
                  <span className="text-muted-foreground">
                    {config[String(name)]?.label ?? String(name)}
                  </span>
                  <span className="font-mono font-medium">
                    {format(Number(value))}
                  </span>
                </div>
              )}
            />
          }
        />
        <ChartLegend content={<ChartLegendContent />} />
        {Object.keys(config).map((key, index) => (
          <Line
            key={key}
            dataKey={key}
            type="monotone"
            stroke={`var(--color-${key})`}
            strokeWidth={2}
            strokeDasharray={index === 1 ? "4 3" : undefined}
            dot={points.length === 1 ? { r: 3 } : false}
            activeDot={{ r: 4 }}
            isAnimationActive={false}
            connectNulls={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  );
}
