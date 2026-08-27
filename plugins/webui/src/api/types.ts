// ── API Envelope ──
export interface ApiEnvelope<T> {
  success: boolean
  data?: T
  count?: number
  error?: ApiError
}

export interface ApiError {
  code: string
  message: string
  user_message_key: string
}

// ── ProxyType (matches Rust ProxyType enum) ──
export type ProxyType =
  | 'tcp'
  | 'udp'
  | 'http'
  | 'https'
  | 'stcp'
  | 'xtcp'
  | 'tcpmux'
  | 'sudp'

// ── VisitorType (matches Rust VisitorType enum) ──
export type VisitorType = 'stcp' | 'sudp' | 'xtcp'

// ── FrpsProfile (matches Rust FrpsProfile 1:1) ──
export interface FrpsProfile {
  id?: number
  name: string
  server_addr: string
  server_port: number
    // `token` is #[serde(skip_serializing)] — not in API responses;
  // send-only on create/update. Empty string on update = keep existing.
  token?: string
  tls_enable: boolean
  tls_cert_file?: string | null
  tls_key_file?: string | null
  tls_trusted_ca_file?: string | null
  transport_protocol: string
  heartbeat_interval: number
  heartbeat_timeout: number
  dial_server_timeout?: number | null
  dial_server_keepalive?: number | null
  connect_server_local_ip?: string | null
  proxy_url?: string | null
  pool_count?: number | null
  tcp_mux?: boolean | null
  tcp_mux_keepalive_interval?: number | null
  quic_keepalive_period?: number | null
  quic_max_idle_timeout?: number | null
  quic_max_incoming_streams?: number | null
  auth_method?: string | null
  oidc_client_id?: string | null
  oidc_client_secret?: string | null
  oidc_token_endpoint_url?: string | null
  oidc_audience?: string | null
  oidc_scope?: string | null
  oidc_additional_endpoint_params?: string | null
  user?: string | null
  metadatas?: string | null
  login_fail_exit?: boolean | null
  start?: string[] | null
  dns_server?: string | null
  nat_hole_stun_server?: string | null
  udp_packet_size?: number | null
  includes?: string[] | null
  store_path?: string | null
  feature_gates?: string | null
  created_at: string
  updated_at: string
}

// ── HttpHeader ──
export interface HttpHeader {
  name: string
  value: string
}

// ── LocalProxy (matches Rust LocalProxy 1:1) ──
export interface LocalProxy {
  id?: number
  name: string
  proxy_type: ProxyType
  local_ip: string
  local_port: number
  remote_port?: number | null
  custom_domains?: string[] | null
  subdomain?: string | null
  use_encryption: boolean
  use_compression: boolean
  bandwidth_limit?: string | null
  bandwidth_limit_mode?: string | null
  secret_key?: string | null
  locations?: string[] | null
  http_user?: string | null
  http_password?: string | null
  host_header_rewrite?: string | null
  request_headers?: string | null
  response_headers?: string | null
  route_by_http_user?: string | null
  annotations?: string | null
  metadatas?: string | null
  allow_users?: string[] | null
  nat_traversal_disable_assisted_addrs?: boolean | null
  proxy_protocol_version?: string | null
  health_check_type?: string | null
  health_check_timeout_s: number
  health_check_max_failed: number
  health_check_interval_s: number
  health_check_path?: string | null
  health_check_http_headers?: HttpHeader[] | null
  plugin_config?: Record<string, unknown> | null
  created_at: string
  updated_at: string
}

// ── BindingRule (matches Rust BindingRule 1:1) ──
export interface BindingRule {
  id?: number
  profile_id: number
  proxy_id: number
  enabled: boolean   // 配置是否已完成、允许被启动（资格）
  running: boolean   // 代理是否正在 frpc 进程中运行（事实）
  priority: number
  group_name?: string | null
  group_key?: string | null
  created_at: string
  updated_at: string
}

// ── Binding start/stop response ──
export interface BindingControlResponse {
  binding_id: number
  running: boolean
  process_status: 'started' | 'reloaded' | 'already_running' | 'stopped' | 'not_running'
  profile_id: number
  profile_name: string
}

// ── LocalVisitor (matches Rust LocalVisitor 1:1) ──
export interface LocalVisitor {
  id?: number
  name: string
  visitor_type: VisitorType
  server_name: string
  server_user?: string | null
  bind_addr?: string | null
  bind_port: number
  secret_key?: string | null
  enabled: boolean
  use_encryption: boolean
  use_compression: boolean
  xtcp_protocol?: string | null
  keep_tunnel_open?: boolean | null
  max_retries_an_hour?: number | null
  min_retry_interval?: number | null
  fallback_to?: string | null
  fallback_timeout_ms?: number | null
  plugin_config?: Record<string, unknown> | null
  profile_id: number
  annotations?: string | null
  created_at: string
  updated_at: string
}

// ── System Status ──
export interface ProcessInfo {
  profile_id: number
  profile_name: string
  pid: number | null
  running: boolean
  restart_count: number
  last_failure?: {
    reason: 'authentication_failed' | 'network_unreachable' | 'configuration_invalid' | 'address_in_use' | 'tls_error' | 'unknown'
    summary: string
    exit_code: number
    occurrences: number
  } | null
  config_path: string
}

export interface InstalledFrpVersion {
  version: string
  platform: string
  active: boolean
  integrity_ok: boolean
  frpc_path: string
  has_frps: boolean
}

export interface FrpVersionList {
  active: string | null
  installed: InstalledFrpVersion[]
}

export interface AvailableFrpVersion {
  version: string
  published_at: string | null
}

export interface StatusResponse {
  state: string
  uptime_secs: number
  active_frpc_instances: number
  total_profiles: number
  total_proxies: number
  total_bindings: number
  total_visitors: number
  processes: ProcessInfo[]
}

export interface Environment {
  id?: number
  name: string
  description?: string | null
  color: string
  is_default: boolean
  created_at: string
  updated_at: string
  profile_ids: number[]
}

export interface ResourceSample {
  timestamp: string
  daemon_cpu_percent: number
  daemon_memory_bytes: number
  system_cpu_percent: number
  system_memory_used_bytes: number
  system_memory_total_bytes: number
  processes: Array<{ profile_id: number; pid: number; cpu_percent: number; memory_bytes: number }>
}

export interface TrafficSample {
  timestamp: string
  environment_id: number
  profile_id: number
  received_bytes: number
  sent_bytes: number
  active_connections: number
}

export interface MetricsHistory {
  resources: ResourceSample[]
  traffic: TrafficSample[]
}

export interface ReloadResponse {
  task_id: string
}

export interface ReloadTaskStatus {
  status: 'running' | 'completed' | 'failed'
  profiles_affected: number
  errors: string[]
  started_at: string
  completed_at: string | null
}

export interface HealthResponse {
  status: string
  version: string
}

// ── Log Response ──
export interface LogResponse {
  profile_id: number
  profile_name: string
  log_type: string
  lines: number
  content: string
  stdout_path: string | null
  stderr_path: string | null
  exists: boolean
}
