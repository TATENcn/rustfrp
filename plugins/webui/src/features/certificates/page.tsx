import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  applyCertificate,
  bindCertificate,
  createIngestToken,
  createPullSource,
  deleteCertificate,
  deleteIngestToken,
  deletePullSource,
  importFromPath,
  importPem,
  listCertificates,
  listIngestTokens,
  listPullSources,
  runPullSource,
  unbindCertificate,
  type Certificate,
  type CreatedIngestToken,
  type PullKind,
  type PullSource,
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
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { DataTable, type Column } from "@/components/common/data-table";
import {
  Choice,
  EmptyState,
  ErrorState,
  PageHeading,
  PageLoading,
} from "@/components/common/page";
import { DeleteDialog, EditorDialog } from "@/components/common/dialogs";
import { getIngestSettings, IngestSettingsCard } from "./ingest-settings";

export default function Certificates() {
  const { t, text } = useI18n();
  const [importing, setImporting] = useState(false);
  const [deleting, setDeleting] = useState<Certificate | null>(null);
  const [sources, setSources] = useState(false);

  const certificates = useQuery({
    queryKey: ["certificates"],
    queryFn: () => data(listCertificates()),
  });

  const remove = useAction(
    (id: number) => command(deleteCertificate(id)),
    text("证书已删除", "Certificate deleted"),
  );

  if (certificates.isLoading) return <PageLoading />;
  if (certificates.error)
    return (
      <ErrorState
        error={certificates.error}
        retry={() => void certificates.refetch()}
      />
    );

  const rows = certificates.data ?? [];

  const columns: Column<Certificate>[] = [
    {
      key: "domain",
      title: text("域名", "Domain"),
      sort: (row) => row.primary_domain,
      cell: (row) => (
        <div className="space-y-1">
          <div className="font-medium">{row.primary_domain}</div>
          {row.sans.length > 1 && (
            <div className="text-xs text-muted-foreground">
              {row.sans.filter((san) => san !== row.primary_domain).join(", ")}
            </div>
          )}
        </div>
      ),
    },
    {
      key: "expiry",
      title: text("到期", "Expires"),
      sort: (row) => row.days_remaining,
      cell: (row) => (
        <div className="space-y-1">
          <Badge variant={expiryVariant(row.days_remaining)}>
            {expiryLabel(row.days_remaining, text)}
          </Badge>
          <div className="text-xs text-muted-foreground">
            {row.not_after.slice(0, 10)}
          </div>
        </div>
      ),
    },
    {
      key: "source",
      title: text("来源", "Source"),
      sort: (row) => row.source,
      cell: (row) => (
        <div className="text-sm">
          {sourceLabel(row.source, text)}
          {row.source_detail && (
            <div className="max-w-56 truncate text-xs text-muted-foreground">
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
      cell: (row) => (
        <div className="space-y-1 text-sm">
          <div>{row.key_algo}</div>
          <div className="font-mono text-xs text-muted-foreground">
            {row.fingerprint_sha256.slice(0, 16)}
          </div>
        </div>
      ),
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
                ) : binding.applied_fingerprint === row.fingerprint_sha256 ? (
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
      cell: (row) => (
        <div className="flex flex-wrap gap-2">
          <ApplyButton certId={row.id} disabled={row.bindings.length === 0} />
          <BindButton certificate={row} />
          <Button
            variant="outline"
            size="sm"
            className="text-destructive"
            onClick={() => setDeleting(row)}
          >
            {t("common.delete")}
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeading
        title={text("证书", "Certificates")}
        description={text(
          "为 https 代理提供域名证书。只支持导入与查看，不提供下载或导出。",
          "Domain certificates for https proxies. Import and inspect only — no download or export.",
        )}
      >
        <Button onClick={() => setImporting(true)}>
          <AppIcon name="plus" />
          {text("导入证书", "Import")}
        </Button>
      </PageHeading>

      {rows.length === 0 ? (
        <EmptyState
          title={text("还没有证书", "No certificates yet")}
          description={text(
            "粘贴 PEM、上传文件、引用本机路径，或配置自动拉取。",
            "Paste PEM, upload a file, reference a local path, or set up an automatic pull.",
          )}
        />
      ) : (
        <DataTable rows={rows} columns={columns} rowKey={(row) => row.id} />
      )}

      <SourceSummaryCards onOpen={() => setSources(true)} />

      <ImportDialog open={importing} onClose={() => setImporting(false)} />

      <EditorDialog
        open={sources}
        title={text("证书来源", "Certificate sources")}
        description={text(
          "接收端点接受 Certd 推送；自动拉取从 Certd 或任意 HTTPS 地址取回证书。",
          "The ingest endpoint accepts Certd pushes; automatic pull fetches from Certd or any HTTPS endpoint.",
        )}
        onClose={() => setSources(false)}
      >
        <Tabs defaultValue="ingest" className="space-y-4">
          <TabsList>
            <TabsTrigger value="ingest">
              {text("接收端点", "Ingest endpoint")}
            </TabsTrigger>
            <TabsTrigger value="pull">
              {text("自动拉取", "Automatic pull")}
            </TabsTrigger>
          </TabsList>
          <TabsContent value="ingest">
            <IngestPanel />
          </TabsContent>
          <TabsContent value="pull">
            <PullPanel />
          </TabsContent>
        </Tabs>
      </EditorDialog>

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

function ApplyButton({ certId, disabled }: { certId: number; disabled: boolean }) {
  const { text } = useI18n();
  const apply = useAction(
    (id: number) => data(applyCertificate(id)),
    text("证书已写入配置文件", "Certificate written to the configured files"),
  );
  return (
    <Button
      variant="outline"
      size="sm"
      disabled={disabled || apply.isPending}
      onClick={() => apply.mutate(certId)}
    >
      <AppIcon name="refresh" className={apply.isPending ? "animate-spin" : ""} />
      {text("应用", "Apply")}
    </Button>
  );
}

function BindButton({ certificate }: { certificate: Certificate }) {
  const { text } = useI18n();
  const proxies = useProxies();
  const [open, setOpen] = useState(false);
  const [proxyId, setProxyId] = useState("");

  // Only proxies that can actually hold a certificate are offered.
  const candidates = (proxies.data ?? []).filter(
    (proxy) =>
      proxy.proxy_type === "https" &&
      typeof proxy.plugin_config === "object" &&
      proxy.plugin_config !== null &&
      ["https2http", "https2https"].includes(
        String((proxy.plugin_config as Record<string, unknown>).type ?? ""),
      ),
  );
  const bound = certificate.bindings.find((binding) => binding.id);
  const bind = useAction(
    (body: { cert_id: number; proxy_id: number }) => data(bindCertificate(body)),
    text("已绑定", "Bound"),
    () => {
      setOpen(false);
      setProxyId("");
    },
  );
  const unbind = useAction(
    (id: number) => command(unbindCertificate(id, false)),
    text("已解绑，目标文件已保留", "Unbound; files kept on disk"),
  );

  return (
    <>
      {bound ? (
        <Button
          variant="outline"
          size="sm"
          disabled={unbind.isPending}
          onClick={() => unbind.mutate(bound.id)}
        >
          {text("解绑", "Unbind")}
        </Button>
      ) : (
        <Button variant="outline" size="sm" onClick={() => setOpen(true)}>
          {text("绑定", "Bind")}
        </Button>
      )}
      <EditorDialog
        open={open}
        title={text("绑定到代理", "Bind to a proxy")}
        description={text(
          "只列出配置了 https2http / https2https 插件的代理。证书会写入该代理声明的 crtPath 与 keyPath。",
          "Only proxies with an https2http / https2https plugin are listed. The certificate is written to the paths that plugin declares.",
        )}
        busy={bind.isPending}
        onClose={() => setOpen(false)}
      >
        <div className="space-y-4">
          {candidates.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              {text(
                "没有可用的 https 代理。请先在代理规则中把类型设为 https，并添加 https2http 插件与证书路径。",
                "No eligible https proxy. Set a proxy's type to https and add an https2http plugin with certificate paths first.",
              )}
            </p>
          ) : (
            <Choice
              label={text("代理", "Proxy")}
              value={proxyId}
              options={candidates.map((proxy) => ({
                label: proxy.name,
                value: String(proxy.id),
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
    </>
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

  const reset = () => {
    setCert("");
    setKey("");
    setCrtPath("");
    setKeyPath("");
    setNote("");
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

  return (
    <EditorDialog
      open={open}
      title={text("导入证书", "Import certificate")}
      description={text(
        "证书与私钥只存于本机，加密保存；界面不会再次显示内容，也不提供下载。",
        "The certificate and key stay on this host, encrypted at rest. They are never shown again and cannot be downloaded.",
      )}
      busy={importPasted.isPending || importPath.isPending}
      onClose={onClose}
    >
      <Tabs value={tab} onValueChange={setTab} className="space-y-4">
        <TabsList>
          <TabsTrigger value="pem">{text("粘贴 PEM", "Paste PEM")}</TabsTrigger>
          <TabsTrigger value="path">{text("本机路径", "Local path")}</TabsTrigger>
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
      </Tabs>
    </EditorDialog>
  );
}

/** Two status cards for the push/pull channels; each opens the dialog. */
function SourceSummaryCards({ onOpen }: { onOpen: () => void }) {
  const { text } = useI18n();
  const ingest = useQuery({
    queryKey: ["cert-ingest"],
    queryFn: () => data(getIngestSettings()),
  });
  const pull = useQuery({
    queryKey: ["pull-sources"],
    queryFn: () => data(listPullSources()),
    retry: false,
  });
  const pulls = pull.data ?? [];
  const pulling = pulls.filter((source: PullSource) => !source.last_error).length;
  const cards: [string, string, string, string][] = [
    [
      text("接收端点（推送）", "Ingest endpoint (push)"),
      ingest.data?.address
        ? text("监听中", "Listening")
        : text("已停止", "Stopped"),
      ingest.data?.address
        ? ingest.data.url ??
          `:${ingest.data.port}/ingest/v1/certs`
        : text("默认仅本机 7901 端口可用", "Loopback 7901 by default"),
      ingest.data?.error ?? "",
    ],
    [
      text("自动拉取", "Automatic pull"),
      pulls.length
        ? text(`${pulling} / ${pulls.length} 个源`, `${pulling} / ${pulls.length} sources`)
        : text("未配置", "Not configured"),
      pulls.length
        ? (pulls[0] as PullSource).base_url
        : text("从 Certd 或任意 HTTPS 地址定时取回", "Fetch from Certd or any HTTPS endpoint"),
      pulls.find((source: PullSource) => source.last_error)?.last_error ?? "",
    ],
  ];
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {cards.map(([title, badge, detail, error]) => (
        <Card key={title} className="shadow-none">
          <CardHeader className="flex-row items-start justify-between gap-2 [.border-b]:pb-0">
            <div className="space-y-1">
              <CardTitle className="text-sm">{title}</CardTitle>
              <CardDescription className="truncate">{detail}</CardDescription>
            </div>
            <Badge variant={badge === text("已停止", "Stopped") ? "outline" : "secondary"}>
              {badge}
            </Badge>
          </CardHeader>
          <CardContent className="flex items-center justify-between gap-2">
            {error ? (
              <p className="truncate text-xs text-destructive" title={error}>
                {error}
              </p>
            ) : (
              <span className="text-xs text-muted-foreground">
                {text("正常", "OK")}
              </span>
            )}
            <Button variant="outline" size="sm" onClick={onOpen}>
              {text("配置", "Configure")}
            </Button>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function IngestPanel() {
  const { text } = useI18n();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [domains, setDomains] = useState("");
  const [created, setCreated] = useState<CreatedIngestToken | null>(null);

  const tokens = useQuery({
    queryKey: ["ingest-tokens"],
    queryFn: () => data(listIngestTokens()),
  });
  // Created with useMutation rather than useAction: the plaintext token exists
  // only in this response, so it has to be captured here and shown once.
  const client = useQueryClient();
  const create = useMutation({
    mutationFn: (body: { name: string; domains: string[] }) =>
      data(createIngestToken(body)),
    onSuccess: async (result) => {
      setCreated(result);
      setCreating(false);
      setName("");
      setDomains("");
      toast.success(text("接收令牌已创建", "Ingest token created"));
      await client.invalidateQueries();
    },
    onError: async (error) => {
      toast.error(extractApiError(error).message);
      await client.invalidateQueries();
    },
  });
  const remove = useAction(
    (id: number) => command(deleteIngestToken(id)),
    text("令牌已删除", "Token deleted"),
  );

  // The push URL is derived from whatever the listener is bound to, so the
  // snippet cannot drift from the configured port or exposure.
  const ingestSettings = useQuery({
    queryKey: ["cert-ingest"],
    queryFn: () => data(getIngestSettings()),
  });
  const ingestPort = ingestSettings.data?.port ?? 7901;
  const ingestHost = ingestSettings.data?.expose
    ? window.location.hostname
    : (ingestSettings.data?.address?.split(":")[0] ?? window.location.hostname);
  const ingestOrigin = `http://${ingestHost}:${ingestPort}`;

  return (
    <div className="space-y-4 rounded-lg border p-4">
      <IngestSettingsCard />

      {created?.plaintext && (
        <div className="space-y-2 rounded-md border border-amber-500/40 bg-amber-500/5 p-3">
          <p className="text-sm font-medium">
            {text("令牌只显示这一次", "This token is shown once")}
          </p>
          <code className="block break-all rounded bg-background p-2 font-mono text-xs">
            {created.plaintext}
          </code>
          <p className="text-xs text-muted-foreground">
            {text("Certd 的 webhook 部署任务按下面填写；消息体保持默认即可。",
              "Configure the Certd webhook deploy task as below; leave its body template at the default.")}
          </p>
          <div className="space-y-1 font-mono text-xs">
            <div>POST {ingestOrigin}/ingest/v1/certs</div>
            <div>Content-Type: application/json</div>
            <div>X-Cert-Ingest-Token: {created.plaintext}</div>
          </div>
          <Button variant="outline" size="sm" onClick={() => setCreated(null)}>
            {text("我已保存", "Saved it")}
          </Button>
        </div>
      )}

      {creating ? (
        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="token-name">{text("名称", "Name")}</Label>
            <Input
              id="token-name"
              placeholder="certd"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="token-domains">
              {text("允许的域名（逗号分隔，支持 *.example.com）", "Allowed domains (comma separated, *.example.com supported)")}
            </Label>
            <Input
              id="token-domains"
              placeholder="example.com, *.example.com"
              value={domains}
              onChange={(event) => setDomains(event.target.value)}
            />
          </div>
          <div className="flex gap-2">
            <Button
              disabled={!name.trim() || !domains.trim() || create.isPending}
              onClick={() =>
                create.mutate({
                  name: name.trim(),
                  domains: domains
                    .split(",")
                    .map((domain) => domain.trim())
                    .filter(Boolean),
                })
              }
            >
              {text("创建令牌", "Create token")}
            </Button>
            <Button variant="outline" onClick={() => setCreating(false)}>
              {text("取消", "Cancel")}
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="outline" onClick={() => setCreating(true)}>
          <AppIcon name="plus" />
          {text("创建接收令牌", "Create ingest token")}
        </Button>
      )}

      {(tokens.data ?? []).length > 0 && (
        <div className="space-y-2">
          {(tokens.data ?? []).map((token) => (
            <div
              key={token.id}
              className="flex flex-wrap items-center justify-between gap-2 rounded-md border p-2 text-sm"
            >
              <div>
                <div className="font-medium">{token.name}</div>
                <div className="text-xs text-muted-foreground">
                  {token.domains.join(", ")} · {text("使用", "used")} {token.use_count}{" "}
                  {text("次", "times")}
                  {token.last_used_at && ` · ${token.last_used_at.slice(0, 19)}`}
                </div>
              </div>
              <Button
                variant="outline"
                size="sm"
                className="text-destructive"
                disabled={remove.isPending}
                onClick={() => remove.mutate(token.id)}
              >
                {text("删除", "Delete")}
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function PullPanel() {
  const { text } = useI18n();
  const [creating, setCreating] = useState(false);
  const [kind, setKind] = useState<PullKind>("certd");
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [domain, setDomain] = useState("");
  const [keyId, setKeyId] = useState("");
  const [keySecret, setKeySecret] = useState("");

  const client = useQueryClient();
  const sources = useQuery({
    queryKey: ["pull-sources"],
    queryFn: () => data(listPullSources()),
  });
  const create = useAction(
    (body: Parameters<typeof createPullSource>[0]) => data(createPullSource(body)),
    text("拉取源已创建", "Pull source created"),
    () => {
      setCreating(false);
      setName("");
      setBaseUrl("");
      setDomain("");
      setKeyId("");
      setKeySecret("");
    },
  );
  const remove = useAction(
    (id: number) => command(deletePullSource(id)),
    text("拉取源已删除", "Pull source deleted"),
  );
  // Shows what the pull actually produced (domain and expiry) instead of a
  // generic toast, and leaves error reporting to one place.
  const run = useMutation({
    mutationFn: (id: number) => data(runPullSource(id)),
    onSuccess: async (report) => {
      toast.success(report.message);
      await client.invalidateQueries();
    },
    onError: async (error) => {
      toast.error(extractApiError(error).message);
      await client.invalidateQueries();
    },
  });

  const reset = () => {
    setName("");
    setBaseUrl("");
    setDomain("");
    setKeyId("");
    setKeySecret("");
  };

  return (
    <div className="space-y-4 rounded-lg border p-4">
      <div className="space-y-1">
        <h3 className="text-sm font-semibold">{text("自动拉取（Pull）", "Automatic pull")}</h3>
        <p className="text-sm text-muted-foreground">
          {text(
            "从 Certd 开放接口或任意 HTTPS 地址取回证书。不需要把本机暴露出去。",
            "Fetch certificates from the Certd open API or any HTTPS endpoint. This host stays unexposed.",
          )}
        </p>
      </div>

      {creating ? (
        <div className="space-y-3">
          <Choice
            label={text("类型", "Type")}
            value={kind}
            options={[
              { label: "Certd", value: "certd" },
              { label: text("通用 URL", "Generic URL"), value: "url" },
            ]}
            onChange={(value) => setKind(value as PullKind)}
          />
          <div className="space-y-1.5">
            <Label htmlFor="pull-name">{text("名称", "Name")}</Label>
            <Input id="pull-name" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pull-url">
              {kind === "certd" ? "Certd 地址" : text("证书地址", "Certificate URL")}
            </Label>
            <Input
              id="pull-url"
              placeholder={kind === "certd" ? "https://certd.example.com" : "https://example.com/fullchain.pem"}
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pull-domain">{text("域名", "Domain")}</Label>
            <Input
              id="pull-domain"
              placeholder="www.example.com"
              value={domain}
              onChange={(e) => setDomain(e.target.value)}
            />
          </div>
          {kind === "certd" && (
            <>
              <div className="space-y-1.5">
                <Label htmlFor="pull-key-id">Key ID</Label>
                <Input id="pull-key-id" value={keyId} onChange={(e) => setKeyId(e.target.value)} />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="pull-key-secret">Key Secret</Label>
                <Input
                  id="pull-key-secret"
                  type="password"
                  value={keySecret}
                  onChange={(e) => setKeySecret(e.target.value)}
                />
              </div>
            </>
          )}
          <div className="flex gap-2">
            <Button
              disabled={!name.trim() || !baseUrl.trim() || !domain.trim() || create.isPending}
              onClick={() =>
                create.mutate({
                  name: name.trim(),
                  kind,
                  base_url: baseUrl.trim(),
                  domain: domain.trim(),
                  key_id: keyId.trim() || undefined,
                  key_secret: keySecret || undefined,
                })
              }
            >
              {text("保存", "Save")}
            </Button>
            <Button
              variant="outline"
              onClick={() => {
                setCreating(false);
                reset();
              }}
            >
              {text("取消", "Cancel")}
            </Button>
          </div>
        </div>
      ) : (
        <Button variant="outline" onClick={() => setCreating(true)}>
          <AppIcon name="plus" />
          {text("添加拉取源", "Add pull source")}
        </Button>
      )}

      {(sources.data ?? []).map((source: PullSource) => (
        <div
          key={source.id}
          className="flex flex-wrap items-center justify-between gap-2 rounded-md border p-2 text-sm"
        >
          <div className="space-y-1">
            <div className="font-medium">
              {source.name} <Badge variant="outline">{source.kind}</Badge>
            </div>
            <div className="text-xs text-muted-foreground">
              {source.base_url} · {source.domain}
              {source.last_run_at && ` · ${source.last_run_at.slice(0, 19)}`}
            </div>
            {source.last_error && (
              <div className="text-xs text-destructive">{source.last_error}</div>
            )}
          </div>
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={run.isPending}
              onClick={() => run.mutate(source.id)}
            >
              {text("立即拉取", "Pull now")}
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="text-destructive"
              disabled={remove.isPending}
              onClick={() => remove.mutate(source.id)}
            >
              {text("删除", "Delete")}
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
}

function expiryVariant(days: number): "default" | "secondary" | "destructive" | "outline" {
  if (days < 0) return "destructive";
  if (days <= 7) return "destructive";
  if (days <= 30) return "secondary";
  return "outline";
}

function expiryLabel(
  days: number,
  text: (zh: string, en: string) => string,
): string {
  if (days < 0) return text("已过期", "Expired");
  if (days === 0) return text("今天到期", "Expires today");
  return text(`${days} 天`, `${days} days`);
}

function sourceLabel(
  source: string,
  text: (zh: string, en: string) => string,
): string {
  switch (source) {
    case "manual":
      return text("手动粘贴", "Pasted");
    case "upload":
      return text("上传文件", "Uploaded");
    case "local_path":
      return text("本机路径", "Local path");
    case "webhook":
      return text("Webhook 推送", "Webhook push");
    case "certd_pull":
      return "Certd";
    case "url_pull":
      return text("URL 拉取", "URL pull");
    default:
      return source;
  }
}
