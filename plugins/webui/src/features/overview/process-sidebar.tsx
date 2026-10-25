import { Link } from "react-router-dom";
import type { ProcessInfo } from "@/api/types";
import { useI18n } from "@/i18n";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { RuntimeBadge } from "@/components/common/page";
import { AppIcon } from "@/components/icon/AppIcon";

export function ProcessSidebar({ processes }: { processes: ProcessInfo[] }) {
  const { text } = useI18n();
  return (
    <Card className="gap-4 shadow-none">
      <CardHeader className="flex flex-row items-start justify-between gap-2 px-5">
        <div className="space-y-1">
          <CardTitle>{text("运行进程", "Processes")}</CardTitle>
          <CardDescription>
            {text(
              "实例状态与最近异常。",
              "Instance health and recent failures.",
            )}
          </CardDescription>
        </div>
        <Button variant="ghost" size="sm" asChild className="-mr-2 shrink-0">
          <Link to="/bindings">
            {text("管理", "Manage")}
            <AppIcon name="right" />
          </Link>
        </Button>
      </CardHeader>
      <CardContent className="px-5">
        {processes.length ? (
          <ul
            aria-label={text("进程列表", "Process list")}
            className="max-h-[min(560px,65dvh)] space-y-3 overflow-y-auto overscroll-contain"
          >
            {processes.map((process) => (
              <li
                key={process.profile_id}
                className="space-y-3 rounded-lg border p-3"
              >
                <div className="flex items-start justify-between gap-2">
                  <span className="min-w-0 break-all text-sm font-medium">
                    {process.profile_name}
                  </span>
                  <span className="shrink-0">
                    <RuntimeBadge running={process.running} />
                  </span>
                </div>
                <dl className="grid grid-cols-2 gap-2 text-xs">
                  <div>
                    <dt className="text-muted-foreground">PID</dt>
                    <dd className="mt-1 font-mono">{process.pid ?? "—"}</dd>
                  </div>
                  <div>
                    <dt className="text-muted-foreground">
                      {text("重启次数", "Restarts")}
                    </dt>
                    <dd className="mt-1 tabular-nums">
                      {process.restart_count}
                    </dd>
                  </div>
                </dl>
                <div className="border-t pt-2 text-xs">
                  <p className="text-muted-foreground">
                    {text("最近异常", "Last failure")}
                  </p>
                  <p className="mt-1 break-words whitespace-pre-wrap">
                    {process.last_failure?.summary ??
                      text("暂无异常", "No recent failures")}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <p className="py-4 text-sm text-muted-foreground">
            {text("暂无运行进程", "No processes yet")}
          </p>
        )}
        <p className="mt-3 text-xs text-muted-foreground">
          {text(
            `共 ${processes.length} 个实例`,
            `${processes.length} instances`,
          )}
        </p>
      </CardContent>
    </Card>
  );
}
