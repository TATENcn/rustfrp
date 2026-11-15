# RustFRP WebUI

The daemon-embedded WebUI uses **React 19, TypeScript, Vite 8 and the original shadcn/ui React components**. Vue, Vue Router, Pinia, Naive UI and the unused ECharts dependency have been removed.

## Development

From the repository root, `bun run mode:reload` builds the UI and daemon and updates the managed HTTPS preview. See [the development manager guide](../../docs/development-mode.md).

For frontend HMR:

```bash
cd plugins/webui
bun install --frozen-lockfile
bun run dev
```

The Vite development server proxies `/api` to `http://127.0.0.1:7900`; run the existing `just dev-daemon` workflow alongside it. The managed mode preview uses its own isolated backend port and built assets instead of HMR.

## UI structure

The UI was rebuilt around the official shadcn/ui neutral theme. The previous application layout, pages, form renderer, shared UI wrappers, and hand-drawn charts have been removed.

- `src/app/`: hash routing, lazy page loading, and official shadcn Sidebar with mobile Sheet navigation.
- `src/features/`: overview, resource management, assignments, logs, system settings, and login.
- `src/components/ui/`: original shadcn registry components (`new-york`, Radix). Sidebar, Field, Chart, Alert, Dialog, Table, Tabs, and other primitives share the standard semantic tokens.
- `src/components/common/`: page states, paginated tables, confirmation dialogs, and process tables composed from those primitives.
- `src/features/overview/metrics-chart.tsx`: shadcn Chart + Recharts 3, with time axes, legends, hover tooltips, keyboard accessibility, CPU/memory switching, and time-window selection. Charts are loaded only with the overview route.
- `src/lib/fields.ts`: field definitions for the existing FRP API. Editors group connection settings and security options; undisplayed server fields are retained on save.
- `src/hooks/`: TanStack Query requests and persistent light/dark/system mode. The redesign uses one neutral theme; existing mode and environment selections survive the migration.
- `src/api/`, `src/lib/entities.ts`, and `src/i18n/`: API contracts, domain helpers, and English/Chinese messages.
- `src/components/logs/format.ts`: ANSI conversion with HTML escaping, consumed by the virtualized log viewer.

Hash URLs are preserved, including `/profiles/new`, `/profiles/:id`, `/proxies/new`, and `/proxies/:id`. Authentication validates the protected `/status` endpoint. No remote fonts, icon services, or production demo data are used.

System management separates runtime, FRP versions, and import/export into tabs. Resource lists offer protocol filters, sorting, and pagination. Desktop and mobile use the same accessible shadcn controls.

## Validation

```bash
bun run type-check
bun run check:icons
bun scripts/check-i18n-keys.ts
bun test
bun run build
bun x playwright install chromium
bun run test:e2e
```

The browser tests start a local Vite server and mock **all** `/api/v1` requests. They exercise protected login, CRUD payloads, confirmation dialogs, assignment/start/stop, ANSI/HTML log handling, import/export, themes, language and responsive navigation without changing a real daemon's configuration. Browser screenshots and failure traces are written to ignored `test-results/`.

Dependency installation uses `bun.lock`; production output is written to `dist/` for Rust's embedded WebUI. The API client never logs authentication tokens.

## README screenshots

From the repository root:

```bash
bun install --cwd plugins/webui --frozen-lockfile
(cd plugins/webui && bun x playwright install chromium)
bun run --cwd plugins/webui screenshots
```

The screenshot suite starts its own Vite server and intercepts every `/api/v1` request with fixtures in `screenshots/demo-data.ts`. It rejects mutations and unhandled requests, so no live daemon or database is needed. Three servers, six proxy rules, eight assignments, one hour of resource/traffic samples, installed versions and synthetic logs populate six screenshots in `docs/images/`.

Use Chromium with Noto Sans CJK SC installed for consistent Chinese text. The suite fixes the clock, viewport, locale and light theme, waits for charts and fonts, and checks that populated content is visible before capturing. Fixtures are used only by Playwright and are not shipped in the production UI. These screenshots illustrate the interface; they are not benchmarks or real deployment records.
