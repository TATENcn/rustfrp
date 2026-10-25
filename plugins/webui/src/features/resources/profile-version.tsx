import { useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  getProfileVersion,
  setProfileVersion,
  listFrpVersions,
} from "@/api/frp";
import { startProfile } from "@/api/profiles";
import { data, useAction } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { EditorDialog } from "@/components/common/dialogs";
import { Choice, ActionButton, ErrorState } from "@/components/common/page";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel } from "@/components/ui/field";

export function ProfileVersionButton({
  id,
  name,
  readonly = false,
}: {
  id: number;
  name: string;
  readonly?: boolean;
}) {
  const { text } = useI18n();
  const [open, setOpen] = useState(false),
    [selection, setSelection] = useState("default");
  const runtime = useQuery({
    queryKey: ["profile-version", id],
    queryFn: () => data(getProfileVersion(id)),
    refetchInterval: 5000,
  });
  const versions = useQuery({
    queryKey: ["frp-versions"],
    queryFn: () => data(listFrpVersions()),
    enabled: open,
  });
  const save = useAction(
    () =>
      data(setProfileVersion(id, selection === "default" ? null : selection)),
    text(
      "版本设置已保存，下次启动时生效",
      "Version saved; applies on next start",
    ),
  );
  const restart = useAction(
    () => data(startProfile(id)),
    text("服务器版本已应用", "Server version applied"),
  );
  const busy = save.isPending || restart.isPending;
  const current = runtime.data;
  const label = runtime.isError
    ? text("版本读取失败", "Version unavailable")
    : runtime.isPending
      ? "frpc …"
      : `frpc ${current?.running ? (current.running_version ?? text("自定义", "custom")) : (current?.effective_version ?? text("系统默认", "default"))}`;
  if (readonly)
    return (
      <Button variant="link" size="sm" className="h-auto p-0 text-xs" asChild>
        <Link to="/profiles">
          {label}
          {current?.restart_required
            ? text(" · 待重启", " · restart pending")
            : ""}
        </Link>
      </Button>
    );
  const saved = current?.version ?? "default";
  return (
    <>
      <Button
        variant="outline"
        size="sm"
        onClick={() => {
          setSelection(saved);
          setOpen(true);
        }}
      >
        {label}
        {current?.restart_required
          ? text(" · 待重启", " · restart pending")
          : ""}
      </Button>
      <EditorDialog
        open={open}
        title={text(`${name} · frpc 版本`, `${name} · frpc version`)}
        description={text(
          "同一服务器下的全部代理共用此版本。保存不打断当前连接。",
          "All proxies on this server share one version. Saving keeps current connections running.",
        )}
        busy={busy}
        onClose={() => setOpen(false)}
      >
        <ErrorState
          error={runtime.error || versions.error || save.error || restart.error}
          retry={() => {
            void runtime.refetch();
            void versions.refetch();
          }}
        />
        <Field>
          <FieldLabel>{text("运行版本", "Runtime version")}</FieldLabel>
          <Choice
            label={text("frpc 版本", "frpc version")}
            value={selection}
            onChange={setSelection}
            disabled={
              busy ||
              runtime.isPending ||
              runtime.isError ||
              versions.isPending ||
              versions.isError
            }
            options={[
              ...(saved !== "default" &&
              !versions.data?.installed.some(
                (v) => v.version === saved && v.integrity_ok,
              )
                ? [
                    {
                      value: saved,
                      label: text(
                        `${saved}（未安装或校验失败）`,
                        `${saved} (missing or damaged)`,
                      ),
                    },
                  ]
                : []),
              {
                value: "default",
                label: text(
                  `跟随系统默认${versions.data?.active ? ` (${versions.data.active})` : ""}`,
                  `Follow system default${versions.data?.active ? ` (${versions.data.active})` : ""}`,
                ),
              },
              ...(versions.data?.installed
                .filter((v) => v.integrity_ok)
                .map((v) => ({ value: v.version, label: v.version })) ?? []),
            ]}
          />
        </Field>
        <p className="text-sm text-muted-foreground">
          {text("当前运行：", "Currently running: ")}
          {current?.running
            ? (current.running_version ?? text("自定义二进制", "Custom binary"))
            : text("未运行", "Stopped")}
        </p>
        <Button asChild variant="link" className="h-auto justify-start p-0">
          <Link to="/status?tab=versions">
            {text("安装或管理 frpc 版本", "Install or manage frpc versions")}
          </Link>
        </Button>
        {current?.restart_required && (
          <div role="status" className="space-y-3 rounded-lg border p-4">
            <p className="text-sm">
              {text(
                "版本变更待生效。立即应用会重启此服务器的 frpc，短暂中断其全部代理连接。",
                "Version change pending. Apply now restarts this server's frpc and briefly interrupts all its proxy connections.",
              )}
            </p>
            <ActionButton
              busy={restart.isPending}
              disabled={busy || selection !== saved}
              onClick={() => restart.mutate()}
            >
              {text("重启并应用版本", "Restart and apply version")}
            </ActionButton>
          </div>
        )}
        <div className="flex justify-end gap-2">
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => setOpen(false)}
          >
            {text("关闭", "Close")}
          </Button>
          <ActionButton
            busy={save.isPending}
            disabled={
              busy ||
              selection === saved ||
              versions.isError ||
              versions.isPending ||
              runtime.isError ||
              runtime.isPending
            }
            onClick={() => save.mutate()}
          >
            {text("保存版本设置", "Save version selection")}
          </ActionButton>
        </div>
      </EditorDialog>
    </>
  );
}
