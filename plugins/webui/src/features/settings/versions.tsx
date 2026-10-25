import { VersionPicker } from "./version-picker";
import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  activateFrpVersion,
  deleteFrpVersion,
  beginInstallation,
  getInstallation,
  listAvailableFrpVersions,
  listFrpVersions,
} from "@/api/frp";
import type { InstalledFrpVersion } from "@/api/types";
import { command, data, useAction } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { formatBytes } from "@/i18n/format";
import { AppIcon } from "@/components/icon/AppIcon";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Field, FieldLabel } from "@/components/ui/field";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from "@/components/ui/alert-dialog";
import {
  ActionButton,
  Choice,
  ErrorState,
  PageLoading,
} from "@/components/common/page";
import { DataTable } from "@/components/common/data-table";
import { DeleteDialog } from "@/components/common/dialogs";
export function Versions() {
  const { t, text, locale } = useI18n(),
    client = useQueryClient();
  const [version, setVersion] = useState(""),
    [channel, setChannel] = useState("official"),
    [url, setUrl] = useState("");
  const [deleting, setDeleting] = useState<InstalledFrpVersion | null>(null),
    [activating, setActivating] = useState<string | null>(null);
  const versions = useQuery({
    queryKey: ["frp-versions"],
    queryFn: () => data(listFrpVersions()),
  });
  const releases = useQuery({
    queryKey: ["frp-releases"],
    queryFn: () => data(listAvailableFrpVersions()),
    retry: false,
    staleTime: 300000,
  });
  const installation = useQuery({
    queryKey: ["frp-installation"],
    queryFn: () => data(getInstallation()),
    refetchInterval: (q) => (q.state.data?.status === "running" ? 1000 : 5000),
    retry: false,
  });
  const task = installation.data;
  useEffect(() => {
    if (task?.status === "completed") {
      void client.invalidateQueries({ queryKey: ["frp-versions"] });
    }
  }, [task?.id, task?.status, client]);
  const install = useAction(async () => {
    const selected = version.trim() || releases.data?.[0]?.version;
    if (!selected) throw new Error(t("frpVersion.selectRequired"));
    if (channel === "custom") {
      try {
        const parsed = new URL(url.trim());
        if (
          parsed.protocol !== "https:" ||
          parsed.username ||
          parsed.password ||
          parsed.search ||
          parsed.hash
        )
          throw new Error();
      } catch {
        throw new Error(
          text(
            "请输入不含账号、查询参数或片段的 HTTPS 下载根地址",
            "Enter an HTTPS release base without credentials, query, or fragment",
          ),
        );
      }
    }
    const result = await data(
      beginInstallation(
        selected,
        channel,
        channel === "custom" ? url.trim() : undefined,
      ),
    );
    client.setQueryData(["frp-installation"], result);
  });
  const activate = useAction(
    (v: string) => data(activateFrpVersion(v)),
    text("系统默认版本已更新", "System default updated"),
    () => setActivating(null),
  );
  const remove = useAction(
    (v: string) => command(deleteFrpVersion(v)),
    text("版本已删除", "Version deleted"),
    () => setDeleting(null),
  );
  const busy =
    install.isPending ||
    activate.isPending ||
    remove.isPending ||
    task?.status === "running";
  const stages: Record<string, string> = {
    checking: text("检查本地版本", "Checking local installation"),
    downloading: text("下载安装包", "Downloading archive"),
    verifying: text(
      "获取官方校验信息并验证 SHA-256",
      "Fetching official checksum and verifying SHA-256",
    ),
    extracting: text("解压文件", "Extracting files"),
    installing: text("安装二进制", "Installing binary"),
    completed: text("安装完成", "Installation complete"),
  };
  const percent = task?.total_bytes
    ? Math.min(
        100,
        Math.floor((task.downloaded_bytes / task.total_bytes) * 100),
      )
    : undefined;
  const sourceName = (source?: string | null) =>
    !source
      ? text("来源未记录", "Source not recorded")
      : source.startsWith("https://ghfast.top/")
        ? "GHFast"
        : source.startsWith("https://gh-proxy.com/")
          ? "GH-Proxy"
          : source.startsWith("https://github.com/")
            ? "GitHub"
            : text("自定义镜像", "Custom mirror");
  return (
    <div className="space-y-6">
      <Card className="shadow-none">
        <CardHeader>
          <CardTitle>
            {text("安装 frpc 版本", "Install frpc version")}
          </CardTitle>
          <CardDescription>
            {text(
              "安装与启用分开：下载新版本不会影响现有连接。每台服务器可固定版本，或跟随系统默认。",
              "Installing a version keeps existing connections running. Servers can pin a version or follow the system default.",
            )}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <form
            className="grid items-end gap-4 sm:grid-cols-2 lg:grid-cols-[1fr_1fr_auto]"
            onSubmit={(e) => {
              e.preventDefault();
              install.mutate();
            }}
          >
            <Field>
              <FieldLabel htmlFor="version">
                {t("frpVersion.version")}
              </FieldLabel>
              <VersionPicker
                value={version}
                onChange={setVersion}
                versions={releases.data?.map((release) => release.version) ?? []}
                disabled={busy}
              />
            </Field>
            <Field>
              <FieldLabel>{text("下载渠道", "Download channel")}</FieldLabel>
              <Choice
                label={text("下载渠道", "Download channel")}
                value={channel}
                onChange={setChannel}
                disabled={busy}
                options={[
                  {
                    value: "official",
                    label: text("GitHub 官方", "GitHub official"),
                  },
                  { value: "ghfast", label: "GHFast · ghfast.top" },
                  { value: "gh-proxy", label: "GH-Proxy · gh-proxy.com" },
                  {
                    value: "custom",
                    label: text("自定义镜像", "Custom mirror"),
                  },
                ]}
              />
            </Field>
            <ActionButton
              type="submit"
              busy={install.isPending}
              disabled={busy || installation.isPending || installation.isError}
            >
              <AppIcon name="download" />
              {text("安装版本", "Install version")}
            </ActionButton>
            {channel === "custom" && (
              <Field className="sm:col-span-2 lg:col-span-3">
                <FieldLabel htmlFor="mirror-url">
                  {text("Release 下载根地址", "Release download base")}
                </FieldLabel>
                <Input
                  id="mirror-url"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                  placeholder="https://your-proxy/https://github.com/fatedier/frp/releases/download"
                  disabled={busy}
                />
              </Field>
            )}
          </form>
          <p className="text-xs leading-5 text-muted-foreground">
            {text(
              "加速渠道为第三方服务，可用性取决于网络。校验信息始终来自 GitHub 官方；无法获取或校验失败时不会安装。",
              "Accelerators are third-party services; availability varies. Checksums always come from official GitHub sources. Installation stops if verification is unavailable or fails.",
            )}
          </p>
          {releases.error && (
            <p className="text-xs text-muted-foreground">
              {text(
                "版本列表暂不可用，仍可手动输入版本号。",
                "Release suggestions are unavailable. You can enter a version manually.",
              )}
            </p>
          )}
          <ErrorState
            error={install.error || installation.error}
            retry={() => void installation.refetch()}
          />
          {task && (
            <div role="status" className="space-y-3 rounded-lg border p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-sm font-medium">
                  frpc {task.version} · {sourceName(task.source)}
                </span>
                <Badge
                  variant={
                    task.status === "failed" ? "destructive" : "secondary"
                  }
                >
                  {task.status === "failed"
                    ? text("安装失败", "Installation failed")
                    : (stages[task.stage] ?? task.stage)}
                </Badge>
              </div>
              {task.status === "running" && (
                <>
                  <progress
                    aria-label={text("安装进度", "Installation progress")}
                    className="h-2 w-full accent-primary"
                    max={100}
                    value={task.stage === "downloading" ? percent : undefined}
                  />
                  <p className="text-xs text-muted-foreground">
                    {task.stage === "downloading"
                      ? `${formatBytes(task.downloaded_bytes, locale)}${task.total_bytes ? ` / ${formatBytes(task.total_bytes, locale)} · ${percent}%` : ""}`
                      : stages[task.stage]}{" "}
                    ·{" "}
                    {text(
                      "后台任务继续运行，可离开此页面。",
                      "This task continues in the background.",
                    )}
                  </p>
                </>
              )}
              {task.status === "failed" && (
                <>
                  <p className="break-words text-sm text-destructive">
                    {task.error}
                  </p>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setVersion(task.version)}
                  >
                    {text(
                      "使用此版本重试 / 更换渠道",
                      "Retry this version / change channel",
                    )}
                  </Button>
                </>
              )}
              {task.status === "completed" && (
                <p className="text-sm text-muted-foreground">
                  {text(
                    "已通过完整性校验。可设为系统默认，或在服务器配置中单独选择。",
                    "Integrity verified. Set it as the default or select it for an individual server.",
                  )}
                </p>
              )}
            </div>
          )}
        </CardContent>
      </Card>
      <ErrorState
        error={versions.error || activate.error || remove.error}
        retry={() => void versions.refetch()}
      />
      <div className="space-y-4">
        <div>
          <h2 className="text-base font-semibold">
            {text("已安装版本", "Installed versions")}
          </h2>
          <p className="mt-1 text-sm text-muted-foreground">
            {text(
              "切换默认版本仅重启跟随默认的实例。被服务器引用或仍在运行的版本不能删除。",
              "Changing the default restarts only servers following it. Referenced or running versions cannot be deleted.",
            )}
          </p>
        </div>
        {versions.isPending ? (
          <PageLoading />
        ) : (
          <DataTable
            rows={versions.data?.installed ?? []}
            rowKey={(v) => v.version}
            columns={[
              {
                key: "version",
                title: t("frpVersion.version"),
                cell: (v) => (
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{v.version}</span>
                    {v.active && (
                      <Badge variant="secondary">
                        {text("系统默认", "System default")}
                      </Badge>
                    )}
                    {v.in_use && (
                      <Badge variant="outline">
                        {text("使用中", "In use")}
                      </Badge>
                    )}
                  </div>
                ),
              },
              {
                key: "source",
                title: text("安装来源", "Installed from"),
                cell: (v) => (
                  <span title={v.source ?? undefined}>
                    {sourceName(v.source)}
                  </span>
                ),
              },
              {
                key: "platform",
                title: t("frpVersion.platform"),
                cell: (v) => <code className="text-xs">{v.platform}</code>,
              },
              {
                key: "integrity",
                title: t("frpVersion.integrity"),
                cell: (v) => (
                  <Badge variant={v.integrity_ok ? "outline" : "destructive"}>
                    {t(
                      v.integrity_ok
                        ? "frpVersion.verified"
                        : "frpVersion.damaged",
                    )}
                  </Badge>
                ),
              },
              {
                key: "actions",
                title: t("common.actions"),
                className: "text-right",
                cell: (v) => (
                  <div className="flex justify-end gap-1">
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy || v.active || !v.integrity_ok}
                      onClick={() => setActivating(v.version)}
                    >
                      {text("设为默认", "Set as default")}
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={busy || v.active || v.in_use}
                      onClick={() => setDeleting(v)}
                    >
                      {t("common.delete")}
                    </Button>
                  </div>
                ),
              },
            ]}
          />
        )}
      </div>
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
        onOpenChange={(v) => {
          if (!v && !activate.isPending) setActivating(null);
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
                `Setting ${activating} restarts running servers that follow the default, briefly interrupting their proxies. Pinned servers are unaffected.`,
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
              {text(
                "切换并重启相关实例",
                "Change and restart affected servers",
              )}
            </ActionButton>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
