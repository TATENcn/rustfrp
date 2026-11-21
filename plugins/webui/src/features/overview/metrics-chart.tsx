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

/**
 * Upper bound on points actually drawn.
 *
 * The sampler produces one point per second, so an hour is 3 600 points across
 * roughly 900 CSS pixels: four samples compete for every pixel and the line
 * reads as noise rather than a trend. Averaging fixed-width buckets both
 * smooths the curve and cuts the render work. A bucket keeps the mean, which
 * is the right summary for utilisation — a single 100% sample between 3599 idle
 * ones is not a trend, it is a blip.
 */
const MAX_RENDERED_POINTS = 240;

export function bucketAverages(points: ChartPoint[]): ChartPoint[] {
  if (points.length <= MAX_RENDERED_POINTS) return points;
  const size = points.length / MAX_RENDERED_POINTS;
  const out: ChartPoint[] = [];
  for (let bucket = 0; bucket < MAX_RENDERED_POINTS; bucket += 1) {
    const start = Math.floor(bucket * size);
    const end = Math.min(Math.floor((bucket + 1) * size), points.length);
    if (end <= start) continue;
    const slice = points.slice(start, end);
    const last = slice.at(-1);
    const first = slice.at(0);
    if (!last || !first) continue;
    // The timestamp is the bucket's last sample, so the series stays anchored
    // to real time instead of drifting half a bucket behind the data.
    const merged: ChartPoint = { timestamp: last.timestamp };
    for (const key of Object.keys(first)) {
      if (key === "timestamp") continue;
      let sum = 0;
      let count = 0;
      for (const point of slice) {
        const value = point[key];
        if (typeof value === "number") {
          sum += value;
          count += 1;
        }
      }
      merged[key] = count ? sum / count : 0;
    }
    out.push(merged);
  }
  return out;
}

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
  // Bucket before anything measures the series: the axis label decides whether
  // to show seconds from the span, and a smoothed hour still spans an hour.
  const series = bucketAverages(points);
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
        data={series}
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
                second:
                  series.at(-1)!.timestamp - series[0]!.timestamp < 300_000
                    ? "2-digit"
                    : undefined,
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
            dot={series.length === 1 ? { r: 3 } : false}
            activeDot={{ r: 4 }}
            isAnimationActive={false}
            connectNulls={false}
          />
        ))}
      </LineChart>
    </ChartContainer>
  );
}
