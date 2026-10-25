import type { ProcessInfo } from "@/api/types";
import { useI18n } from "@/i18n";
import { DataTable } from "./data-table";
import { RuntimeBadge } from "./page";
export function ProcessTable({ processes }: { processes: ProcessInfo[] }) {
  const { text } = useI18n();
  return (
    <DataTable
      rows={processes}
      rowKey={(p) => p.profile_id}
      columns={[
        {
          key: "name",
          title: text("服务器", "Profile"),
          sort: (p) => p.profile_name,
          cell: (p) => <span className="font-medium">{p.profile_name}</span>,
        },
        {
          key: "status",
          title: text("状态", "Status"),
          cell: (p) => <RuntimeBadge running={p.running} />,
        },
        {
          key: "pid",
          title: "PID",
          cell: (p) => (
            <span className="font-mono text-xs">{p.pid ?? "—"}</span>
          ),
        },
        {
          key: "restarts",
          title: text("重启次数", "Restarts"),
          sort: (p) => p.restart_count,
          cell: (p) => p.restart_count,
        },
        {
          key: "detail",
          title: text("最近异常", "Last failure"),
          cell: (p) => (
            <span className="text-xs text-muted-foreground">
              {p.last_failure?.summary ?? "—"}
            </span>
          ),
        },
      ]}
    />
  );
}
