import { useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { toast } from "sonner";
import { getLogs } from "@/api/logs";
import { data, useProfiles } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { formatLogLines } from "@/components/logs/format";
import { AppIcon } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Choice, ErrorState, PageHeading } from "@/components/common/page";
export default function Logs() {
  const { t, text, locale } = useI18n(),
    profiles = useProfiles();
  const [profile, setProfile] = useState(""),
    [stream, setStream] = useState("combined"),
    [lines, setLines] = useState("200"),
    [search, setSearch] = useState(""),
    [cleared, setCleared] = useState(0);
  const selected = Number(profile || profiles.data?.[0]?.id) || null;
  const query = useQuery({
    queryKey: ["logs", selected, stream, lines],
    queryFn: () =>
      data(
        getLogs(selected!, {
          lines: Number(lines),
          log_type: stream as "combined" | "stdout" | "stderr",
        }),
      ),
    enabled: !!selected,
    refetchOnWindowFocus: false,
  });
  const content =
    query.dataUpdatedAt > cleared ? (query.data?.content ?? "") : "";
  const items = useMemo(
    () =>
      formatLogLines(content).filter(
        (row) =>
          row.text && row.text.toLowerCase().includes(search.toLowerCase()),
      ),
    [content, search],
  );
  const parent = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: items.length,
    getScrollElement: () => parent.current,
    estimateSize: () => 28,
    overscan: 12,
  });
  const download = () => {
    const url = URL.createObjectURL(
      new Blob([content], { type: "text/plain;charset=utf-8" }),
    );
    const link = document.createElement("a");
    link.href = url;
    link.download = `${query.data?.profile_name ?? "frpc"}-${stream}.log`;
    link.click();
    URL.revokeObjectURL(url);
  };
  return (
    <div className="space-y-6">
      <PageHeading
        title={t("nav.logs")}
        description={text(
          "筛选运行日志，定位连接与配置问题。",
          "Inspect runtime output and troubleshoot connections.",
        )}
      >
        <Button variant="outline" disabled={!content} onClick={download}>
          <AppIcon name="download" />
          {text("下载", "Download")}
        </Button>
        <Button
          disabled={!selected || query.isFetching}
          onClick={() => {
            setCleared(0);
            void query.refetch();
          }}
        >
          <AppIcon
            name="refresh"
            className={query.isFetching ? "animate-spin" : ""}
          />
          {t("common.refresh")}
        </Button>
      </PageHeading>
      <ErrorState
        error={query.error || profiles.error}
        retry={() => {
          void profiles.refetch();
          void query.refetch();
        }}
      />
      <div className="flex flex-wrap gap-3">
        <Choice
          label={t("logs.selectProfile")}
          value={selected}
          options={(profiles.data ?? []).map((p) => ({
            label: p.name,
            value: p.id!,
          }))}
          className="w-full sm:w-56"
          onChange={(v) => {
            setProfile(v);
            setCleared(0);
          }}
        />
        <Choice
          label={text("日志行数", "Lines")}
          value={lines}
          options={["100", "200", "500", "1000"].map((value) => ({
            value,
            label: text(`最近 ${value} 行`, `Last ${value} lines`),
          }))}
          onChange={setLines}
          className="w-40"
        />
        <div className="relative min-w-40 flex-1 sm:max-w-sm">
          <AppIcon
            name="search"
            className="absolute left-3 top-2.5 text-muted-foreground"
          />
          <Input
            className="pl-9"
            aria-label={t("common.search")}
            placeholder={text("搜索日志…", "Search logs…")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>
      <section className="overflow-hidden rounded-xl border">
        <div className="flex flex-wrap items-center justify-between gap-2 border-b bg-muted/30 px-3 py-2">
          <Tabs
            value={stream}
            onValueChange={(v) => {
              setStream(v);
              setCleared(0);
            }}
          >
            <TabsList>
              {["combined", "stdout", "stderr"].map((s) => (
                <TabsTrigger key={s} value={s}>
                  {t(`logs.${s}` as "logs.combined")}
                </TabsTrigger>
              ))}
            </TabsList>
          </Tabs>
          <div className="flex gap-1">
            <Button
              variant="ghost"
              size="sm"
              disabled={!content}
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(content);
                  toast.success(text("日志已复制", "Logs copied"));
                } catch {
                  toast.error(text("复制失败", "Copy failed"));
                }
              }}
            >
              <AppIcon name="copy" />
              {text("复制", "Copy")}
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={!content}
              onClick={() => setCleared(Date.now())}
            >
              {text("清空", "Clear")}
            </Button>
          </div>
        </div>
        <div
          ref={parent}
          tabIndex={0}
          role="region"
          aria-label={text("日志输出", "Log output")}
          className="log-console h-[58vh] min-h-80 overflow-auto bg-zinc-950 py-3 text-zinc-200"
        >
          {!items.length ? (
            <div className="flex h-full flex-col items-center justify-center gap-3 text-sm text-zinc-500">
              <AppIcon name="logs" size={24} />
              <p>
                {query.isFetching
                  ? text("正在读取日志…", "Loading logs…")
                  : !selected
                    ? text(
                        "选择一个服务器以查看日志",
                        "Select a server to view logs",
                      )
                    : text("没有匹配的日志", "No matching log entries")}
              </p>
            </div>
          ) : (
            <div
              style={{
                height: virtual.getTotalSize(),
                position: "relative",
                width: "100%",
              }}
            >
              {virtual.getVirtualItems().map((v) => {
                const row = items[v.index]!;
                return (
                  <div
                    key={row.id}
                    data-index={v.index}
                    ref={virtual.measureElement}
                    className="absolute left-0 top-0 flex w-full items-start font-mono text-xs leading-7 hover:bg-white/5"
                    style={{ transform: `translateY(${v.start}px)` }}
                  >
                    <span className="w-12 shrink-0 select-none pr-3 text-right text-zinc-600">
                      {row.number}
                    </span>
                    <span
                      className="min-w-0 flex-1 whitespace-pre-wrap break-all border-l border-zinc-800 px-4"
                      dangerouslySetInnerHTML={{ __html: row.html }}
                    />
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <div className="flex justify-between gap-3 border-t px-4 py-2.5 text-xs text-muted-foreground">
          <span>{text(`${items.length} 行`, `${items.length} entries`)}</span>
          <span>
            {query.dataUpdatedAt
              ? new Date(query.dataUpdatedAt).toLocaleTimeString(locale)
              : "—"}
          </span>
        </div>
      </section>
    </div>
  );
}
