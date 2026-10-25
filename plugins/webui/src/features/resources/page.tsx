import { ProfileVersionButton } from "./profile-version";
import { useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";
import { apiDelete, apiGet, apiPost, apiPut } from "@/api/client";
import { assignProfile } from "@/api/environments";
import type { FrpsProfile, LocalProxy, LocalVisitor } from "@/api/types";
import {
  command,
  data,
  useAction,
  useBindings,
  useEnvironments,
  useProfiles,
} from "@/hooks/api";
import { usePreferences } from "@/hooks/preferences";
import { useI18n } from "@/i18n";
import {
  defaultProfile,
  defaultProxy,
  defaultVisitor,
  mappedAddresses,
  type Entity,
  type EntityKind,
} from "@/lib/entities";
import { AppIcon } from "@/components/icon/AppIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DataTable, type Column } from "@/components/common/data-table";
import {
  Choice,
  ErrorState,
  PageHeading,
  PageLoading,
} from "@/components/common/page";
import { DeleteDialog, EditorDialog } from "@/components/common/dialogs";
import { ResourceEditor } from "./editor";
import { BatchDialog } from "./batch";
export default function Resources({ kind }: { kind: EntityKind }) {
  const { t, text } = useI18n(),
    navigate = useNavigate(),
    location = useLocation(),
    prefs = usePreferences();
  const query = useQuery({
    queryKey: [kind],
    queryFn: () => data(apiGet<Entity[]>(`/${kind}`)),
  });
  const profiles = useProfiles(),
    bindings = useBindings(),
    environments = useEnvironments();
  const [search, setSearch] = useState(""),
    [protocol, setProtocol] = useState("all"),
    [editing, setEditing] = useState<Entity | null>(null),
    [deleting, setDeleting] = useState<Entity | null>(null),
    [batch, setBatch] = useState(false);
  const singular =
    kind === "profiles" ? "profile" : kind === "proxies" ? "proxy" : "visitor";
  const defaults = () =>
    kind === "profiles"
      ? defaultProfile()
      : kind === "proxies"
        ? defaultProxy()
        : defaultVisitor();
  const route = location.pathname.split("/")[2];
  const entity =
    editing ??
    (route === "new"
      ? defaults()
      : route
        ? query.data?.find((row) => row.id === Number(route))
        : null);
  const close = () => {
    setEditing(null);
    if (route) navigate(`/${kind}`);
  };
  const save = useAction(
    (value: Entity) =>
      data(
        value.id
          ? apiPut(`/${kind}/${value.id}`, value)
          : apiPost(`/${kind}`, value),
      ),
    text("保存成功", "Saved"),
    close,
  );
  const remove = useAction(
    (value: Entity) => command(apiDelete(`/${kind}/${value.id}`)),
    text("删除成功", "Deleted"),
    () => setDeleting(null),
  );
  const assign = useAction(
    ({ id, environment }: { id: number; environment: number }) =>
      data(assignProfile(id, environment)),
  );
  const active =
    environments.data?.find((e) => e.id === prefs.environmentId) ??
    environments.data?.find((e) => e.is_default);
  const getProtocol = (row: Entity) =>
    "transport_protocol" in row
      ? row.transport_protocol
      : "proxy_type" in row
        ? row.proxy_type
        : row.visitor_type;
  const rows = (query.data ?? []).filter(
    (row) =>
      (kind !== "profiles" ||
        !active ||
        active.profile_ids.includes(row.id!)) &&
      (protocol === "all" || getProtocol(row) === protocol) &&
      [
        row.name,
        "server_addr" in row ? row.server_addr : "",
        "local_ip" in row ? row.local_ip : "",
        "server_name" in row ? row.server_name : "",
      ].some((value) => value.toLowerCase().includes(search.toLowerCase())),
  );
  const columns: Column<Entity>[] = [
    {
      key: "name",
      title: t("common.name"),
      sort: (row) => row.name,
      cell: (row) => <span className="font-medium">{row.name}</span>,
    },
    {
      key: "protocol",
      title: text("协议", "Protocol"),
      cell: (row) => (
        <Badge variant="outline" className="font-mono text-[11px]">
          {getProtocol(row).toUpperCase()}
        </Badge>
      ),
    },
  ];
  if (kind === "profiles")
    columns.push(
      {
        key: "endpoint",
        title: text("服务器地址", "Server address"),
        cell: (row) => (
          <code className="text-xs">
            {(row as FrpsProfile).server_addr}:
            {(row as FrpsProfile).server_port}
          </code>
        ),
      },
      {
        key: "tls",
        title: "TLS",
        cell: (row) => (
          <Badge variant="secondary">
            {(row as FrpsProfile).tls_enable
              ? text("已启用", "Enabled")
              : text("未启用", "Disabled")}
          </Badge>
        ),
      },
      {
        key: "environment",
        title: t("app.environment"),
        cell: (row) => (
          <Choice
            label={t("app.environment")}
            value={
              environments.data?.find((e) => e.profile_ids.includes(row.id!))
                ?.id
            }
            options={(environments.data ?? []).map((e) => ({
              value: e.id!,
              label: e.name,
            }))}
            disabled={assign.isPending}
            onChange={(v) =>
              assign.mutate({ id: row.id!, environment: Number(v) })
            }
            className="h-8 w-32"
          />
        ),
      },
    );
  if (kind === "proxies")
    columns.push(
      {
        key: "local",
        title: text("本地地址", "Local address"),
        cell: (row) => (
          <code className="text-xs">
            {(row as LocalProxy).local_ip}:{(row as LocalProxy).local_port}
          </code>
        ),
      },
      {
        key: "remote",
        title: t("proxy.remotePort"),
        cell: (row) => (
          <span className="font-mono text-xs">
            {(row as LocalProxy).remote_port ?? "—"}
          </span>
        ),
      },
      {
        key: "security",
        title: text("安全选项", "Security"),
        cell: (row) => (
          <div className="flex gap-1">
            {(row as LocalProxy).use_encryption && (
              <Badge variant="secondary">{t("proxy.encryption")}</Badge>
            )}
            {(row as LocalProxy).use_compression && (
              <Badge variant="secondary">{t("proxy.compression")}</Badge>
            )}
            {!(row as LocalProxy).use_encryption &&
              !(row as LocalProxy).use_compression &&
              "—"}
          </div>
        ),
      },
      {
        key: "mapped",
        title: text("映射地址", "Mapped address"),
        cell: (row) => (
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              const addresses = mappedAddresses(
                row as LocalProxy,
                profiles.data ?? [],
                bindings.data ?? [],
              );
              if (!addresses.length)
                return toast.warning(t("proxy.noMappedAddress"));
              try {
                await navigator.clipboard.writeText(addresses.join("\n"));
                toast.success(
                  t("proxy.copySuccess", { count: addresses.length }),
                );
              } catch {
                toast.error(t("proxy.copyFailed"));
              }
            }}
          >
            <AppIcon name="copy" />
            {text("复制地址", "Copy addresses")}
          </Button>
        ),
      },
    );
  if (kind === "visitors")
    columns.push(
      {
        key: "server",
        title: text("服务名称", "Server name"),
        cell: (row) => (row as LocalVisitor).server_name,
      },
      {
        key: "bind",
        title: text("绑定地址", "Bind address"),
        cell: (row) => (
          <code className="text-xs">
            {(row as LocalVisitor).bind_addr}:{(row as LocalVisitor).bind_port}
          </code>
        ),
      },
      {
        key: "profile",
        title: text("服务器", "Profile"),
        cell: (row) =>
          profiles.data?.find((p) => p.id === (row as LocalVisitor).profile_id)
            ?.name ?? `#${(row as LocalVisitor).profile_id}`,
      },
      {
        key: "enabled",
        title: text("状态", "Status"),
        cell: (row) => (
          <Badge
            variant={(row as LocalVisitor).enabled ? "secondary" : "outline"}
          >
            {(row as LocalVisitor).enabled
              ? t("common.enabled")
              : t("common.disabled")}
          </Badge>
        ),
      },
    );
  columns.push({
    key: "actions",
    title: t("common.actions"),
    className: "text-right",
    cell: (row) => (
      <div className="flex justify-end gap-1">
        {kind === "profiles" && (
          <ProfileVersionButton id={row.id!} name={row.name} />
        )}
        <Button variant="ghost" size="sm" onClick={() => setEditing(row)}>
          {t("common.edit")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="text-muted-foreground hover:text-destructive"
          onClick={() => setDeleting(row)}
        >
          {t("common.delete")}
        </Button>
      </div>
    ),
  });
  const description =
    kind === "profiles"
      ? text(
          "管理 FRP 服务端连接及认证方式。",
          "Manage server endpoints and authentication.",
        )
      : kind === "proxies"
        ? text(
            "定义本地服务，并通过分配关系将其发布到服务器。",
            "Define local services, then assign them to a server.",
          )
        : text(
            "通过安全隧道连接其他用户共享的服务。",
            "Connect to shared services through secure tunnels.",
          );
  return (
    <div className="space-y-6">
      <PageHeading title={t(`nav.${kind}`)} description={description}>
        {kind === "proxies" && (
          <Button variant="outline" onClick={() => setBatch(true)}>
            {t("proxy.batchCreate")}
          </Button>
        )}
        <Button onClick={() => setEditing(defaults())}>
          <AppIcon name="plus" />
          {t(`${singular}.create`)}
        </Button>
      </PageHeading>
      <ErrorState
        error={
          query.error ||
          environments.error ||
          profiles.error ||
          (kind === "proxies" && bindings.error)
        }
        retry={() => {
          void query.refetch();
          void environments.refetch();
          void profiles.refetch();
          void bindings.refetch();
        }}
      />
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative w-full sm:w-72">
          <AppIcon
            name="search"
            className="absolute left-3 top-2.5 text-muted-foreground"
          />
          <Input
            className="pl-9"
            aria-label={t("common.search")}
            placeholder={text("搜索名称或地址…", "Search by name or address…")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <Choice
          label={text("协议筛选", "Filter protocol")}
          value={protocol}
          onChange={setProtocol}
          className="w-36"
          options={[
            { value: "all", label: text("所有协议", "All protocols") },
            ...[...new Set((query.data ?? []).map(getProtocol))]
              .sort()
              .map((value) => ({ value, label: value.toUpperCase() })),
          ]}
        />
        {(search || protocol !== "all") && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setSearch("");
              setProtocol("all");
            }}
          >
            {text("重置", "Reset")}
            <AppIcon name="close" />
          </Button>
        )}
      </div>
      {query.isPending ? (
        <PageLoading />
      ) : (
        <DataTable
          key={search + protocol}
          rows={rows}
          columns={columns}
          rowKey={(row) => row.id!}
        />
      )}
      {route && route !== "new" && !query.isPending && !entity && (
        <ErrorState error={new Error(text("记录不存在", "Record not found"))} />
      )}
      <EditorDialog
        open={!!entity}
        title={t(`${singular}.${entity?.id ? "edit" : "create"}`)}
        description={description}
        onClose={close}
        busy={save.isPending}
      >
        {entity && (
          <ResourceEditor
            key={`${kind}-${entity.id ?? "new"}`}
            kind={kind}
            initial={entity}
            onSave={save.mutate}
            onCancel={close}
            busy={save.isPending}
          />
        )}
      </EditorDialog>
      <DeleteDialog
        open={!!deleting}
        title={t(`${singular}.delete`)}
        description={t(`${singular}.deleteConfirm`, {
          name: deleting?.name ?? "",
        })}
        busy={remove.isPending}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && remove.mutate(deleting)}
      />
      {batch && (
        <BatchDialog
          existing={(query.data ?? []) as LocalProxy[]}
          onClose={() => setBatch(false)}
        />
      )}
    </div>
  );
}
