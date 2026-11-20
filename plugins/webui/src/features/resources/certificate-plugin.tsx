import { useEffect, useId, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listCertificates, type Certificate } from "@/api/certificates";
import { getStatus } from "@/api/system";
import { data } from "@/hooks/api";
import { useI18n } from "@/i18n";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { Choice } from "@/components/common/page";

/**
 * Structured editor for an https proxy's `plugin_config`.
 *
 * A `https2http` / `https2https` plugin only needs three values, but the raw
 * JSON box made operators hand-write `crtPath` / `keyPath` and remember that the
 * certificate must already exist on disk. This picks a managed certificate and
 * derives both paths from the directory the daemon can actually write to.
 *
 * The raw JSON stays available: the plugin may carry `requestHeaders` or
 * `enableHTTP2`, which this form does not model.
 */
export function CertificatePluginEditor({
  value,
  busy,
  onChange,
}: {
  value: unknown;
  busy?: boolean;
  onChange: (next: unknown) => void;
}) {
  const { text } = useI18n();
  const prefix = useId();

  const certificates = useQuery({
    queryKey: ["certificates"],
    queryFn: () => data(listCertificates()),
  });
  const status = useQuery({
    queryKey: ["status", null],
    queryFn: () => data(getStatus()),
    staleTime: 60_000,
  });

  const configDir = status.data?.config_dir ?? "~/.rustfrp/runtime";

  // Anything that is not a certificate plugin is left exactly as it is: this
  // editor must not overwrite a `http2https` or `socks5` plugin it does not model.
  const object = (typeof value === "object" && value !== null
    ? value
    : {}) as Record<string, unknown>;
  const type = String(object.type ?? "");
  // `tls2raw` terminates TLS in frpc and forwards raw bytes to a plain TCP
  // backend, so it holds the same `crtPath` / `keyPath` pair as the HTTPS pair.
  if (type !== "https2http" && type !== "https2https" && type !== "tls2raw") {
    return null;
  }

  const crtPath = String(object.crtPath ?? "");
  const keyPath = String(object.keyPath ?? "");
  const localAddr = String(object.localAddr ?? "");

  /** Default directory this editor assigns to one certificate's material. */
  const directoryFor = (domain: string) =>
    `${configDir.replace(/\/+$/, "")}/certs/${domain}`;

  // The selection is derived from the paths rather than stored: the plugin
  // object is FRP-native and has no room for a RustFRP-only id.
  const selected = (certificates.data ?? []).find(
    (certificate) =>
      crtPath === `${directoryFor(certificate.primary_domain)}/fullchain.pem`,
  );

  const patch = (next: Record<string, unknown>) =>
    onChange({ ...object, ...next });

  const selectCertificate = (certificate: Certificate) => {
    const base = directoryFor(certificate.primary_domain);
    patch({
      crtPath: `${base}/fullchain.pem`,
      keyPath: `${base}/privkey.pem`,
    });
  };

  return (
    <div className="space-y-4 rounded-lg border p-4">
      <div className="space-y-1">
        <h4 className="text-sm font-semibold">
          {type === "tls2raw"
            ? text("证书（tls2raw）", "Certificate (tls2raw)")
            : text("证书（https2http）", "Certificate (https2http)")}
        </h4>
        <p className="text-xs text-muted-foreground">
          {text(
            "frps 只按 SNI 转发，TLS 在 frpc 侧终止；这里指定 frpc 读取的证书文件。导入证书后到「证书」页绑定即可自动填写。",
            "frps only routes by SNI and TLS terminates at frpc, so these are the files frpc reads. Import a certificate and bind it on the Certificates page to fill them in.",
          )}
        </p>
      </div>

      <Field>
        <FieldLabel htmlFor={`${prefix}-cert`}>
          {text("已导入的证书", "Imported certificate")}
        </FieldLabel>
        <Choice
          id={`${prefix}-cert`}
          label={text("选择证书", "Pick a certificate")}
          disabled={busy}
          value={selected ? String(selected.id) : ""}
          options={(certificates.data ?? []).map((certificate) => ({
            value: String(certificate.id),
            label: `${certificate.primary_domain} · ${certificate.days_remaining}d`,
          }))}
          onChange={(id) => {
            const certificate = (certificates.data ?? []).find(
              (candidate) => String(candidate.id) === id,
            );
            if (certificate) selectCertificate(certificate);
          }}
        />
        <FieldDescription>
          {text(
            "选择后填入默认路径；实际生效需要先在证书页完成绑定。",
            "Selecting fills in the default paths. Making it live still requires a binding on the Certificates page.",
          )}
        </FieldDescription>
      </Field>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${prefix}-crt`}>crtPath</FieldLabel>
          <Input
            id={`${prefix}-crt`}
            disabled={busy}
            value={crtPath}
            placeholder={`${configDir}/certs/example.com/fullchain.pem`}
            onChange={(event) => patch({ crtPath: event.target.value })}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-key`}>keyPath</FieldLabel>
          <Input
            id={`${prefix}-key`}
            disabled={busy}
            value={keyPath}
            placeholder={`${configDir}/certs/example.com/privkey.pem`}
            onChange={(event) => patch({ keyPath: event.target.value })}
          />
        </Field>
      </div>

      <Field>
        <FieldLabel htmlFor={`${prefix}-local`}>localAddr</FieldLabel>
        <Input
          id={`${prefix}-local`}
          disabled={busy}
          value={localAddr}
          placeholder="127.0.0.1:8080"
          onChange={(event) => patch({ localAddr: event.target.value })}
        />
        <FieldDescription>
          {type === "tls2raw"
            ? text(
                "tls2raw 以明文 TCP 回源（如 DNS、MySQL、SMTP），TLS 只在 frpc 侧终止。",
                "tls2raw forwards plain TCP to the backend (DNS, MySQL, SMTP); TLS terminates only at frpc.",
              )
            : text(
                "也可以填 https2https，让 frpc 以 TLS 回源。",
                "Use https2https to have frpc speak TLS to the local service.",
              )}
        </FieldDescription>
      </Field>

      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={() =>
            onChange({ type: "https2http", localAddr: "127.0.0.1:80" })
          }
        >
          {text("重置为 https2http", "Reset to https2http")}
        </Button>
      </div>
    </div>
  );
}

/** Plugin type selector. Only the shapes RustFRP can act on are offered as
 * first-class rows; anything else is still reachable through the JSON box. */
export function PluginTypeControl({
  value,
  busy,
  onChange,
}: {
  value: unknown;
  busy?: boolean;
  onChange: (next: unknown) => void;
}) {
  const { text } = useI18n();
  const prefix = useId();
  const object = (typeof value === "object" && value !== null
    ? value
    : {}) as Record<string, unknown>;
  const type = String(object.type ?? "");

  return (
    <Field>
      <FieldLabel htmlFor={`${prefix}-type`}>
        {text("插件类型", "Plugin type")}
      </FieldLabel>
      <Choice
        id={`${prefix}-type`}
        label={text("插件类型", "Plugin type")}
        disabled={busy}
        value={type}
        options={[
          { label: text("无插件", "No plugin"), value: "" },
          { label: "https2http", value: "https2http" },
          { label: "https2https", value: "https2https" },
          { label: "tls2raw", value: "tls2raw" },
          { label: "http2https", value: "http2https" },
          { label: "http2http", value: "http2http" },
          { label: "socks5", value: "socks5" },
          { label: "static_file", value: "static_file" },
        ]}
        onChange={(next) =>
          onChange(
            next === ""
              ? null
              : {
                  ...object,
                  type: next,
                  localAddr:
                    object.localAddr ??
                    (next === "https2http" ? "127.0.0.1:80" : ""),
                },
          )
        }
      />
      <FieldDescription>
        {text(
          "清空即可移除插件配置。",
          "Choose “No plugin” to remove the plugin configuration.",
        )}
      </FieldDescription>
    </Field>
  );
}

/**
 * Raw JSON editor for the plugin block.
 *
 * Kept because the structured form covers certificate paths only: header
 * rewrites, `enableHTTP2`, and plugin-specific fields still need free editing.
 * Invalid JSON is reported inline and never replaces the stored value.
 */
export function JsonTextArea({
  value,
  busy,
  onChange,
}: {
  value: unknown;
  busy?: boolean;
  onChange: (next: unknown) => void;
}) {
  const { text } = useI18n();
  const serialized =
    value === null || value === undefined ? "" : JSON.stringify(value, null, 2);
  const [draft, setDraft] = useState(serialized);
  const [error, setError] = useState<string | null>(null);
  // Edits from the structured controls arrive as a new `value`. Adopt them,
  // but not the value this box itself just emitted, or typing would be reverted
  // on every keystroke.
  const emitted = useRef(serialized);
  useEffect(() => {
    if (serialized !== emitted.current) {
      emitted.current = serialized;
      setDraft(serialized);
      setError(null);
    }
  }, [serialized]);

  return (
    <Field>
      <FieldLabel htmlFor="plugin-json">
        {text("插件 JSON（高级）", "Plugin JSON (advanced)")}
      </FieldLabel>
      <textarea
        id="plugin-json"
        disabled={busy}
        className="h-32 w-full rounded-md border bg-transparent p-2 font-mono text-xs"
        value={draft}
        onChange={(event) => {
          const next = event.target.value;
          setDraft(next);
          if (!next.trim()) {
            setError(null);
            emitted.current = "";
            onChange(null);
            return;
          }
          try {
            const parsed = JSON.parse(next);
            emitted.current = JSON.stringify(parsed, null, 2);
            onChange(parsed);
            setError(null);
          } catch (parseError) {
            setError(
              parseError instanceof Error ? parseError.message : String(parseError),
            );
          }
        }}
      />
      {error ? (
        <FieldDescription className="text-destructive">{error}</FieldDescription>
      ) : (
        <FieldDescription>
          {text(
            "留空表示不使用插件。",
            "Leave empty for no plugin.",
          )}
        </FieldDescription>
      )}
    </Field>
  );
}
