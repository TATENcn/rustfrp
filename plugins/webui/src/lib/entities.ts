import type { FrpsProfile, LocalProxy, LocalVisitor } from "@/api/types";
export const defaultProfile = (): FrpsProfile => ({
  name: "",
  server_addr: "",
  server_port: 7000,
  tls_enable: false,
  tls_cert_file: null,
  tls_key_file: null,
  tls_trusted_ca_file: null,
  transport_protocol: "tcp",
  heartbeat_interval: 30,
  heartbeat_timeout: 90,
  dial_server_timeout: null,
  dial_server_keepalive: null,
  connect_server_local_ip: null,
  proxy_url: null,
  pool_count: null,
  tcp_mux: null,
  tcp_mux_keepalive_interval: null,
  quic_keepalive_period: null,
  quic_max_idle_timeout: null,
  quic_max_incoming_streams: null,
  auth_method: "none",
  oidc_client_id: null,
  oidc_client_secret: null,
  oidc_token_endpoint_url: null,
  oidc_audience: null,
  oidc_scope: null,
  oidc_additional_endpoint_params: null,
  user: null,
  metadatas: null,
  login_fail_exit: null,
  start: null,
  dns_server: null,
  nat_hole_stun_server: null,
  udp_packet_size: null,
  includes: null,
  store_path: null,
  feature_gates: null,
  created_at: "",
  updated_at: "",
});
export const defaultProxy = (): LocalProxy => ({
  name: "",
  proxy_type: "tcp",
  local_ip: "127.0.0.1",
  local_port: 0,
  remote_port: null,
  custom_domains: null,
  subdomain: null,
  use_encryption: true,
  use_compression: true,
  bandwidth_limit: null,
  bandwidth_limit_mode: null,
  secret_key: null,
  locations: null,
  http_user: null,
  http_password: null,
  host_header_rewrite: null,
  request_headers: null,
  response_headers: null,
  route_by_http_user: null,
  annotations: null,
  metadatas: null,
  allow_users: null,
  nat_traversal_disable_assisted_addrs: null,
  proxy_protocol_version: null,
  health_check_type: null,
  health_check_timeout_s: 3,
  health_check_max_failed: 3,
  health_check_interval_s: 10,
  health_check_path: null,
  health_check_http_headers: null,
  plugin_config: null,
  created_at: "",
  updated_at: "",
});
export const defaultVisitor = (): LocalVisitor => ({
  name: "",
  visitor_type: "stcp",
  server_name: "",
  server_user: null,
  bind_addr: "127.0.0.1",
  bind_port: -1,
  secret_key: null,
  enabled: true,
  use_encryption: true,
  use_compression: true,
  profile_id: 0,
  created_at: "",
  updated_at: "",
});
export type Entity = FrpsProfile | LocalProxy | LocalVisitor;
export type EntityKind = "profiles" | "proxies" | "visitors";
export function parsePortSpec(spec: string): number[] {
  const ports = new Set<number>();
  for (const token of spec.split(/[\s,，]+/).filter(Boolean)) {
    const match = token.match(/^(\d+)(?:-(\d+))?$/);
    if (!match) throw new Error("invalid");
    const start = Number(match[1]),
      end = Number(match[2] || match[1]);
    if (start < 1 || end > 65535 || start > end) throw new Error("invalid");
    for (let port = start; port <= end; port++) {
      ports.add(port);
      if (ports.size > 100) throw new Error("too_many");
    }
  }
  return [...ports].sort((a, b) => a - b);
}
export function mappedAddresses(
  proxy: LocalProxy,
  profiles: FrpsProfile[],
  bindings: import("@/api/types").BindingRule[],
): string[] {
  return [
    ...new Set(
      bindings
        .filter((b) => b.enabled && b.proxy_id === proxy.id)
        .flatMap((b) => {
          const profile = profiles.find((p) => p.id === b.profile_id);
          if (!profile) return [];
          if (["tcp", "udp"].includes(proxy.proxy_type) && proxy.remote_port)
            return [`${profile.server_addr}:${proxy.remote_port}`];
          if (["http", "https"].includes(proxy.proxy_type))
            return (proxy.custom_domains ?? []).map(
              (domain) => `${proxy.proxy_type}://${domain}`,
            );
          return [];
        }),
    ),
  ];
}
