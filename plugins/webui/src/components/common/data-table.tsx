import { useState, type ReactNode } from "react";
import { useI18n } from "@/i18n";
import { AppIcon } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { EmptyState } from "./page";
export type Column<T> = {
  key: string;
  title: string;
  cell: (row: T) => ReactNode;
  sort?: (row: T) => string | number;
  className?: string;
};
export function DataTable<T>({
  rows,
  columns,
  rowKey,
  paginate = true,
}: {
  rows: T[];
  columns: Column<T>[];
  rowKey: (row: T) => string | number;
  paginate?: boolean;
}) {
  const { text } = useI18n();
  const [sort, setSort] = useState({ key: "", direction: 1 });
  const [page, setPage] = useState(0);
  const selected = columns.find((c) => c.key === sort.key);
  const sorted = selected?.sort
    ? [...rows].sort((a, b) => {
        const x = selected.sort!(a),
          y = selected.sort!(b);
        return (
          (typeof x === "number" && typeof y === "number"
            ? x - y
            : String(x).localeCompare(String(y))) * sort.direction
        );
      })
    : rows;
  const pages = Math.max(1, Math.ceil(rows.length / 10)),
    current = Math.min(page, pages - 1);
  const visible = paginate
    ? sorted.slice(current * 10, current * 10 + 10)
    : sorted;
  return (
    <div className="space-y-4 min-w-0">
      <div className="overflow-hidden rounded-lg border">
        <Table>
          <TableHeader className="bg-muted/40">
            <TableRow>
              {columns.map((c) => (
                <TableHead
                  key={c.key}
                  className={c.className}
                  aria-sort={
                    sort.key === c.key
                      ? sort.direction === 1
                        ? "ascending"
                        : "descending"
                      : undefined
                  }
                >
                  {c.sort ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="-ml-3 h-9"
                      onClick={() =>
                        setSort({
                          key: c.key,
                          direction: sort.key === c.key ? -sort.direction : 1,
                        })
                      }
                    >
                      {c.title}
                      <AppIcon name="sort" />
                    </Button>
                  ) : (
                    c.title
                  )}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {visible.map((row) => (
              <TableRow key={rowKey(row)}>
                {columns.map((c) => (
                  <TableCell
                    key={c.key}
                    className={`py-3 ${c.className ?? ""}`}
                  >
                    {c.cell(row)}
                  </TableCell>
                ))}
              </TableRow>
            ))}
            {!rows.length && (
              <TableRow>
                <TableCell colSpan={columns.length}>
                  <EmptyState
                    title={text("没有匹配的记录", "No records found")}
                    description={text(
                      "添加一条记录，或尝试调整筛选条件。",
                      "Create a record or try changing your filters.",
                    )}
                  />
                </TableCell>
              </TableRow>
            )}
          </TableBody>
        </Table>
      </div>
      {paginate && (
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span>
            {text(`共 ${rows.length} 条记录`, `${rows.length} records`)}
          </span>
          <div className="flex items-center gap-2">
            <span className="mr-2">
              {current + 1} / {pages}
            </span>
            <Button
              variant="outline"
              size="icon"
              className="size-8"
              aria-label={text("上一页", "Previous page")}
              disabled={current === 0}
              onClick={() => setPage(current - 1)}
            >
              <AppIcon name="left" />
            </Button>
            <Button
              variant="outline"
              size="icon"
              className="size-8"
              aria-label={text("下一页", "Next page")}
              disabled={current === pages - 1}
              onClick={() => setPage(current + 1)}
            >
              <AppIcon name="right" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
