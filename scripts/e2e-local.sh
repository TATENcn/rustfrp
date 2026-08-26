#!/usr/bin/env bash
#
# e2e-local.sh — 本地双 frps + 双 frpc 穿透闭环测试
#
# 验证范围（不含公网可达）：
#   - rustfrp-daemon 自动下载/托管 frpc 二进制
#   - 多 profile 并发 → 多个 frpc 实例同时运行
#   - SQLite → TOML 生成 → frpc 子进程拉起
#   - frpc ↔ frps 真实协议握手 + 隧道数据搬运
#
# 前置：frps 二进制已随 rustfrp 下载到 ~/.rustfrp/frp/.../frps
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FRPS_BIN="$HOME/.rustfrp/frp/frp_0.70.1_linux_amd64/frp_0.70.1_linux_amd64/frps"
E2E=/tmp/rustfrp-e2e
DAEMON_DB="$E2E/daemon.db"
DAEMON_RT="$E2E/runtime"
API=http://127.0.0.1:7900/api/v1
FRPC_PATH="$HOME/.rustfrp/frp/frpc"
LOCAL_A=9100   # frpc A 本地监听（避开 rootlesskit 占用的 9000/9001）
LOCAL_B=9101

mkdir -p "$E2E" "$DAEMON_RT"
rm -f "$DAEMON_DB" "$DAEMON_RT"/*.toml 2>/dev/null || true

cleanup() {
  echo "--- cleanup ---"
  pkill -f "rustfrp-daemon" 2>/dev/null || true
  pkill -f "frps -c $E2E" 2>/dev/null || true
  pkill -f "frpc -c $DAEMON_RT" 2>/dev/null || true
}
trap cleanup EXIT

# 1) 启动两个原生 frps（0.70 用 TOML；token 在 auth 段）
cat > "$E2E/frps-a.toml" <<'EOF'
bindPort = 7000
auth.method = "token"
auth.token = "sectest"
EOF
cat > "$E2E/frps-b.toml" <<'EOF'
bindPort = 7001
auth.method = "token"
auth.token = "sectest2"
EOF
nohup "$FRPS_BIN" -c "$E2E/frps-a.toml" > "$E2E/frps-a.log" 2>&1 &
nohup "$FRPS_BIN" -c "$E2E/frps-b.toml" > "$E2E/frps-b.log" 2>&1 &
sleep 1.5
(ss -ltn 2>/dev/null || netstat -ltn 2>/dev/null) | grep -E ':7000|:7001' >/dev/null || {
  echo "FRPS FAIL: frps not listening"; cat "$E2E/frps-a.log"; exit 1
}
echo "[ok] frps A/B listening on 7000/7001"

# 2) 启动 daemon（新 db）
cargo run -q -p rustfrp-daemon -- --config-dir "$DAEMON_RT" --db-path "$DAEMON_DB" &
DAEMON_PID=$!
for i in $(seq 1 120); do
  curl -sf "$API/health" >/dev/null 2>&1 && break
  sleep 0.5
done
curl -sf "$API/health" >/dev/null 2>&1 || { echo "DAEMON FAIL: health check"; kill $DAEMON_PID; exit 1; }
echo "[ok] daemon up (frpc managed at $FRPC_PATH)"

# 3) 建配置
mk_profile() {
  local name="$1" addr="$2" port="$3" token="$4"
  curl -sf -X POST "$API/profiles" -H 'Content-Type: application/json' -d "{
    \"name\":\"$name\",\"server_addr\":\"$addr\",\"server_port\":$port,\"token\":\"$token\",
    \"tls_enable\":false,\"tls_cert_file\":null,\"tls_key_file\":null,\"tls_trusted_ca_file\":null,
    \"transport_protocol\":\"tcp\",\"heartbeat_interval\":30,\"heartbeat_timeout\":90,
    \"dial_server_timeout\":null,\"dial_server_keepalive\":null,\"connect_server_local_ip\":null,
    \"proxy_url\":null,\"pool_count\":null,\"tcp_mux\":null,\"tcp_mux_keepalive_interval\":null,
    \"quic_keepalive_period\":null,\"quic_max_idle_timeout\":null,\"quic_max_incoming_streams\":null,
    \"auth_method\":null,\"oidc_client_id\":null,\"oidc_client_secret\":null,\"oidc_token_endpoint_url\":null,
    \"oidc_audience\":null,\"oidc_scope\":null,\"oidc_additional_endpoint_params\":null,\"user\":null,
    \"metadatas\":null,\"login_fail_exit\":null,\"start\":null,\"dns_server\":null,
    \"nat_hole_stun_server\":null,\"udp_packet_size\":null,\"includes\":null,\"store_path\":null,\"feature_gates\":null
  }" | python3 -c "import sys,json;print(json.load(sys.stdin)['data']['id'])"
}
mk_proxy() {
  local name="$1" type="$2" lip="$3" lp="$4" rp="$5"
  curl -sf -X POST "$API/proxies" -H 'Content-Type: application/json' -d "{
    \"name\":\"$name\",\"proxy_type\":\"$type\",\"local_ip\":\"$lip\",\"local_port\":$lp,\"remote_port\":$rp,
    \"custom_domains\":null,\"subdomain\":null,\"use_encryption\":true,\"use_compression\":true,
    \"bandwidth_limit\":null,\"health_check_type\":null,\"health_check_timeout_s\":3,\"health_check_max_failed\":3,
    \"health_check_interval_s\":10,\"bandwidth_limit_mode\":null,\"secret_key\":null,\"locations\":null,
    \"http_user\":null,\"http_password\":null,\"host_header_rewrite\":null,\"request_headers\":null,
    \"response_headers\":null,\"route_by_http_user\":null,\"annotations\":null,\"metadatas\":null,
    \"allow_users\":null,\"nat_traversal_disable_assisted_addrs\":null,\"proxy_protocol_version\":null,
    \"health_check_path\":null,\"health_check_http_headers\":null,\"plugin_config\":null
  }" | python3 -c "import sys,json;print(json.load(sys.stdin)['data']['id'])"
}
mk_binding() {
  curl -sf -X POST "$API/bindings" -H 'Content-Type: application/json' -d "{
    \"profile_id\":$1,\"proxy_id\":$2,\"enabled\":true,\"running\":false,\"priority\":0,\"group_name\":null,\"group_key\":null
  }" >/dev/null
}

PA=$(mk_profile "smoke-a" 127.0.0.1 7000 sectest)
PB=$(mk_profile "smoke-b" 127.0.0.1 7001 sectest2)
QA=$(mk_proxy "rdp-a" tcp 127.0.0.1 "$LOCAL_A" 6000)
QB=$(mk_proxy "rdp-b" tcp 127.0.0.1 "$LOCAL_B" 6001)
mk_binding "$PA" "$QA"
mk_binding "$PB" "$QB"
echo "[ok] created profile A/B + proxy A/B + bindings"

# 4) start 两个 binding
curl -sf -X POST "$API/bindings/1/start" >/dev/null || { echo "[FAIL] start binding 1"; exit 1; }
curl -sf -X POST "$API/bindings/2/start" >/dev/null || { echo "[FAIL] start binding 2"; exit 1; }
sleep 2

# 5) 探针：监听 LOCAL_A/LOCAL_B，连 6000/6001 发数据，验证回显过来
probe() {
  local tunnel="$1" local_port="$2" tag="$3"
  python3 - "$tunnel" "$local_port" "$tag" <<'PY'
import socket, sys, threading, time
tunnel, local, tag = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
payload = f"rustfrp-e2e-{tag}-{time.time_ns()}".encode()
received = []
def server():
    s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", local)); s.listen(1)
    conn, _ = s.accept(); received.append(conn.recv(4096)); conn.close(); s.close()
t = threading.Thread(target=server, daemon=True); t.start(); time.sleep(0.3)
c = socket.socket(); c.connect(("127.0.0.1", tunnel)); c.sendall(payload); c.close()
t.join(timeout=5)
ok = payload in received
print(f"[{'PASS' if ok else 'FAIL'}] {tag}: tunnel {tunnel} -> local {local} ({len(payload)} bytes)")
sys.exit(0 if ok else 1)
PY
}

RC=0
probe 6000 "$LOCAL_A" A || RC=1
probe 6001 "$LOCAL_B" B || RC=1

# 6) 验证多 frpc 实例并发
N=$(pgrep -fc "frpc -c $DAEMON_RT" || true)
echo "running frpc instances: $N"
[ "$N" -ge 2 ] || { echo "[FAIL] expected >=2 concurrent frpc instances"; RC=1; }

if [ "$RC" -eq 0 ]; then echo "=== E2E PASS ==="; else echo "=== E2E FAIL ==="; fi
exit $RC
