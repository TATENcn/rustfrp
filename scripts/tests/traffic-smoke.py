"""Isolated FRP traffic integration test. Requires daemon, frpc and frps binaries."""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import threading
import time
from datetime import datetime
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("--daemon", required=True)
parser.add_argument("--frpc", required=True)
parser.add_argument("--frps", required=True)
args = parser.parse_args()

def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]

api_port, server_port, remote_port = port(), port(), port()
echo = socket.socket()
echo.bind(("127.0.0.1", 0))
echo.listen()
echo_port = echo.getsockname()[1]
def echo_loop():
    try:
        while True:
            connection, _ = echo.accept()
            def handle(conn):
                with conn:
                    while data := conn.recv(65536):
                        conn.sendall(data)
            threading.Thread(target=handle, args=(connection,), daemon=True).start()
    except OSError:
        pass
threading.Thread(target=echo_loop, daemon=True).start()

processes = []
with tempfile.TemporaryDirectory(prefix="rustfrp-traffic-") as directory:
    root = Path(directory)
    token = "isolated-test-token"
    (root / "token").write_text(token)
    (root / "server.toml").write_text(f'bindAddr = "127.0.0.1"\nbindPort = {server_port}\nauth.token = "isolated-frp-token"\nproxyBindAddr = "127.0.0.1"\n')
    def api(path, method="GET", body=None):
        request = urllib.request.Request(f"http://127.0.0.1:{api_port}/api/v1/{path}", method=method,
            data=json.dumps(body).encode() if body is not None else None,
            headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"})
        with urllib.request.urlopen(request, timeout=3) as response:
            result = json.load(response)
        assert result["success"], result
        return result["data"]

    def until(check, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                value = check()
                if value:
                    return value
            except (OSError, urllib.error.URLError):
                pass
            time.sleep(.2)
        raise AssertionError("Timed out waiting for integration state")
    try:
        with (root / "server.log").open("w") as log:
            processes.append(subprocess.Popen([str(Path(args.frps).resolve()), "-c", str(root/"server.toml")], stdout=log, stderr=log, start_new_session=True))
        with (root / "daemon.log").open("w") as log:
            processes.append(subprocess.Popen([
                str(Path(args.daemon).resolve()), "--db-path", str(root/"config.db"),
                "--config-dir", str(root/"runtime"), "--api-listen", f"127.0.0.1:{api_port}",
                "--frpc-path", str(Path(args.frpc).resolve()), "--no-auto-download"],
                env={**os.environ, "RUSTFRP_API_TOKEN_FILE":str(root/"token")}, stdout=log, stderr=log, start_new_session=True))
        until(lambda: api("status"))
        profile = api("profiles", "POST", {"name":"traffic-test","server_addr":"127.0.0.1","server_port":server_port,
            "auth_method":"token","token":"isolated-frp-token","transport_protocol":"tcp","tls_enable":False,
            "heartbeat_interval":10,"heartbeat_timeout":60})
        proxy = api("proxies", "POST", {"name":"echo","proxy_type":"tcp","local_ip":"127.0.0.1","local_port":echo_port,
            "remote_port":remote_port,"use_encryption":False,"use_compression":False})
        profile_id = profile["id"]
        api(f"profiles/{profile_id}/proxies","PUT",{"proxy_ids":[proxy["id"]]})
        api(f"profiles/{profile_id}/start","POST")
        baseline = until(lambda: api("metrics/history")["traffic"])
        epoch = baseline[-1]["counter_epoch"]
        connection = until(lambda: socket.create_connection(("127.0.0.1",remote_port),timeout=2))
        with connection:
            connection.settimeout(5)
            payload = os.urandom(262144)
            for _ in range(4):
                connection.sendall(payload)
                returned = bytearray()
                while len(returned)<len(payload):
                    chunk=connection.recv(min(65536,len(payload)-len(returned)))
                    assert chunk, "unexpected EOF"
                    returned.extend(chunk)
                assert returned == payload
            samples = until(lambda: [s for s in api("metrics/history")["traffic"] if s["received_bytes"]>=len(payload)*4 and s["sent_bytes"]>=len(payload)*4])
            local = until(lambda: [s for s in api("metrics/history")["proxy_traffic"] if s["proxy_id"] == proxy["id"] and s["received_bytes"] >= len(payload)*4 and s["sent_bytes"] >= len(payload)*4])
            assert local[-1]["profile_id"] == profile_id
        history = api("metrics/history")
        assert history["traffic_collection"]["host_state"] == "collecting"
        assert all(s["interface"] != "lo" and s["source"] == "linux_interface" for s in history["host_traffic"])
        measured = until(lambda: api("metrics/history")["traffic"] if len(api("metrics/history")["traffic"]) >= 4 else None)
        times = [datetime.fromisoformat(s["timestamp"]) for s in measured[-4:]]
        interval = (times[-1] - times[0]).total_seconds() / 3
        assert .7 < interval < 1.5, interval
        status = api("metrics/history")["traffic_collection"]
        assert status["profiles"][0]["state"]=="collecting", status
        assert samples[-1]["source"]=="linux_tcp"
        api(f"profiles/{profile_id}/stop","POST")
        api(f"profiles/{profile_id}/start","POST")
        until(lambda: any(s["counter_epoch"]!=epoch for s in api("metrics/history")["traffic"]))
        duplicate = api("proxies", "POST", {"name":"echo-shared","proxy_type":"tcp","local_ip":"127.0.0.1","local_port":echo_port,
            "remote_port":port(),"use_encryption":False,"use_compression":False})
        api(f"profiles/{profile_id}/proxies","PUT",{"proxy_ids":[proxy["id"], duplicate["id"]]})
        until(lambda: len([p for p in api("metrics/history")["traffic_collection"]["proxies"] if p["state"] == "ambiguous_endpoint"]) == 2)
        print(f"PASS: 1 MiB real FRP upstream + per-proxy counters; host collection; {interval:.2f}s cadence; restart epoch and shared-endpoint ambiguity verified")
    except Exception:
        # Runtime logs belong only to this synthetic test setup.
        print((root/"daemon.log").read_text()[-3000:])
        print((root/"server.log").read_text()[-1500:])
        raise
    finally:
        echo.close()
        for process in reversed(processes):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
