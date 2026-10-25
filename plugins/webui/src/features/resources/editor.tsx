import { useId, useState } from "react";
import { useI18n } from "@/i18n";
import { useProfiles } from "@/hooks/api";
import {
  profileFields,
  proxyFields,
  visitorFields,
  type Field as FieldSchema,
} from "@/lib/fields";
import type { Entity, EntityKind } from "@/lib/entities";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
  Field,
  FieldLabel,
  FieldDescription,
  FieldSet,
  FieldLegend,
} from "@/components/ui/field";
import { ActionButton, Choice, ErrorState } from "@/components/common/page";
const chinese: Record<string, string> = {
  name: "名称",
  server_addr: "服务器地址",
  server_port: "服务器端口",
  transport_protocol: "传输协议",
  auth_method: "认证方式",
  token: "Token",
  tls_enable: "启用 TLS",
  tls_cert_file: "TLS 证书路径",
  tls_key_file: "TLS 私钥路径",
  tls_trusted_ca_file: "可信 CA 路径",
  heartbeat_interval: "心跳间隔（秒）",
  heartbeat_timeout: "心跳超时（秒）",
  user: "用户前缀",
  login_fail_exit: "登录失败后退出",
  proxy_type: "协议类型",
  local_ip: "本地 IP",
  local_port: "本地端口",
  remote_port: "远程端口",
  use_encryption: "加密",
  use_compression: "压缩",
  custom_domains: "自定义域名",
  subdomain: "子域名",
  locations: "路径",
  http_user: "HTTP 用户名",
  http_password: "HTTP 密码",
  host_header_rewrite: "重写 Host 请求头",
  secret_key: "访问密钥",
  visitor_type: "访问类型",
  server_name: "服务名称",
  server_user: "服务用户",
  bind_addr: "绑定地址",
  bind_port: "绑定端口",
  profile_id: "服务器配置",
  enabled: "启用",
  oidc_client_id: "OIDC 客户端 ID",
  oidc_client_secret: "OIDC 客户端密钥",
  oidc_token_endpoint_url: "OIDC Token 端点",
  oidc_audience: "OIDC Audience",
  oidc_scope: "OIDC Scope",
};
export function ResourceEditor({
  kind,
  initial,
  busy,
  onSave,
  onCancel,
}: {
  kind: EntityKind;
  initial: Entity;
  busy?: boolean;
  onSave: (entity: Entity) => void;
  onCancel: () => void;
}) {
  const { t, text } = useI18n(),
    prefix = useId(),
    profiles = useProfiles();
  const [form, setForm] = useState<Record<string, unknown>>(() => {
    const value = { ...initial } as unknown as Record<string, unknown>;
    if (kind === "profiles" && initial.id) value.auth_method ??= "token";
    for (const key of ["custom_domains", "locations", "allow_users"])
      if (Array.isArray(value[key]))
        value[key] = (value[key] as string[]).join(", ");
    return value;
  });
  const fields =
    kind === "profiles"
      ? profileFields
      : kind === "proxies"
        ? proxyFields
        : visitorFields;
  const connectionKeys = new Set([
    "name",
    "server_addr",
    "server_port",
    "transport_protocol",
    "proxy_type",
    "local_ip",
    "local_port",
    "remote_port",
    "visitor_type",
    "server_name",
    "server_user",
    "bind_addr",
    "bind_port",
    "profile_id",
  ]);
  const sections = [
    {
      title: text("基本信息", "Connection"),
      description: text(
        "设置名称、连接地址与协议。",
        "Name, endpoint, and protocol for this connection.",
      ),
      fields: fields.filter((f) => connectionKeys.has(f.key)),
    },
    {
      title: text("安全与选项", "Security & options"),
      description: text(
        "配置认证、加密和运行参数。",
        "Authentication, encryption, and runtime options.",
      ),
      fields: fields.filter((f) => !connectionKeys.has(f.key)),
    },
  ];
  function control(field: FieldSchema) {
    const id = `${prefix}-${field.key}`,
      value = form[field.key],
      label = text(chinese[field.key] ?? field.label, field.label);
    const update = (v: unknown) =>
      setForm((old) => ({ ...old, [field.key]: v }));
    return (
      <Field
        key={field.key}
        orientation={field.kind === "switch" ? "horizontal" : "vertical"}
        className={
          field.key === "tls_enable"
            ? "p-4"
            : field.kind === "switch" ? "rounded-lg border p-3" : ""
        }
      >
        <FieldLabel
          htmlFor={id}
          className={field.kind === "switch" ? "flex-1" : "capitalize"}
        >
          {label}
          {field.required && <span className="text-muted-foreground"> *</span>}
        </FieldLabel>
        {field.kind === "switch" ? (
          <Switch
            id={id}
            checked={!!value}
            disabled={busy}
            aria-controls={field.key === "tls_enable" && value ? `${prefix}-tls-options` : undefined}
            onCheckedChange={update}
          />
        ) : field.kind === "select" ? (
          <Choice
            id={id}
            label={label}
            disabled={busy}
            value={
              field.key === "profile_id"
                ? Number(value) || undefined
                : String(value ?? "")
            }
            options={
              field.key === "profile_id"
                ? (profiles.data ?? []).map((p) => ({
                    value: p.id!,
                    label: p.name,
                  }))
                : (field.options ?? []).map((v) => ({
                    value: v,
                    label: v.toUpperCase(),
                  }))
            }
            onChange={(v) => update(field.key === "profile_id" ? Number(v) : v)}
          />
        ) : (
          <Input
            id={id}
            name={field.key}
            type={
              field.kind === "number"
                ? "number"
                : field.kind === "password"
                  ? "password"
                  : "text"
            }
            value={String(value ?? "")}
            required={field.required}
            min={field.min}
            max={field.max}
            autoComplete={field.kind === "password" ? "new-password" : "off"}
            placeholder={
              field.kind === "list"
                ? text("使用逗号分隔", "Comma-separated")
                : field.key === "token" && initial.id
                  ? t("profile.tokenKeep")
                  : undefined
            }
            onChange={(e) =>
              update(
                field.kind === "number"
                  ? e.target.value === ""
                    ? null
                    : Number(e.target.value)
                  : e.target.value,
              )
            }
          />
        )}
        {field.key === "token" && initial.id && (
          <FieldDescription>{t("profile.tokenKeep")}</FieldDescription>
        )}
      </Field>
    );
  }
  return (
    <form
      className="space-y-6"
      onSubmit={(e) => {
        e.preventDefault();
        if (busy) return;
        const payload = { ...form };
        for (const key of ["custom_domains", "locations", "allow_users"])
          if (typeof payload[key] === "string")
            payload[key] = (payload[key] as string)
              .split(",")
              .map((s) => s.trim())
              .filter(Boolean);
        onSave(payload as unknown as Entity);
      }}
    >
      {kind === "visitors" && (
        <ErrorState
          error={profiles.error}
          retry={() => void profiles.refetch()}
        />
      )}
      {sections.map((section) => (
        <FieldSet
          key={section.title}
          disabled={busy}
          className="gap-4 border-t pt-5"
        >
          <div>
            <FieldLegend className="mb-1">{section.title}</FieldLegend>
            <p className="text-xs text-muted-foreground">
              {section.description}
            </p>
          </div>
          <div className="grid items-start gap-x-5 gap-y-5 sm:grid-cols-2">
            {section.fields
              .filter((f) => f.kind !== "switch" && !f.key.startsWith("tls_") && (!f.when || f.when(form)))
              .map(control)}
          </div>
          {section.fields.some(
            (f) => f.kind === "switch" && f.key !== "tls_enable" && (!f.when || f.when(form)),
          ) && (
            <div className="grid items-stretch gap-3 sm:grid-cols-2">
              {section.fields
                .filter((f) => f.kind === "switch" && f.key !== "tls_enable" && (!f.when || f.when(form)))
                .map(control)}
            </div>
          )}
          {section.fields.some((f) => f.key === "tls_enable") && (
            <div className="overflow-hidden rounded-lg border">
              {control(section.fields.find((f) => f.key === "tls_enable")!)}
              {!!form.tls_enable && (
                <div id={`${prefix}-tls-options`} className="grid items-start gap-5 border-t p-4 sm:grid-cols-2">
                  {section.fields
                    .filter((f) => f.key.startsWith("tls_") && f.key !== "tls_enable" && (!f.when || f.when(form)))
                    .map(control)}
                </div>
              )}
            </div>
          )}
        </FieldSet>
      ))}
      <div className="sticky -bottom-6 -mx-6 flex justify-end gap-2 border-t bg-background px-6 py-4">
        <Button
          type="button"
          variant="outline"
          disabled={busy}
          onClick={onCancel}
        >
          {t("common.cancel")}
        </Button>
        <ActionButton
          type="submit"
          busy={busy}
          disabled={kind === "visitors" && !form.profile_id}
        >
          {t("common.save")}
        </ActionButton>
      </div>
    </form>
  );
}
