# Full observability Compose deployment

The root `compose.yml` starts RustFRP daemon, the official multi-architecture
FRPS image, RustFRP control, Prometheus, Alertmanager, and Grafana. Prometheus
scrapes the control service; control injects `node_id` and `node_name` into every
upstream FRPS sample before aggregation.

Create the required secrets before starting:

```sh
mkdir -p deploy/compose/secrets
openssl rand -hex 32 > deploy/compose/secrets/rustfrp_api_token.txt
export GRAFANA_ADMIN_PASSWORD='replace-with-a-long-password'
docker compose up -d --build
```

Endpoints default to daemon `7900`, certificate ingest `7901`, FRPS `7000`,
control `3000`, Prometheus `9090`, Alertmanager `9093`, and Grafana `3001`. The
FRPS HTTP/HTTPS vhost ports default to `8080` and `8443`.

## Certificate ingest endpoint

Certificates for `https` proxies are pushed to the daemon, because frps does not
terminate TLS — it only reads the SNI and routes. The push route
(`/ingest/v1/certs`, plus `/ingest/v1/healthz`) is served by **both** the control
API on `7900` and a dedicated listener on `7901`. It authenticates with the
`X-Cert-Ingest-Token` header and returns no certificate material.

Prefer the control API route: it answers at whatever address the UI is reached
on — a frpc `https` proxy, a reverse proxy, a LAN address — so nothing extra
needs publishing. The dedicated `7901` listener exists for deployments that map
that port directly; it is loopback-only by default and enabled by default *inside
the container*, so the port mapping is what makes it reachable.

The URL shown in System Status → Certificate channels is resolved per request:
the `Host` (or `X-Forwarded-Host`/`X-Forwarded-Proto`) the browser used wins, then
the bind address. Set `RUSTFRP_CERT_INGEST_PUBLIC_URL` to pin it when neither
describes how Certd reaches the daemon (a path-rewriting proxy, a remapped port).

In a container the `7901` listener has to bind all interfaces or the published
port reaches nothing, so `RUSTFRP_CERT_INGEST_LISTEN=0.0.0.0:7901` is set for you.
Whether the listener runs at all still comes from the stored setting exposed in
the UI (System Status → Certificate channels), which defaults to enabled. The host side of that mapping defaults to `127.0.0.1`, so nothing on the network
can reach it. A Certd running in this same Compose stack does not need the host
port at all — point its webhook at `http://daemon:7900/ingest/v1/certs`. For an
external Certd that must use the dedicated port, set `RUSTFRP_CERT_INGEST_BIND=0.0.0.0`
(or a specific interface)
and publish `RUSTFRP_CERT_INGEST_PORT`; the container port stays `7901` so the
mapping and the listener cannot disagree.

Removing the `7901` mapping keeps the endpoint unreachable from outside the
container while leaving it usable inside the Compose network.

The provisioned `RustFRP Overview` dashboard is read-only and refreshes every
15 seconds. `RustFRPNodeOffline` fires after two minutes without a successful
node scrape; `RustFRPControlUnavailable` fires after one minute. Alertmanager's
default receiver retains and displays alerts without sending them externally.
Add email, Slack, webhook, or another receiver to `alertmanager.yml` for outbound
delivery; no third-party destination is enabled implicitly.

For additional FRPS nodes, append entries to `targets.json`. Their metrics URLs
must be reachable from the control container.

For multiple automation identities, tenant claims, and least-privilege scopes,
copy `auth-policy.example.json` outside the repository, replace the example
digest (`printf %s YOUR_TOKEN | sha256sum`), mount it read-only, and set
`RUSTFRP_AUTH_POLICY_FILE` to its container path. Policy files store only SHA256
digests; bearer tokens shorter than 32 characters are rejected, so generate
high-entropy credentials. Tenant-safe scopes are `read`, `write`, and
`telemetry:write`; global
operations additionally require `platform:read` or `platform:write`. `*` grants
all scopes and should be reserved for platform administration.
Call `/api/v1/auth/whoami` to verify the active identity and tenant. Supplying an
`X-RustFRP-Tenant` header that differs from the token claim is rejected.

On Linux, daemon resource samples and Prometheus output also report kernel BTF,
bpffs, pinned-object, and unprivileged-BPF status. Loading flow probes remains an
explicit privileged deployment choice; the daemon itself needs no additional
capabilities and degrades cleanly on other platforms.
