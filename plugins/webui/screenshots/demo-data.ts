import { defaultProfile, defaultProxy } from "../src/lib/entities";

// Documentation-only fixtures. No API calls or writes reach a real daemon.
export const capturedAt = new Date("2026-10-25T08:30:00Z");
const timestamp = (seconds: number) => new Date(+capturedAt - seconds * 1000).toISOString();
export const profiles = [
  { ...defaultProfile(), id: 1, name: "香港 · 主节点", server_addr: "hk.example.com", server_port: 7000, tls_enable: true, auth_method: "token" },
  { ...defaultProfile(), id: 2, name: "新加坡 · 亚太节点", server_addr: "sg.example.com", server_port: 7000, tls_enable: true, auth_method: "token", transport_protocol: "wss" },
  { ...defaultProfile(), id: 3, name: "东京 · 备用节点", server_addr: "jp.example.com", server_port: 7000, tls_enable: true, auth_method: "token" },
];
export const proxies = [
  { ...defaultProxy(), id: 1, name: "团队文档", proxy_type: "http", local_ip: "192.0.2.10", local_port: 8080, custom_domains: ["docs.example.com"] },
  { ...defaultProxy(), id: 2, name: "Git 代码仓库", proxy_type: "tcp", local_ip: "192.0.2.20", local_port: 22, remote_port: 2222 },
  { ...defaultProxy(), id: 3, name: "家庭 NAS", proxy_type: "https", local_ip: "192.0.2.30", local_port: 443, custom_domains: ["nas.example.com"] },
  { ...defaultProxy(), id: 4, name: "监控面板", proxy_type: "http", local_ip: "192.0.2.40", local_port: 3000, custom_domains: ["monitor.example.com"] },
  { ...defaultProxy(), id: 5, name: "开发环境 SSH", proxy_type: "tcp", local_ip: "192.0.2.50", local_port: 22, remote_port: 2223 },
  { ...defaultProxy(), id: 6, name: "文件同步", proxy_type: "tcp", local_ip: "192.0.2.60", local_port: 22000, remote_port: 22000 },
];
export const bindings = [[1,1], [1,2], [1,3], [2,4], [2,5], [2,6], [3,2], [3,6]].map(([profile_id, proxy_id], index) => ({
  id: index + 1, profile_id, proxy_id, enabled: true, running: true, priority: 0,
  created_at: timestamp(86400), updated_at: timestamp(300),
}));
export const processes = profiles.map((profile, index) => ({
  profile_id: profile.id, profile_name: profile.name, pid: 18240 + index,
  running: true, restart_count: 0, config_path: `/data/runtime/server-${profile.id}.toml`,
}));
export const status = {
  state: "running", uptime_secs: 4 * 3600 + 26 * 60,
  active_frpc_instances: 3, total_profiles: 3, total_proxies: proxies.length,
  total_bindings: bindings.length, total_visitors: 0, processes,
};
const resources = Array.from({ length: 721 }, (_, i) => ({
  timestamp: timestamp(3600 - i * 5),
  system_cpu_percent: 12 + 4 * Math.sin(i / 33) + 18 * Math.exp(-(((i - 230) / 60) ** 2)) + 9 * Math.exp(-(((i - 550) / 35) ** 2)),
  system_memory_used_bytes: Math.round((2.2 + .15 * Math.sin(i / 80)) * 1024 ** 3),
  system_memory_total_bytes: 8 * 1024 ** 3,
  daemon_cpu_percent: 1.1 + .45 * Math.sin(i / 35),
  daemon_memory_bytes: Math.round((54 + 3 * Math.sin(i / 80)) * 1024 ** 2),
  processes: processes.map(p => ({ profile_id: p.profile_id, pid: p.pid, cpu_percent: .7, memory_bytes: 24 * 1024 ** 2 })),
}));
const trafficFor = (profile: number, scale: number, extra = {}) => {
  let received = 0, sent = 0;
  return Array.from({ length: 3601 }, (_, i) => {
    const wave = 1.2 + .35 * Math.sin(i / 180 + profile) + .65 * Math.exp(-(((i - 2100) / 320) ** 2));
    if (i) { received += Math.round(22000 * scale * wave); sent += Math.round(13500 * scale * (wave + .15 * Math.sin(i / 130))); }
    return { timestamp: timestamp(3600 - i), environment_id: 1, profile_id: profile,
      received_bytes: received, sent_bytes: sent, active_connections: 4 + profile,
      source: "linux_tcp", counter_epoch: `demo-${profile}`, ...extra };
  });
};
export const metrics = {
  resources,
  traffic: profiles.flatMap(p => trafficFor(p.id, 1 / p.id)),
  proxy_traffic: bindings.flatMap(b => trafficFor(b.profile_id!, .12 + b.proxy_id! / 30, { proxy_id: b.proxy_id, source: "linux_local_tcp" })),
  host_traffic: trafficFor(0, 2.1, { interface: "eth0", source: "linux_interface" }),
  traffic_collection: { source: "linux_tcp", state: "ready", updated_at: timestamp(0), host_state: "collecting",
    profiles: profiles.map(p => ({ profile_id: p.id, environment_id: 1, state: "collecting" })),
    proxies: bindings.map(b => ({ profile_id: b.profile_id, proxy_id: b.proxy_id, environment_id: 1, state: "collecting", name: proxies.find(p => p.id === b.proxy_id)!.name })),
  },
};
export const versions = {
  active: "0.70.1",
  installed: ["0.70.1", "0.69.1", "0.68.1"].map((version, i) => ({
    version, platform: "linux-amd64", active: i === 0, in_use: i < 2,
    integrity_ok: true, has_frps: true, frpc_path: `/data/.rustfrp/frp/versions/${version}/linux-amd64/frpc`,
    source: i === 1 ? "https://ghfast.top/https://github.com/fatedier/frp/releases/download" : "https://github.com/fatedier/frp/releases/download",
  })),
};
export const logs = [
  "2026-10-25 16:26:02 [I] [client/service.go:295] login to server success, get run id [demo-hk-01]",
  "2026-10-25 16:26:02 [I] [proxy/proxy_manager.go:173] proxy added: [team-docs git-ssh home-nas]",
  "2026-10-25 16:26:02 [I] [client/control.go:168] [team-docs] start proxy success",
  "2026-10-25 16:26:02 [I] [client/control.go:168] [git-ssh] start proxy success",
  "2026-10-25 16:26:02 [I] [client/control.go:168] [home-nas] start proxy success",
  "2026-10-25 16:26:15 [INFO] rustfrp::runtime: profile=1 state=running version=0.70.1",
  "2026-10-25 16:26:20 [INFO] rustfrp::traffic: collector=linux_tcp active_connections=5",
  "2026-10-25 16:27:00 [INFO] rustfrp::config: configuration verified profile=1 proxies=3",
  "2026-10-25 16:27:01 [INFO] rustfrp::runtime: configuration reloaded profile=1",
  "2026-10-25 16:27:30 [INFO] rustfrp::traffic: upstream=hk.example.com collection=ready",
  "2026-10-25 16:28:00 [INFO] rustfrp::metrics: host_cpu=12.8% daemon_memory=54MiB",
  "2026-10-25 16:28:30 [INFO] rustfrp::runtime: active_profiles=3 enabled_bindings=8",
  "2026-10-25 16:29:00 [INFO] rustfrp::traffic: sample_interval=1s snapshot=complete",
  "2026-10-25 16:29:30 [INFO] rustfrp::runtime: profile=1 health=ready",
  "2026-10-25 16:30:00 [INFO] rustfrp::metrics: resource and traffic samples updated",
].join("\n");
