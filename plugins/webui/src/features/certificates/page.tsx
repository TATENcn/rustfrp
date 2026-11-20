import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  applyCertificate,
  attachSource,
  bindCertificate,
  createSource,
  deleteCertificate,
  deleteSource,
  importFromPath,
  importPem,
  listCertificates,
  listSources,
  promoteSource,
  runSource,
  unbindCertificate,
  type Certificate,
  type CertificateSource,
  type CreatedSource,
  type SourceKind,
} from "@/api/certificates";
import { command, data, useAction, useProxies } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { extractApiError } from "@/api/errors";
import { AppIcon } from "@/components/icon/AppIcon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { DataTable, type Column } from "@/components/common/data-table";
import {
  Choice,
  EmptyState,
  ErrorState,
  PageHeading,
  PageLoading,
} from "@/components/common/page";
import { DeleteDialog, EditorDialog } from "@/components/common/dialogs";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

/** Sort order for the status column: problems first, healthy last. */
const STATUS_ORDER: Record<Certificate["status"], number> = {
  expired: 0,
  not_yet_valid: 1,
  pending: 2,
  expiring_soon: 3,
  active: 4,
};

export default function Certificates() {
  const { t, text } = useI18n();
  const [importing, setImporting] = useState(false);
  const [deleting, setDeleting] = useState<Certificate | null>(null);
  const [channelsFor, setChannelsFor] = useState<Certificate | null>(null);

  const certificates = useQuery({
    queryKey: ["certificates"],
    queryFn: () => data(listCertificates()),
  });
  // One request for every channel: the table needs to say how each certificate
  // is kept up to date, and asking per row would be a request per certificate.
  const sources = useQuery({
    queryKey: ["certificate-sources", "all"],
    queryFn: () => data(listSources()),
  });

  const remove = useAction(
    (id: number) => command(deleteCertificate(id)),
    text("证书已删除", "Certificate deleted"),
  );
  const apply = useAction(
    (id: number) => data(applyCertificate(id)),
    text("证书已写入配置文件", "Certificate written to the configured files"),
  );
  // Bind and unbind share one dialog: the row menu offers whichever applies.
  const [binding, setBinding] = useState<Certificate | null>(null);
  const bindTarget = (row: Certificate) => setBinding(row);

  if (certificates.isLoading) return <PageLoading />;
  if (certificates.error)
    return (
      <ErrorState
        error={certificates.error}
        retry={() => void certificates.refetch()}
      />
    );

  const rows = certificates.data ?? [];
  // Unattached channels (an older shape, or one whose certificate was deleted)
  // are surfaced in the empty state instead of being silently invisible.
  const orphans = (sources.data ?? []).filter(
    (source) => source.certificate_id === null,
  );

  const columns: Column<Certificate>[] = [
    {
      key: "domain",
      title: text("域名", "Domain"),
      sort: (row) => row.primary_domain,
      className: "max-w-40",
      cell: (row) => (
        <div className="space-y-1">
          <div className="truncate font-medium">{row.primary_domain}</div>
          {row.sans.length > 1 && (
            <div className="truncate text-xs text-muted-foreground">
              {row.sans.filter((san) => san !== row.primary_domain).join(", ")}
            </div>
          )}
        </div>
      ),
    },
    {
      key: "status",
      title: text("状态", "Status"),
      sort: (row) => STATUS_ORDER[row.status],
      cell: (row) => (
        <div className="space-y-1">
          <Badge
            variant={
              row.status === "active"
                ? "secondary"
                : row.status === "expiring_soon"
                  ? "outline"
                  : row.status === "pending"
                    ? "outline"
                    : "destructive"
            }
          >
            {{
              active: text("可用", "Active"),
              expiring_soon: text("即将到期", "Expiring soon"),
              expired: text("已过期", "Expired"),
              not_yet_valid: text("尚未生效", "Not yet valid"),
              pending: text("待拉取", "Pending"),
            }[row.status]}
          </Badge>
          {row.status === "pending" ? (
            /* A pending certificate is not broken, it is waiting — and the
               reason it is still waiting is a channel failure, so show that. */
            <div
              className="max-w-48 truncate text-xs text-destructive"
              title={row.channel_error ?? undefined}
            >
              {row.channel_error ?? text("等待首次推送或拉取", "Waiting for the first push or fetch")}
            </div>
          ) : (
            <div className="text-xs text-muted-foreground">
              {row.status_reason ?? row.not_after.slice(0, 10)}
            </div>
          )}
        </div>
      ),
    },
    {
      key: "source",
      title: text("来源", "Source"),
      sort: (row) => row.source,
      className: "max-w-44",
      cell: (row) =>
        row.material_id === null ? (
          <span className="text-xs text-muted-foreground">
            {text("尚未导入", "No material yet")}
          </span>
        ) : (
          <div className="text-sm">
            {{
              manual: text("手动粘贴", "Pasted"),
              upload: text("上传文件", "Uploaded"),
              local_path: text("本机路径", "Local path"),
              webhook: text("推送", "Push"),
              certd_pull: text("Certd 拉取", "Certd pull"),
              url_pull: text("URL 拉取", "URL pull"),
            }[row.source] ?? row.source}
            {row.source_detail && (
              <div className="truncate text-xs text-muted-foreground">
                {row.source_detail}
              </div>
            )}
          </div>
        ),
    },
    {
      key: "key",
      title: text("密钥", "Key"),
      sort: (row) => row.key_algo,
      cell: (row) =>
        row.material_id === null ? (
          <span className="text-xs text-muted-foreground">—</span>
        ) : (
          <div className="space-y-1 text-sm">
            <div>{row.key_algo}</div>
            <div className="font-mono text-xs text-muted-foreground">
              {row.fingerprint_sha256.slice(0, 16)}
            </div>
          </div>
        ),
    },
    {
      key: "channels",
      title: text("更新方式", "Updates"),
      className: "max-w-40",
      cell: (row) => {
        const mine = (sources.data ?? []).filter(
          (source) => source.certificate_id === row.id,
        );
        if (mine.length === 0)
          return (
            <span className="text-xs text-muted-foreground">
              {text("仅手动", "Manual only")}
            </span>
          );
        return (
          <div className="space-y-1">
            {mine.map((source) => (
              <div key={source.id} className="flex items-center gap-2 text-xs">
                <span>
                  {{
                    push: text("推送", "Push"),
                    certd: text("Certd 拉取", "Certd pull"),
                    url: text("URL 拉取", "URL pull"),
                  }[source.kind]}
                </span>
                {source.is_primary && (
                  <Badge variant="outline">{text("优先", "Preferred")}</Badge>
                )}
                {source.last_status === "failed" ? (
                  <Badge variant="destructive">
                    {text("失败", "Failed")}
                  </Badge>
                ) : source.enabled === false ? (
                  <Badge variant="outline">{text("已停用", "Disabled")}</Badge>
                ) : null}
              </div>
            ))}
            {mine.some((source) => source.next_run_at) && (
              <div className="text-xs text-muted-foreground">
                {text("下次检查", "Next check")}{" "}
                {mine
                  .map((source) => source.next_run_at)
                  .filter(Boolean)
                  .sort()[0]
                  ?.slice(0, 16)}
              </div>
            )}
          </div>
        );
      },
    },
    {
      key: "bindings",
      title: text("绑定代理", "Bound proxies"),
      cell: (row) =>
        row.bindings.length === 0 ? (
          <span className="text-xs text-muted-foreground">
            {text("未绑定", "Not bound")}
          </span>
        ) : (
          <div className="space-y-1">
            {row.bindings.map((binding) => (
              <div key={binding.id} className="flex items-center gap-2 text-xs">
                <span>{binding.proxy_name || `#${binding.proxy_id}`}</span>
                {binding.last_error ? (
                  <Badge variant="destructive">{text("失败", "Failed")}</Badge>
                ) : row.fingerprint_sha256 &&
                  binding.applied_fingerprint === row.fingerprint_sha256 ? (
                  <Badge variant="secondary">{text("已生效", "Applied")}</Badge>
                ) : (
                  <Badge variant="outline">{text("待应用", "Pending")}</Badge>
                )}
              </div>
            ))}
          </div>
        ),
    },
    {
      key: "actions",
      title: "",
      // One button keeps the table from scrolling horizontally; the four
      // actions do not fit as a row at this width.
      className: "w-12",
      cell: (row) => (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" aria-label={t("common.actions")}>
              <AppIcon name="more" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-48">
            <DropdownMenuItem onSelect={() => setChannelsFor(row)}>
              {text("更新方式", "Channels")}
            </DropdownMenuItem>
            <DropdownMenuItem
              disabled={row.bindings.length === 0 || !row.usable}
              onSelect={() => apply.mutate(row.id)}
            >
              {text("应用到文件", "Apply to files")}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => bindTarget(row)}>
              {row.bindings.length ? text("解绑", "Unbind") : text("绑定代理", "Bind proxy")}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={() => setDeleting(row)}>
              {t("common.delete")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeading
        title={text("证书", "Certificates")}
        description={text(
          "为 https 代理提供域名证书。导入后可选推送或自动拉取来更新它。",
          "Domain certificates for https proxies. Choose push or automatic pull to keep one updated.",
        )}
      >
        <Button onClick={() => setImporting(true)}>
          <AppIcon name="plus" />
          {text("导入证书", "Import")}
        </Button>
      </PageHeading>

      {rows.length === 0 ? (
        <div className="space-y-4">
          <EmptyState
            title={text("还没有证书", "No certificates yet")}
            description={text(
              "粘贴 PEM、引用本机路径，或直接配置推送 / 自动拉取。",
              "Paste PEM, reference a local path, or configure push / automatic pull.",
            )}
          />
          {/* A channel whose first fetch failed used to leave this page looking
              untouched, which reads as "my configuration was not saved". */}
          {orphans.length > 0 && (
            <div className="space-y-3 rounded-lg border p-4">
              <div className="space-y-1">
                <h3 className="text-sm font-semibold">
                  {text(
                    "已配置更新方式，但还没有拉到证书",
                    "A channel is configured but has not delivered a certificate",
                  )}
                </h3>
                <p className="text-xs text-muted-foreground">
                  {text(
                    "证书会在首次成功推送或拉取后出现在上面。失败原因如下。",
                    "The certificate appears above once a push or fetch succeeds. Here is why it has not yet.",
                  )}
                </p>
              </div>
              {orphans.map((source) => (
                <div key={source.id} className="space-y-1 rounded-md border p-3 text-sm">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{source.name}</span>
                    <Badge variant="outline">
                      {{ push: text("推送", "Push"), certd: "Certd", url: "URL" }[source.kind]}
                    </Badge>
                    <span className="text-xs text-muted-foreground">{source.domain}</span>
                    {source.last_status === "failed" && (
                      <Badge variant="destructive">{text("失败", "Failed")}</Badge>
                    )}
                  </div>
                  {source.last_error && (
                    <p className="text-xs text-destructive">{source.last_error}</p>
                  )}
                  {source.next_run_at && (
                    <p className="text-xs text-muted-foreground">
                      {text("下次检查", "Next check")} {source.next_run_at.slice(0, 16)}
                    </p>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      ) : (
        <DataTable rows={rows} columns={columns} rowKey={(row) => row.id} />
      )}

      {binding && (
        <BindDialog
          certificate={binding}
          onClose={() => setBinding(null)}
          onDone={() => {
            setBinding(null);
            void certificates.refetch();
          }}
        />
      )}

      <ImportDialog
        open={importing}
        onClose={() => {
          setImporting(false);
          void sources.refetch();
        }}
      />

      {channelsFor && (
        <ChannelsDialog
          certificate={channelsFor}
          onClose={() => {
            setChannelsFor(null);
            void sources.refetch();
          }}
        />
      )}

      <DeleteDialog
        open={deleting !== null}
        title={text("删除证书", "Delete certificate")}
        description={
          deleting
            ? text(
                `删除 ${deleting.primary_domain}？已绑定的证书必须先解绑。`,
                `Delete ${deleting.primary_domain}? Bound certificates must be unbound first.`,
              )
            : ""
        }
        busy={remove.isPending}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
      />
    </div>
  );
}



/** Bind a certificate to a proxy, or unbind the one it already has. */
function BindDialog({
  certificate,
  onClose,
  onDone,
}: {
  certificate: Certificate;
  onClose: () => void;
  onDone: () => void;
}) {
  const { text } = useI18n();
  const proxies = useProxies();
  const [proxyId, setProxyId] = useState("");
  const bound = certificate.bindings[0];

  // Only proxies that can actually hold a certificate are offered.
  const candidates = (proxies.data ?? []).filter(
    (proxy) =>
      typeof proxy.plugin_config === "object" &&
      proxy.plugin_config !== null &&
      ["https2http", "https2https", "tls2raw"].includes(
        String((proxy.plugin_config as Record<string, unknown>).type ?? ""),
      ),
  );
  const bind = useAction(
    (body: { cert_id: number; proxy_id: number }) => data(bindCertificate(body)),
    text("已绑定", "Bound"),
    onDone,
  );
  const unbind = useAction(
    (id: number) => command(unbindCertificate(id, false)),
    text("已解绑，目标文件已保留", "Unbound; files kept on disk"),
    onDone,
  );

  if (bound) {
    return (
      <EditorDialog
        open
        title={text("解绑", "Unbind")}
        description={text(
          "解除后目标文件保留在磁盘上，证书本身不被删除。",
          "The files stay on disk; the certificate itself is not deleted.",
        )}
        busy={unbind.isPending}
        onClose={onClose}
      >
        <div className="space-y-4">
          <p className="text-sm">
            {text("当前绑定到", "Currently bound to")}{" "}
            <span className="font-medium">{bound.proxy_name || `#${bound.proxy_id}`}</span>
          </p>
          <Button disabled={unbind.isPending} onClick={() => unbind.mutate(bound.id)}>
            {text("确认解绑", "Unbind")}
          </Button>
        </div>
      </EditorDialog>
    );
  }

  return (
    <EditorDialog
      open
      title={text("绑定到代理", "Bind to a proxy")}
      description={text(
        "只列出配置了 https2http / https2https / tls2raw 插件的代理。证书会写入该代理声明的 crtPath 与 keyPath。",
        "Only proxies with an https2http / https2https / tls2raw plugin are listed. The certificate is written to the paths that plugin declares.",
      )}
      busy={bind.isPending}
      onClose={onClose}
    >
      <div className="space-y-4">
        {candidates.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {text(
              "没有可用的代理。请先在代理规则中添加 https2http / https2https / tls2raw 插件与证书路径。",
              "No eligible proxy. Add an https2http / https2https / tls2raw plugin with certificate paths first.",
            )}
          </p>
        ) : (
          <Choice
            label={text("代理", "Proxy")}
            value={proxyId}
            options={candidates.map((proxy) => ({
              value: String(proxy.id),
              label: proxy.name,
            }))}
            onChange={setProxyId}
          />
        )}
        <Button
          disabled={!proxyId || bind.isPending}
          onClick={() =>
            bind.mutate({ cert_id: certificate.id, proxy_id: Number(proxyId) })
          }
        >
          {text("确认绑定", "Bind")}
        </Button>
      </div>
    </EditorDialog>
  );
}

function ImportDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t, text } = useI18n();
  const [tab, setTab] = useState("pem");
  const [cert, setCert] = useState("");
  const [key, setKey] = useState("");
  const [crtPath, setCrtPath] = useState("");
  const [keyPath, setKeyPath] = useState("");
  const [note, setNote] = useState("");

  // Channel-first import: the certificate is created before any material
  // exists, and the channel that will supply it is configured in the same step.
  const [channelKind, setChannelKind] = useState<SourceKind>("certd");
  const [domain, setDomain] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [keyId, setKeyId] = useState("");
  const [keySecret, setKeySecret] = useState("");
  const [allowDomains, setAllowDomains] = useState("");
  const client = useQueryClient();

  const reset = () => {
    setCert("");
    setKey("");
    setCrtPath("");
    setKeyPath("");
    setNote("");
    setDomain("");
    setBaseUrl("");
    setKeyId("");
    setKeySecret("");
    setAllowDomains("");
  };
  const done = () => {
    reset();
    onClose();
  };

  const importPasted = useAction(
    (body: { cert: string; key: string; note?: string }) => data(importPem(body)),
    text("证书已导入", "Certificate imported"),
    done,
  );
  const importPath = useAction(
    (body: { crt_path: string; key_path: string; note?: string }) =>
      data(importFromPath(body)),
    text("证书已导入", "Certificate imported"),
    done,
  );

  // A channel-only import needs no material, so it uses the channel endpoint
  // directly and lets the first fetch fill the certificate in.
  const createChannel = useMutation({
    mutationFn: (body: Parameters<typeof createSource>[0]) =>
      data(createSource(body)),
    onSuccess: async (result) => {
      if (result.source.kind === "push" && result.plaintext) {
        setCreatedToken(result);
      } else {
        toast.success(text("更新方式已配置", "Channel configured"));
        done();
      }
      await client.invalidateQueries();
    },
    onError: (error) => toast.error(extractApiError(error).message),
  });
  const [createdToken, setCreatedToken] = useState<CreatedSource | null>(null);

  // Push is configured against a domain allowlist; certd/url against one domain.
  const channelReady =
    channelKind === "push"
      ? allowDomains.trim().length > 0
      : domain.trim().length > 0 && baseUrl.trim().length > 0;

  return (
    <EditorDialog
      open={open}
      title={text("导入证书", "Import certificate")}
      description={text(
        "证书与私钥只存于本机，加密保存；界面不会再次显示内容，也不提供下载。",
        "The certificate and key stay on this host, encrypted at rest. They are never shown again and cannot be downloaded.",
      )}
      busy={importPasted.isPending || importPath.isPending || createChannel.isPending}
      onClose={onClose}
    >
      <Tabs value={tab} onValueChange={setTab} className="space-y-4">
        <TabsList>
          <TabsTrigger value="pem">{text("粘贴 PEM", "Paste PEM")}</TabsTrigger>
          <TabsTrigger value="path">{text("本机路径", "Local path")}</TabsTrigger>
          <TabsTrigger value="channel">{text("推送 / 自动拉取", "Push / pull")}</TabsTrigger>
        </TabsList>
        <TabsContent value="pem" className="space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="cert">{text("证书（可含中间证书）", "Certificate (fullchain allowed)")}</Label>
            <textarea
              id="cert"
              className="h-28 w-full rounded-md border bg-transparent p-2 font-mono text-xs"
              placeholder="-----BEGIN CERTIFICATE-----"
              value={cert}
              onChange={(event) => setCert(event.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="key">{text("私钥", "Private key")}</Label>
            <textarea
              id="key"
              className="h-24 w-full rounded-md border bg-transparent p-2 font-mono text-xs"
              placeholder="-----BEGIN PRIVATE KEY-----"
              value={key}
              onChange={(event) => setKey(event.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="note">{text("备注（可选）", "Note (optional)")}</Label>
            <Input
              id="note"
              value={note}
              onChange={(event) => setNote(event.target.value)}
            />
          </div>
          <Button
            disabled={!cert.trim() || !key.trim() || importPasted.isPending}
            onClick={() =>
              importPasted.mutate({
                cert,
                key,
                note: note.trim() || undefined,
              })
            }
          >
            {text("导入", "Import")}
          </Button>
        </TabsContent>
        <TabsContent value="path" className="space-y-3">
          <p className="text-sm text-muted-foreground">
            {text(
              "适用于本机已有 acme.sh / certbot 输出的场景：只读取一次并加密存入数据库，不复制或监视文件。",
              "For certificates an existing acme.sh / certbot run already writes. Read once and stored encrypted; the file is not copied or watched.",
            )}
          </p>
          <div className="space-y-1.5">
            <Label htmlFor="crt-path">{text("证书路径", "Certificate path")}</Label>
            <Input
              id="crt-path"
              placeholder="/etc/letsencrypt/live/example.com/fullchain.pem"
              value={crtPath}
              onChange={(event) => setCrtPath(event.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="key-path">{text("私钥路径", "Key path")}</Label>
            <Input
              id="key-path"
              placeholder="/etc/letsencrypt/live/example.com/privkey.pem"
              value={keyPath}
              onChange={(event) => setKeyPath(event.target.value)}
            />
          </div>
          <Button
            disabled={!crtPath.trim() || !keyPath.trim() || importPath.isPending}
            onClick={() =>
              importPath.mutate({ crt_path: crtPath, key_path: keyPath })
            }
          >
            {text("导入", "Import")}
          </Button>
        </TabsContent>
        <TabsContent value="channel" className="space-y-3">
          <p className="text-sm text-muted-foreground">
            {text(
              "现在只配置来源：证书条目会先建立，等推送或首次拉取到达时才有内容。拉取到的证书必须覆盖下面填写的域名，否则会被拒绝。",
              "Configure the source only. The certificate entry is created now; its material arrives with the first push or fetch. A fetched certificate must cover the domain below or it is refused.",
            )}
          </p>
          <div className="space-y-1.5">
            <Label>{text("类型", "Kind")}</Label>
            <Choice
              label={text("类型", "Kind")}
              value={channelKind}
              options={[
                { value: "push", label: text("推送（Certd webhook）", "Push (Certd webhook)") },
                { value: "certd", label: text("自动拉取（Certd）", "Pull (Certd)") },
                { value: "url", label: text("自动拉取（HTTPS）", "Pull (HTTPS)") },
              ]}
              onChange={(next) => setChannelKind(next as SourceKind)}
            />
          </div>
          {channelKind === "push" ? (
            <div className="space-y-1.5">
              <Label htmlFor="allow-domains">
                {text("允许的域名（逗号分隔）", "Allowed domains (comma separated)")}
              </Label>
              <Input
                id="allow-domains"
                placeholder="*.example.com"
                value={allowDomains}
                onChange={(event) => setAllowDomains(event.target.value)}
              />
            </div>
          ) : (
            <>
              <div className="space-y-1.5">
                <Label htmlFor="channel-domain">{text("域名", "Domain")}</Label>
                <Input
                  id="channel-domain"
                  placeholder="example.com"
                  value={domain}
                  onChange={(event) => setDomain(event.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="channel-url">
                  {channelKind === "certd"
                    ? text("Certd 地址", "Certd base URL")
                    : text("证书地址", "Certificate URL")}
                </Label>
                <Input
                  id="channel-url"
                  placeholder={
                    channelKind === "certd"
                      ? "https://certd.example.com"
                      : "https://example.com/bundle.json"
                  }
                  value={baseUrl}
                  onChange={(event) => setBaseUrl(event.target.value)}
                />
              </div>
              {channelKind === "certd" && (
                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="space-y-1.5">
                    <Label htmlFor="channel-key-id">keyId</Label>
                    <Input
                      id="channel-key-id"
                      value={keyId}
                      onChange={(event) => setKeyId(event.target.value)}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="channel-key-secret">keySecret</Label>
                    <Input
                      id="channel-key-secret"
                      type="password"
                      value={keySecret}
                      onChange={(event) => setKeySecret(event.target.value)}
                    />
                  </div>
                </div>
              )}
            </>
          )}
          {createdToken?.plaintext ? (
            <div className="space-y-2 rounded-md border border-amber-500/40 bg-amber-500/5 p-3">
              <p className="text-sm font-medium">
                {text("令牌只显示这一次", "This token is shown once")}
              </p>
              <code className="block break-all rounded bg-background p-2 font-mono text-xs">
                {createdToken.plaintext}
              </code>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setCreatedToken(null);
                  done();
                }}
              >
                {text("我已保存", "Saved it")}
              </Button>
            </div>
          ) : (
            <Button
              disabled={!channelReady || createChannel.isPending}
              onClick={() =>
                createChannel.mutate({
                  kind: channelKind,
                  name: `certd-${Date.now()}`,
                  ...(channelKind === "push"
                    ? {
                        domains: allowDomains
                          .split(",")
                          .map((value) => value.trim())
                          .filter(Boolean),
                      }
                    : {
                        domain: domain.trim(),
                        base_url: baseUrl.trim(),
                        ...(channelKind === "certd"
                          ? {
                              key_id: keyId.trim() || undefined,
                              key_secret: keySecret || undefined,
                            }
                          : {}),
                      }),
                })
              }
            >
              {text("创建", "Create")}
            </Button>
          )}
        </TabsContent>
      </Tabs>
    </EditorDialog>
  );
}

/** Channels for one certificate: show, switch primary, run, detach, delete. */
function ChannelsDialog({
  certificate,
  onClose,
}: {
  certificate: Certificate;
  onClose: () => void;
}) {
  const { text } = useI18n();
  const client = useQueryClient();
  const sources = useQuery({
    queryKey: ["certificate-sources", certificate.id],
    queryFn: () => data(listSources(certificate.id)),
  });

  const refresh = () => client.invalidateQueries();
  const run = useAction(
    (id: number) => data(runSource(id)),
    text("拉取完成", "Pull finished"),
    () => void refresh(),
  );
  const promote = useAction(
    (id: number) => data(promoteSource(id)),
    text("已设为优先", "Preferred channel updated"),
    () => void refresh(),
  );
  const detach = useAction(
    (id: number) => data(attachSource(id, null)),
    text("已解除关联", "Channel detached"),
    () => void refresh(),
  );
  const remove = useAction(
    (id: number) => command(deleteSource(id)),
    text("已删除", "Channel deleted"),
    () => void refresh(),
  );

  return (
    <EditorDialog
      open
      title={text("更新方式", "Channels")}
      description={text(
        "这个证书的更新渠道。推送由证书管理器发起；自动拉取由本服务每天检查一次。",
        "How this certificate gets updated. Push is driven by the certificate manager; pull is checked once a day.",
      )}
      onClose={onClose}
    >
      <div className="space-y-3">
        {sources.isLoading && <p className="text-sm text-muted-foreground">…</p>}
        {(sources.data ?? []).length === 0 && (
          <p className="text-sm text-muted-foreground">
            {text(
              "还没有更新方式。用「导入证书 → 推送 / 自动拉取」新建一个。",
              "No channel yet. Create one from Import → Push / pull.",
            )}
          </p>
        )}
        {(sources.data ?? []).map((source) => (
          <ChannelRow
            key={source.id}
            source={source}
            busy={
              run.isPending || promote.isPending || detach.isPending || remove.isPending
            }
            onRun={() => run.mutate(source.id)}
            onPromote={() => promote.mutate(source.id)}
            onDetach={() => detach.mutate(source.id)}
            onDelete={() => remove.mutate(source.id)}
          />
        ))}
      </div>
    </EditorDialog>
  );
}

function ChannelRow({
  source,
  busy,
  onRun,
  onPromote,
  onDetach,
  onDelete,
}: {
  source: CertificateSource;
  busy: boolean;
  onRun: () => void;
  onPromote: () => void;
  onDetach: () => void;
  onDelete: () => void;
}) {
  const { text } = useI18n();
  const kindLabel = {
    push: text("推送", "Push"),
    certd: text("Certd 拉取", "Certd pull"),
    url: text("URL 拉取", "URL pull"),
  }[source.kind];

  return (
    <div className="space-y-2 rounded-md border p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2 text-sm font-medium">
            {kindLabel}
            {source.is_primary && (
              <Badge variant="secondary">{text("优先", "Preferred")}</Badge>
            )}
            {source.last_status === "failed" && (
              <Badge variant="destructive">{text("上次失败", "Last run failed")}</Badge>
            )}
          </div>
          <div className="text-xs text-muted-foreground">
            {source.domain || source.domains.join(", ")}
            {source.base_url && ` · ${source.base_url}`}
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          {source.kind !== "push" && (
            <Button variant="outline" size="sm" disabled={busy} onClick={onRun}>
              {text("立即拉取", "Fetch now")}
            </Button>
          )}
          {!source.is_primary && (
            <Button variant="outline" size="sm" disabled={busy} onClick={onPromote}>
              {text("设为优先", "Make preferred")}
            </Button>
          )}
          <Button variant="outline" size="sm" disabled={busy} onClick={onDetach}>
            {text("解除关联", "Detach")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            className="text-destructive"
            disabled={busy}
            onClick={onDelete}
          >
            {text("删除", "Delete")}
          </Button>
        </div>
      </div>
      {source.last_error && (
        <p className="text-xs text-destructive">{source.last_error}</p>
      )}
      {source.next_run_at && !source.last_error && (
        <p className="text-xs text-muted-foreground">
          {text("下次检查", "Next check")} {source.next_run_at.slice(0, 16)}
        </p>
      )}
    </div>
  );
}
