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

Endpoints default to daemon `7900`, FRPS `7000`, control `3000`, Prometheus
`9090`, Alertmanager `9093`, and Grafana `3001`. The FRPS HTTP/HTTPS vhost ports
default to `8080` and `8443`.

The provisioned `RustFRP Overview` dashboard is read-only and refreshes every
15 seconds. `RustFRPNodeOffline` fires after two minutes without a successful
node scrape; `RustFRPControlUnavailable` fires after one minute. Alertmanager's
default receiver retains and displays alerts without sending them externally.
Add email, Slack, webhook, or another receiver to `alertmanager.yml` for outbound
delivery; no third-party destination is enabled implicitly.

For additional FRPS nodes, append entries to `targets.json`. Their metrics URLs
must be reachable from the control container.
