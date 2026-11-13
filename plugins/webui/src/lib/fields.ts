export type Field = {
  key: string;
  label: string;
  kind?: "number" | "switch" | "password" | "select" | "list";
  required?: boolean;
  min?: number;
  max?: number;
  options?: string[];
  when?: (form: Record<string, unknown>) => boolean;
};
export const profileFields: Field[] = [
  { key: "name", label: "Name", required: true },
  { key: "server_addr", label: "Server Address", required: true },
  {
    key: "server_port",
    label: "Server Port",
    kind: "number",
    required: true,
    min: 1,
    max: 65535,
  },
  {
    key: "transport_protocol",
    label: "Transport Protocol",
    kind: "select",
    options: ["tcp", "kcp", "wss", "quic"],
  },
  {
    key: "auth_method",
    label: "Auth Method",
    kind: "select",
    options: ["none", "token", "oidc"],
  },
  {
    key: "token",
    label: "Token",
    kind: "password",
    when: (f) => f.auth_method === "token",
  },
  ...[
    "oidc_client_id",
    "oidc_client_secret",
    "oidc_token_endpoint_url",
    "oidc_audience",
    "oidc_scope",
  ].map((key) => ({
    key,
    label: key.replaceAll("_", " "),
    kind: key.endsWith("secret") ? ("password" as const) : undefined,
    when: (f: Record<string, unknown>) => f.auth_method === "oidc",
  })),
  { key: "tls_enable", label: "TLS Enable", kind: "switch" },
  ...["tls_cert_file", "tls_key_file", "tls_trusted_ca_file"].map((key) => ({
    key,
    label: key.replaceAll("_", " "),
    when: (f: Record<string, unknown>) => !!f.tls_enable,
  })),
  {
    key: "heartbeat_interval",
    label: "Heartbeat Interval (s)",
    kind: "number",
    min: 1,
    required: true,
  },
  {
    key: "heartbeat_timeout",
    label: "Heartbeat Timeout (s)",
    kind: "number",
    min: 1,
    required: true,
  },
  { key: "user", label: "User (prefix)" },
  { key: "login_fail_exit", label: "Login Fail Exit", kind: "switch" },
  { key: "client_id", label: "Client ID" },
  {
    key: "wire_protocol",
    label: "Wire Protocol",
    kind: "select",
    options: ["v1", "v2"],
  },
  {
    key: "auth_additional_scopes",
    label: "Auth Additional Scopes",
    kind: "list",
    when: (f) => f.auth_method !== "none",
  },
  {
    key: "tls_server_name",
    label: "TLS Server Name",
    when: (f) => !!f.tls_enable,
  },
  {
    key: "tls_disable_custom_first_byte",
    label: "TLS Disable Custom First Byte",
    kind: "switch",
    when: (f) => !!f.tls_enable,
  },
  {
    key: "oidc_additional_params",
    label: "OIDC Additional Endpoint Params (JSON)",
    when: (f) => f.auth_method === "oidc",
  },
  {
    key: "oidc_trusted_ca_file",
    label: "OIDC Trusted CA File",
    when: (f) => f.auth_method === "oidc",
  },
  {
    key: "oidc_insecure_skip_verify",
    label: "OIDC Insecure Skip Verify",
    kind: "switch",
    when: (f) => f.auth_method === "oidc",
  },
  {
    key: "oidc_proxy_url",
    label: "OIDC Proxy URL",
    when: (f) => f.auth_method === "oidc",
  },
  { key: "start", label: "Start (proxy allowlist)", kind: "list" },
  { key: "dns_server", label: "DNS Server" },
  { key: "nat_hole_stun_server", label: "NAT Hole STUN Server" },
  { key: "virtual_net_address", label: "Virtual Net Address" },
  {
    key: "log_level",
    label: "Log Level",
    kind: "select",
    options: ["trace", "debug", "info", "warn", "error"],
  },
  { key: "log_max_days", label: "Log Max Days", kind: "number", min: 1 },
];
export const proxyFields: Field[] = [
  { key: "name", label: "Name", required: true },
  {
    key: "proxy_type",
    label: "Type",
    kind: "select",
    options: ["tcp", "udp", "http", "https", "stcp", "xtcp", "tcpmux", "sudp"],
  },
  { key: "local_ip", label: "Local IP", required: true },
  {
    key: "local_port",
    label: "Local Port",
    kind: "number",
    required: true,
    min: 1,
    max: 65535,
  },
  {
    key: "remote_port",
    label: "Remote Port",
    kind: "number",
    min: 1,
    max: 65535,
  },
  { key: "use_encryption", label: "Encryption", kind: "switch" },
  { key: "use_compression", label: "Compression", kind: "switch" },
  ...[
    "custom_domains",
    "subdomain",
    "locations",
    "http_user",
    "http_password",
    "host_header_rewrite",
  ].map((key) => ({
    key,
    label: key.replaceAll("_", " "),
    kind: ["custom_domains", "locations"].includes(key)
      ? ("list" as const)
      : key === "http_password"
        ? ("password" as const)
        : undefined,
    when: (f: Record<string, unknown>) =>
      ["http", "https"].includes(String(f.proxy_type)),
  })),
  {
    key: "secret_key",
    label: "Secret Key",
    kind: "password",
    when: (f) => ["stcp", "xtcp", "sudp"].includes(String(f.proxy_type)),
  },
  {
    key: "multiplexer",
    label: "Multiplexer",
    kind: "select",
    options: ["httpconnect"],
    required: true,
    when: (f) => f.proxy_type === "tcpmux",
  },
];
export const visitorFields: Field[] = [
  { key: "name", label: "Name", required: true },
  {
    key: "visitor_type",
    label: "Type",
    kind: "select",
    options: ["stcp", "sudp", "xtcp"],
  },
  { key: "server_name", label: "Server Name", required: true },
  { key: "server_user", label: "Server User" },
  { key: "bind_addr", label: "Bind Address" },
  {
    key: "bind_port",
    label: "Bind Port",
    kind: "number",
    min: -1,
    max: 65535,
    required: true,
  },
  { key: "secret_key", label: "Secret Key", kind: "password" },
  { key: "profile_id", label: "Profile", kind: "select", required: true },
  { key: "enabled", label: "Enabled", kind: "switch" },
  { key: "use_encryption", label: "Encryption", kind: "switch" },
  { key: "use_compression", label: "Compression", kind: "switch" },
];
