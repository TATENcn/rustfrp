import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  activateFrpVersion,
  beginInstallation,
  deleteFrpVersion,
  getInstallations,
  listAvailableFrpVersions,
  listFrpVersions,
  type InstallTask,
} from "@/api/frp";
import type { AvailableFrpVersion, InstalledFrpVersion } from "@/api/types";
import { command, data, useAction } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { formatBytes, formatNumber } from "@/i18n/format";
import { AppIcon } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogCancel,
} from "@/components/ui/alert-dialog";
import {
  ActionButton,
  Choice,
  EmptyState,
  ErrorState,
  PageLoading,
} from "@/components/common/page";
import { DataTable } from "@/components/common/data-table";
import { DeleteDialog } from "@/components/common/dialogs";

/** Channel identifiers accepted by the daemon's installer. */
const CHANNELS = ["official", "ghfast", "gh-proxy", "custom"] as const;
type DownloadChannel = (typeof CHANNELS)[number];

const OFFICIAL_BASE = "https://github.com/fatedier/frp/releases/download";

/** Release base for a channel; mirrors the daemon's own mapping. */
function channelBase(channel: DownloadChannel, custom: string): string {
  switch (channel) {
    case "official":
      return OFFICIAL_BASE;
    case "ghfast":
      return `https://ghfast.top/${OFFICIAL_BASE}`;
    case "gh-proxy":
      return `https://gh-proxy.com/${OFFICIAL_BASE}`;
    case "custom":
      return custom.trim().replace(/\/+$/, "");
  }
}

const STAGE_LABELS: Record<string, [string, string]> = {
  checking: ["检查本地文件", "Checking local files"],
  downloading: ["下载安装包", "Downloading archive"],
  verifying: ["校验 SHA-256", "Verifying SHA-256"],
  extracting: ["解压文件", "Extracting files"],
  installing: ["安装二进制", "Installing binary"],
  completed: ["安装完成", "Installation complete"],
};

function sourceLabel(source: string | null | undefined, text: I18nText) {
  if (!source) return text("来源未记录", "Source not recorded");
  if (source.startsWith("https://ghfast.top/")) return "GHFast";
  if (source.startsWith("https://gh-proxy.com/")) return "GH-Proxy";
  if (source.startsWith("https://github.com/")) return "GitHub";
  return text("自定义镜像", "Custom mirror");
}

type I18nText = (zh: string, en: string) => string;

/** Progress, retry, or the installed-version controls for one release. */
function InstallControls({
  record,
  task,
  busy,
  onInstall,
  onActivate,
  onDelete,
}: {
  record?: InstalledFrpVersion;
  task?: InstallTask;
  busy: boolean;
  onInstall: () => void;
  onActivate: () => void;
  onDelete: () => void;
}) {
  const { t, text, locale } = useI18n();
  if (task?.status === "running") {
    const percent = task.total_bytes
      ? Math.min(
          100,
          Math.floor((task.downloaded_bytes / task.total_bytes) * 100),
        )
      : undefined;
    return (
      <div className="w-full space-y-1.5">
        <progress
          aria-label={text("下载进度", "Download progress")}
          className="h-1.5 w-full accent-primary"
          max={100}
          value={task.stage === "downloading" ? percent : undefined}
        />
        <p className="text-xs text-muted-foreground">
          {task.stage === "downloading"
            ? `${formatBytes(task.downloaded_bytes, locale)}${
                task.total_bytes
                  ? ` / ${formatBytes(task.total_bytes, locale)} · ${percent}%`
                  : ""
              }`
            : (STAGE_LABELS[task.stage]?.[locale === "zh" ? 0 : 1] ?? task.stage)}
        </p>
      </div>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-1">
      {task?.status === "failed" && (
        <p className="w-full break-words text-xs text-destructive">
          {task.error}
        </p>
      )}
      {record ? (
        <>
          {!record.active && (
            <Button
              variant="ghost"
              size="sm"
              disabled={busy || !record.integrity_ok}
              onClick={onActivate}
            >
              {text("设为默认", "Set as default")}
            </Button>
          )}
          {!record.active && (
            <Button
              variant="ghost"
              size="sm"
              disabled={busy || record.in_use}
              onClick={onDelete}
            >
              <AppIcon name="trash" />
              {t("common.delete")}
            </Button>
          )}
        </>
      ) : (
        <ActionButton variant="outline" size="sm" busy={busy} onClick={onInstall}>
          <AppIcon name="download" />
          {task?.status === "failed"
            ? text("重试", "Retry")
            : text("下载", "Download")}
        </ActionButton>
      )}
    </div>
  );
}

type ReleaseRow = {
  version: string;
  release?: AvailableFrpVersion;
  record?: InstalledFrpVersion;
};

/** Status of a release as compact badges; shared by both layouts. */
function VersionBadges({ row }: { row: ReleaseRow }) {
  const { text } = useI18n();
  return (
    <>
      {row.record?.active && (
        <Badge variant="secondary" className="px-1.5 text-[11px]">
          {text("系统默认", "System default")}
        </Badge>
      )}
      {row.record?.in_use && (
        <Badge variant="outline" className="px-1.5 text-[11px]">
          {text("使用中", "In use")}
        </Badge>
      )}
      {row.record && !row.record.integrity_ok && (
        <Badge variant="destructive" className="px-1.5 text-[11px]">
          {text("校验失败", "Integrity failed")}
        </Badge>
      )}
      {!row.release && (
        <Badge variant="outline" className="px-1.5 text-[11px]">
          {text("不在发布列表", "Not in release list")}
        </Badge>
      )}
    </>
  );
}

/** Size, download count, publish date and install source on a single line. */
function ReleaseMeta({ row }: { row: ReleaseRow }) {
  const { text, locale } = useI18n();
  const parts: string[] = [];
  if (row.release?.asset_size) {
    parts.push(formatBytes(row.release.asset_size, locale));
  }
  if (row.release?.download_count != null) {
    parts.push(
      text(
        `${formatNumber(row.release.download_count, locale)} 次下载`,
        `${formatNumber(row.release.download_count, locale)} downloads`,
      ),
    );
  }
  if (row.release?.published_at) {
    parts.push(
      new Date(row.release.published_at).toLocaleDateString(
        locale === "zh" ? "zh-CN" : "en-US",
      ),
    );
  }
  if (row.record) {
    parts.push(
      text(
        `来源 ${sourceLabel(row.record.source, text)}`,
        `from ${sourceLabel(row.record.source, text)}`,
      ),
    );
  }
  if (!parts.length) return null;
  const summary = parts.join(" · ");
  return (
    <p className="truncate text-xs text-muted-foreground" title={summary}>
      {summary}
    </p>
  );
}

export default function Downloads() {
  const { t, text, locale } = useI18n();
  const client = useQueryClient();
  const [channel, setChannel] = useState<DownloadChannel>("official");
  const [custom, setCustom] = useState("");
  const [view, setView] = useState<"cards" | "list">("cards");
  const [deleting, setDeleting] = useState<InstalledFrpVersion | null>(null);
  const [activating, setActivating] = useState<string | null>(null);

  const versions = useQuery({
    queryKey: ["frp-versions"],
    queryFn: () => data(listFrpVersions()),
  });
  const releases = useQuery({
    queryKey: ["frp-releases"],
    queryFn: () => data(listAvailableFrpVersions()),
    retry: false,
    staleTime: 300_000,
  });
  const tasks = useQuery({
    queryKey: ["frp-installations"],
    queryFn: () => data(getInstallations()),
    retry: false,
    refetchInterval: (q) =>
      q.state.data?.some((task) => task.status === "running") ? 1000 : false,
  });

  // A finished download only becomes an installed version once the registry is
  // re-read, so revalidate when the task list stops reporting running work.
  const running = tasks.data?.some((task) => task.status === "running") ?? false;
  useEffect(() => {
    if (!running && tasks.data?.length) {
      void client.invalidateQueries({ queryKey: ["frp-versions"] });
    }
  }, [running, tasks.data?.length, client]);

  const install = useAction(async (version: string) => {
    if (channel === "custom") {
      let valid = false;
      try {
        const parsed = new URL(custom.trim());
        valid =
          parsed.protocol === "https:" &&
          !parsed.username &&
          !parsed.password &&
          !parsed.search &&
          !parsed.hash;
      } catch {
        valid = false;
      }
      if (!valid) {
        throw new Error(
          text(
            "请输入不含账号、查询参数或片段的 HTTPS 下载根地址",
            "Enter an HTTPS release base without credentials, query, or fragment",
          ),
        );
      }
    }
    await data(
      beginInstallation(
        version,
        channel,
        channel === "custom" ? custom.trim() : undefined,
      ),
    );
  });
  const activate = useAction(
    (version: string) => data(activateFrpVersion(version)),
    text("系统默认版本已更新", "System default updated"),
    () => setActivating(null),
  );
  const remove = useAction(
    (version: string) => command(deleteFrpVersion(version)),
    text("版本已删除", "Version deleted"),
    () => setDeleting(null),
  );

  const installed = new Map(
    (versions.data?.installed ?? []).map((item) => [item.version, item]),
  );
  const taskByVersion = new Map<string, InstallTask>();
  for (const task of tasks.data ?? []) {
    // The daemon returns newest first, so the first entry per version wins.
    if (!taskByVersion.has(task.version)) taskByVersion.set(task.version, task);
  }
  const available = releases.data ?? [];
  const listed = new Set(available.map((release) => release.version));
  // Installed versions missing from the GitHub listing (offline fetch, a
  // mirror-only build, or a release deleted upstream) still need controls.
  const unlisted = [...installed.values()].filter(
    (item) => !listed.has(item.version),
  );
  const busy = install.isPending || activate.isPending || remove.isPending;
  const installingVersion = install.isPending ? install.variables : undefined;
  const rows = [
    ...available.map((release) => ({
      version: release.version,
      release,
      record: installed.get(release.version),
    })),
    ...unlisted.map((record) => ({
      version: record.version,
      release: undefined,
      record,
    })),
  ];

  const controls = (row: ReleaseRow) => (
    <InstallControls
      record={row.record}
      task={taskByVersion.get(row.version)}
      busy={busy || installingVersion === row.version}
      onInstall={() => install.mutate(row.version)}
      onActivate={() => setActivating(row.version)}
      onDelete={() => row.record && setDeleting(row.record)}
    />
  );

  // Direct archive URL per release for the selected channel, shown as a plain
  // link so a user can right-click or copy it without a button.
  const base = channelBase(channel, custom);
  const links = new Map<string, string>();
  if (base) {
    for (const release of available) {
      links.set(
        release.version,
        `${base}/v${release.version}/${release.asset_name}`,
      );
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <h1 className="text-xl font-semibold tracking-tight">
          {text("版本下载", "FRP downloads")}
        </h1>
        <Choice
          label={text("下载渠道", "Download channel")}
          value={channel}
          onChange={(value) => setChannel(value as DownloadChannel)}
          className="h-8 w-52"
          options={CHANNELS.map((value) => ({
            value,
            label:
              value === "official"
                ? text("GitHub 官方", "GitHub official")
                : value === "ghfast"
                  ? "GHFast · ghfast.top"
                  : value === "gh-proxy"
                    ? "GH-Proxy · gh-proxy.com"
                    : text("自定义镜像", "Custom mirror"),
          }))}
        />
        {channel === "custom" && (
          <Input
            aria-label={text("Release 下载根地址", "Release download base")}
            className="h-8 min-w-64 flex-1"
            value={custom}
            onChange={(event) => setCustom(event.target.value)}
            placeholder="https://your-proxy/https://github.com/fatedier/frp/releases/download"
          />
        )}
        <div className="ml-auto flex items-center gap-2">
          <div
            className="flex items-center gap-1 rounded-lg border p-0.5"
            role="group"
            aria-label={text("显示方式", "Layout")}
          >
            {(["cards", "list"] as const).map((mode) => (
              <Button
                key={mode}
                size="sm"
                className="h-7"
                variant={view === mode ? "secondary" : "ghost"}
                aria-pressed={view === mode}
                onClick={() => setView(mode)}
              >
                {mode === "cards" ? text("卡片", "Cards") : text("列表", "List")}
              </Button>
            ))}
          </div>
          <Button
            variant="outline"
            size="icon-sm"
            aria-label={t("common.refresh")}
            title={t("common.refresh")}
            disabled={releases.isFetching}
            onClick={() => void releases.refetch()}
          >
            <AppIcon
              name="refresh"
              className={releases.isFetching ? "animate-spin" : ""}
            />
          </Button>
        </div>
      </div>

      {channel !== "official" && (
        <p className="text-xs text-muted-foreground">
          {text(
            "加速渠道为第三方服务。校验信息始终来自 GitHub 官方，无法获取或校验失败时不会安装。",
            "Accelerators are third-party services. Checksums always come from official GitHub sources; installation stops when verification fails.",
          )}
        </p>
      )}

      <ErrorState
        error={
          install.error ||
          activate.error ||
          remove.error ||
          versions.error ||
          tasks.error
        }
        retry={() => void versions.refetch()}
      />
      {releases.error && (
        <p className="text-sm text-muted-foreground">
          {text(
            "发布列表暂不可用，已安装版本仍可管理。",
            "The release list is unavailable; installed versions can still be managed.",
          )}
        </p>
      )}

      {releases.isPending && versions.isPending ? (
        <PageLoading />
      ) : rows.length === 0 ? (
        <EmptyState
          icon="download"
          title={text("暂无可用版本", "No versions available")}
          description={text(
            "无法获取发布列表，请检查网络后重试。",
            "The release list could not be loaded. Check the network and retry.",
          )}
        />
      ) : view === "cards" ? (
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
          {rows.map((row) => {
            const link = links.get(row.version);
            return (
              <article
                key={row.version}
                className={`flex flex-col gap-1.5 rounded-lg border px-3 py-2 ${
                  row.release ? "border-l-2 border-l-primary" : "border-dashed"
                }`}
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex min-w-0 items-center gap-1.5">
                    <h2 className="font-mono text-sm font-semibold">
                      v{row.version}
                    </h2>
                    <VersionBadges row={row} />
                  </div>
                  {controls(row)}
                </div>
                <ReleaseMeta row={row} />
                {link && (
                  <a
                    href={link}
                    target="_blank"
                    rel="noreferrer"
                    title={link}
                    className="truncate font-mono text-xs text-primary underline-offset-2 hover:underline"
                  >
                    {link}
                  </a>
                )}
              </article>
            );
          })}
        </div>
      ) : (
        <DataTable
          dense
          rows={rows}
          rowKey={(row) => row.version}
          columns={[
            {
              key: "version",
              title: text("版本", "Version"),
              sort: (row) => row.version,
              cell: (row) => (
                <span className="flex items-center gap-1.5">
                  <span className="font-mono font-medium">v{row.version}</span>
                  <VersionBadges row={row} />
                </span>
              ),
            },
            {
              key: "size",
              title: text("大小", "Size"),
              sort: (row) => row.release?.asset_size ?? 0,
              cell: (row) =>
                row.release?.asset_size
                  ? formatBytes(row.release.asset_size, locale)
                  : "—",
            },
            {
              key: "downloads",
              title: text("下载数", "Downloads"),
              sort: (row) => row.release?.download_count ?? 0,
              cell: (row) =>
                row.release?.download_count == null
                  ? "—"
                  : formatNumber(row.release.download_count, locale),
            },
            {
              key: "published",
              title: text("发布时间", "Published"),
              sort: (row) => row.release?.published_at ?? "",
              cell: (row) =>
                row.release?.published_at
                  ? new Date(row.release.published_at).toLocaleDateString(
                      locale === "zh" ? "zh-CN" : "en-US",
                    )
                  : "—",
            },
            {
              key: "link",
              title: text("下载链接", "Link"),
              cell: (row) => {
                const link = links.get(row.version);
                return link ? (
                  <a
                    href={link}
                    target="_blank"
                    rel="noreferrer"
                    title={link}
                    className="block max-w-80 truncate font-mono text-xs text-primary underline-offset-2 hover:underline"
                  >
                    {link}
                  </a>
                ) : (
                  "—"
                );
              },
            },
            {
              key: "actions",
              title: t("common.actions"),
              className: "text-right",
              cell: (row) => (
                <div className="flex justify-end">{controls(row)}</div>
              ),
            },
          ]}
        />
      )}

      <DeleteDialog
        open={!!deleting}
        title={t("common.delete")}
        description={t("frpVersion.deleteConfirm", {
          version: deleting?.version ?? "",
        })}
        busy={remove.isPending}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && remove.mutate(deleting.version)}
      />
      <AlertDialog
        open={!!activating}
        onOpenChange={(value) => {
          if (!value && !activate.isPending) setActivating(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {text("切换系统默认版本", "Change system default")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {text(
                `设为 ${activating} 后，跟随默认版本的运行实例将重启，相关代理连接会短暂中断。固定版本的服务器不受影响。`,
                `Setting ${activating} restarts servers that follow the default, briefly interrupting their proxies. Pinned servers are unaffected.`,
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={activate.isPending}>
              {t("common.cancel")}
            </AlertDialogCancel>
            <ActionButton
              busy={activate.isPending}
              onClick={() => activating && activate.mutate(activating)}
            >
              {text("切换并重启相关实例", "Change and restart affected servers")}
            </ActionButton>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
