# Official WASM plugins

These reference plugins are WebAssembly text modules that Wasmtime compiles at
load time. Copy a plugin directory into `~/.rustfrp/plugins/`; RustFRP validates
its manifest, compiles it without WASI, initializes it, and starts it in an
isolated store.

## Stable core ABI v1

A module must export `memory`. It may export these functions:

| Export | Signature | Purpose |
| --- | --- | --- |
| `rustfrp_init` | `() -> i32` | Allocate or validate plugin state |
| `rustfrp_start` | `() -> i32` | Subscribe and begin work |
| `rustfrp_stop` | `() -> i32` | Release logical resources |
| `rustfrp_alloc` | `(len) -> ptr` | Allocate an event input buffer |
| `rustfrp_on_event` | `(ptr, len) -> i32` | Receive a UTF-8 JSON event |

Lifecycle functions return zero on success. Host imports live in the `rustfrp`
module and use `(ptr, len)` UTF-8 ranges. A non-negative host result is success;
`-1` is permission denied, `-2` is invalid guest memory, and `-3` means the
output buffer is too small.

Available host imports are `log`, `get_config`, `get_traffic_stats`,
`subscribe_event`, `publish_event`, `control_process`, and `http_post`. Each call
is checked against `manifest.json`; modules never receive WASI, filesystem,
socket, or process handles. `http_post` creates a host action for an HTTPS URL,
rather than performing network I/O inside the guest.

Each invocation receives 10 million fuel units and a wall-clock epoch deadline.
Each store permits one instance, one linear memory capped at 50 MiB, and two
tables. A trap moves only that plugin to the error state; other plugins continue.

## Included plugins

- `failover`: subscribes to `connection.disconnected` and requests a controlled
  failover for the default target.
- `traffic-statistics`: reads the current traffic JSON and publishes a
  `traffic.snapshot` event.
- `webhook-notifications`: subscribes to disconnect events and emits an HTTPS
  POST action using the host-provided `webhook.url` configuration value.
