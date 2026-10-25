import { test, expect, type Page } from "@playwright/test";
import {
  defaultProfile,
  defaultProxy,
  defaultVisitor,
} from "../src/lib/entities";
async function fixture(page: Page, authenticated = true) {
  const records = {
    profiles: [
      {
        ...defaultProfile(),
        id: 1,
        name: "Edge server",
        server_addr: "edge.example.com",
        auth_method: "token",
      },
    ],
    proxies: [
      {
        ...defaultProxy(),
        id: 1,
        name: "Web service",
        local_port: 8080,
        remote_port: 18080,
      },
    ],
    visitors: [
      {
        ...defaultVisitor(),
        id: 1,
        name: "Private visitor",
        server_name: "private",
        profile_id: 1,
      },
    ],
    bindings: [
      {
        id: 1,
        profile_id: 1,
        proxy_id: 1,
        enabled: true,
        running: false,
        priority: 0,
        created_at: "",
        updated_at: "",
      },
    ],
  };
  const writes: { method: string; path: string; body: any }[] = [];
  let running = false,
    pending = false;
  await page.addInitScript((auth) => {
    localStorage.setItem("rustfrp-locale", "en");
    localStorage.setItem(
      "rustfrp-ui-preferences",
      JSON.stringify({ version: 1, mode: "light", accent: "blue" }),
    );
    if (auth) localStorage.setItem("api_token", "valid-token");
  }, authenticated);
  await page.route("**/api/v1/**", async (route) => {
    const req = route.request(),
      url = new URL(req.url()),
      path = url.pathname.replace("/api/v1/", ""),
      method = req.method();
    if (req.headers().authorization !== "Bearer valid-token")
      return route.fulfill({
        status: 401,
        json: {
          success: false,
          error: { code: "AUTH_001", message: "Invalid token" },
        },
      });
    let body: any = null;
    if (req.postData()) body = JSON.parse(req.postData()!);
    if (method !== "GET") writes.push({ method, path, body });
    const ok = (data: unknown) =>
      route.fulfill({ json: { success: true, data } });
    const [kind, rawId, action] = path.split("/");
    const id = Number(rawId);
    if (path === "frp/installations" && method === "GET") return ok(null);
    if (kind === "profiles" && action === "frpc-version")
      return ok({
        profile_id: id,
        version: null,
        effective_version: "0.70.1",
        running_version: running ? "0.70.1" : null,
        running,
        restart_required: false,
      });
    if (path === "status")
      return ok({
        state: "running",
        uptime_secs: 125,
        active_frpc_instances: Number(running),
        total_profiles: records.profiles.length,
        total_proxies: records.proxies.length,
        total_bindings: 1,
        total_visitors: 1,
        processes: [
          {
            profile_id: 1,
            profile_name: "Edge server",
            pid: running ? 124 : null,
            running,
            restart_count: 0,
            config_path: "/tmp/frpc.toml",
          },
        ],
      });
    if (path === "environments")
      return ok([
        {
          id: 1,
          name: "Default",
          profile_ids: records.profiles.map((p) => p.id),
          is_default: true,
          color: "#2563eb",
        },
      ]);
    if (path === "metrics/history")
      return ok({
        resources: [
          {
            timestamp: new Date().toISOString(),
            system_cpu_percent: 12,
            system_memory_used_bytes: 536870912,
            system_memory_total_bytes: 1073741824,
            daemon_cpu_percent: 1,
            daemon_memory_bytes: 1000,
            processes: [],
          },
        ],
        traffic: [],
      });
    if (kind === "profiles" && action === "runtime")
      return ok({
        profile_id: id,
        running,
        desired_running: running,
        config_pending: pending,
        enabled_proxy_count: records.bindings.length,
        process_status: running ? "running" : "stopped",
      });
    if (kind === "profiles" && ["start", "stop"].includes(action!)) {
      running = action === "start";
      pending = false;
      return ok({
        profile_id: id,
        running,
        desired_running: running,
        config_pending: false,
        enabled_proxy_count: records.bindings.length,
        process_status: running ? "started" : "stopped",
      });
    }
    if (kind === "profiles" && action === "proxies") {
      records.bindings = body.proxy_ids.map((proxy_id: number) => ({
        id: proxy_id,
        profile_id: id,
        proxy_id,
        enabled: true,
        running: false,
        priority: 0,
        created_at: "",
        updated_at: "",
      }));
      pending = running;
      return ok(records.bindings);
    }
    if (kind && kind in records) {
      const rows = records[kind as keyof typeof records] as any[];
      if (method === "GET")
        return ok(rawId ? rows.find((row) => row.id === id) : rows);
      if (method === "POST") {
        const row = {
          ...body,
          id: Math.max(0, ...rows.map((row) => row.id)) + 1,
        };
        rows.push(row);
        return ok(row);
      }
      if (method === "PUT") {
        const index = rows.findIndex((row) => row.id === id);
        rows[index] = { ...body, id };
        return ok(rows[index]);
      }
      if (method === "DELETE") {
        const index = rows.findIndex((row) => row.id === id);
        if (index < 0)
          return route.fulfill({ status: 404, json: {
            success: false,
            error: { message: "profile was not found in the active tenant" },
          } });
        rows.splice(index, 1);
        return route.fulfill({ json: { success: true } });
      }
    }
    if (kind === "logs")
      return ok({
        profile_id: 1,
        profile_name: "Edge server",
        log_type: url.searchParams.get("log_type"),
        lines: 2,
        exists: true,
        content:
          "\u001b[31mERROR test\u001b[0m\n<script>window.pwned=1</script>\nINFO ready",
        stdout_path: "/tmp/stdout",
        stderr_path: "/tmp/stderr",
      });
    if (path === "frp/releases")
      return ok([{ version: "0.70.1", published_at: null }]);
    if (path === "frp/versions" && method === "GET")
      return ok({
        active: "0.70.1",
        installed: [
          {
            version: "0.70.1",
            active: true,
            integrity_ok: true,
            platform: "linux-amd64",
          },
          {
            version: "0.69.0",
            active: false,
            integrity_ok: true,
            platform: "linux-amd64",
          },
        ],
      });
    if (path === "reload") return ok({ task_id: "task-1" });
    if (path === "reload/task-1")
      return ok({ status: "completed", profiles_affected: 1, errors: [] });
    if (path === "config/import")
      return ok({
        profile_name: body.profile_name,
        proxies_imported: 1,
        visitors_imported: 0,
      });
    if (path === "config/export")
      return route.fulfill({
        body: "SQLite backup",
        headers: {
          "content-disposition": 'attachment; filename="backup.sqlite"',
        },
      });
    return ok(null);
  });
  return { records, writes };
}
test("login verifies the protected API and rejects an invalid token", async ({
  page,
}) => {
  await fixture(page, false);
  await page.goto("/#/login");
  await page.getByLabel("API Token").fill("invalid");
  await page.getByRole("button", { name: "Login", exact: true }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await expect(page).toHaveURL(/login/);
  await page.getByLabel("API Token").fill("valid-token");
  await page.getByRole("button", { name: "Login", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
});
test("profile form saves typed fields and edit keeps undisplayed fields", async ({
  page,
}) => {
  const { writes, records } = await fixture(page);
  records.profiles[0]!.dial_server_timeout = 45;
  await page.goto("/#/profiles");
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Server Address").fill("new.example.com");
  await dialog.getByLabel("Server Port").fill("7443");
  await page.screenshot({
    path: "test-results/redesign-editor.png",
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.screenshot({
    path: "test-results/redesign-profiles.png",
    fullPage: true,
  });
  expect(writes.find((w) => w.method === "PUT")?.body).toMatchObject({
    server_addr: "new.example.com",
    server_port: 7443,
    dial_server_timeout: 45,
  });
});
test("proxy HTTP form serializes lists and deletion requires confirmation", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/proxies/new");
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel(/^Name/).fill("Public web");
  await dialog.getByRole("combobox", { name: "Type", exact: true }).click();
  await page.getByRole("option", { name: "HTTP", exact: true }).click();
  await dialog.getByLabel("Local Port").fill("8081");
  await dialog
    .getByLabel("custom domains", { exact: true })
    .fill("one.example.com, two.example.com");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page).toHaveURL(/#\/proxies$/);
  expect(
    writes.find((w) => w.path === "proxies" && w.method === "POST")?.body,
  ).toMatchObject({
    proxy_type: "http",
    custom_domains: ["one.example.com", "two.example.com"],
    local_port: 8081,
  });
  await page
    .getByRole("row")
    .filter({ hasText: "Public web" })
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click();
  expect(writes.filter((w) => w.method === "DELETE")).toHaveLength(0);
  await page
    .getByRole("row")
    .filter({ hasText: "Public web" })
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await expect(
    page.getByRole("row").filter({ hasText: "Public web" }),
  ).toHaveCount(0);
});
test("visitor edits preserve profile selection and checkbox boolean values", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/visitors");
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await page.getByRole("dialog").getByLabel("Server Name").fill("new-server");
  await page
    .getByRole("dialog")
    .getByRole("switch", { name: "Encryption", exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(writes.find((w) => w.method === "PUT")?.body).toMatchObject({
    server_name: "new-server",
    profile_id: 1,
    use_encryption: false,
  });
});
test("assignment and start/stop actions use the profile runtime APIs", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/bindings");
  await page
    .getByRole("button", { name: "Start configuration", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Stop configuration", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Configure.*Edge server/ }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("checkbox").uncheck();
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(writes.find((w) => w.path === "profiles/1/proxies")?.body).toEqual({
    proxy_ids: [],
  });
  await page
    .getByRole("button", { name: "Stop configuration", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Start configuration", exact: true }),
  ).toBeDisabled();
});
test("logs support stream filters, ANSI colors and escaped HTML", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/logs");
  await expect(page.locator(".log-console")).toContainText("ERROR test");
  await expect(page.locator(".ansi-red-fg")).toBeVisible();
  await expect(page.locator(".log-console")).toContainText(
    "<script>window.pwned=1</script>",
  );
  expect(await page.evaluate(() => "pwned" in window)).toBe(false);
  await page.getByRole("tab", { name: "stderr", exact: false }).click();
  await page.getByRole("textbox", { name: "Search" }).fill("ready");
  await expect(page.locator(".log-console")).not.toContainText("ERROR test");
  await expect(page.locator(".log-console")).toContainText("INFO ready");
  expect(writes).toHaveLength(0);
});
test("system actions preserve import/export, mirror validation and version confirmation", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/status");
  await page.getByRole("tab", { name: "FRP versions", exact: true }).click();
  await page
    .getByRole("row")
    .filter({ hasText: "0.69.0" })
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Delete", exact: true })
    .click();
  await expect
    .poll(() =>
      writes.some(
        (w) => w.path === "frp/versions/0.69.0" && w.method === "DELETE",
      ),
    )
    .toBe(true);
  await page.getByRole("tab", { name: "Import & export", exact: true }).click();
  await page.locator("input[type=file]").setInputFiles({
    name: "sample.toml",
    mimeType: "text/plain",
    buffer: Buffer.from('serverAddr = "example.com"'),
  });
  await page
    .getByRole("button", { name: "Import frpc.toml", exact: true })
    .click();
  await expect
    .poll(() => writes.some((w) => w.path === "config/import"))
    .toBe(true);
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Download backup", exact: true })
    .click();
  expect((await download).suggestedFilename()).toBe("backup.sqlite");
});
test("theme, language, sidebar and responsive navigation remain available", async ({
  page,
}) => {
  await fixture(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  await page.getByRole("menuitemradio", { name: "Dark", exact: true }).click();
  await expect(page.locator("html")).toHaveClass(/dark/);
  await page
    .getByRole("button", { name: "Change language", exact: true })
    .click();
  await expect(page.getByRole("heading", { name: "仪表盘" })).toBeVisible();
  await page.screenshot({
    path: "test-results/dashboard-dark.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("button", { name: "打开导航" }).click();
  await page
    .getByRole("dialog")
    .getByRole("link", { name: "代理规则", exact: true })
    .click();
  await expect(page.getByRole("dialog")).not.toBeVisible();
  await expect(page.locator("body")).toHaveJSProperty("scrollWidth", 390);
  await page.screenshot({
    path: "test-results/mobile-proxies.png",
    fullPage: true,
  });
});
test("API failures show a retry state without hiding previous data", async ({
  page,
}) => {
  await fixture(page);
  await page.goto("/");
  await expect(
    page
      .getByRole("complementary", { name: "Processes", exact: true })
      .getByText("Edge server", { exact: true }),
  ).toBeVisible();
  await page.route("**/api/v1/status", (route) =>
    route.fulfill({
      status: 500,
      json: { success: false, error: { message: "Database unavailable" } },
    }),
  );
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Database unavailable", {
    timeout: 10000,
  });
  await expect(
    page
      .getByRole("complementary", { name: "Processes", exact: true })
      .getByText("Edge server", { exact: true }),
  ).toBeVisible();
});
test("batch ports create typed proxy records and reject name conflicts", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/proxies");
  await page.getByRole("button", { name: "Batch Ports" }).click();
  await page
    .getByRole("dialog")
    .getByLabel("Port list or ranges")
    .fill("9000-9002");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save", exact: true })
    .click();
  await expect(page.getByRole("dialog")).not.toBeVisible();
  const creates = writes.filter(
    (w) => w.path === "proxies" && w.method === "POST",
  );
  expect(creates.map((w) => w.body.local_port)).toEqual([9000, 9001, 9002]);
  expect(creates[0]!.body).toMatchObject({
    name: "tcp-9000",
    remote_port: 9000,
    use_encryption: true,
  });
  await page.getByRole("button", { name: "Batch Ports" }).click();
  await page.getByRole("dialog").getByLabel("Port list or ranges").fill("9000");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Save", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.locator("[data-sonner-toast]").last()).toContainText(
    "9000",
  );
  expect(
    writes.filter((w) => w.path === "proxies" && w.method === "POST"),
  ).toHaveLength(3);
});

test("resource filters and pagination work together", async ({ page }) => {
  const { records } = await fixture(page);
  for (let id = 2; id <= 23; id++)
    records.proxies.push({
      ...defaultProxy(),
      id,
      name: `Service ${id}`,
      local_port: 8000 + id,
      remote_port: 18000 + id,
      proxy_type: id % 2 ? "udp" : "tcp",
    });
  await page.goto("/#/proxies");
  await expect(page.getByRole("row")).toHaveCount(11);
  await page.getByRole("button", { name: "Next page" }).click();
  await expect(
    page.getByRole("cell", { name: "Service 11", exact: true }),
  ).toBeVisible();
  await page.getByRole("textbox", { name: "Search" }).fill("Service 23");
  await expect(page.getByRole("row")).toHaveCount(2);
  await expect(
    page.getByRole("cell", { name: "Service 23", exact: true }),
  ).toBeVisible();
  await page.getByRole("combobox", { name: "Filter protocol" }).click();
  await page.getByRole("option", { name: "TCP", exact: true }).click();
  await expect(page.getByText("No records found")).toBeVisible();
  await page.getByRole("button", { name: "Reset", exact: true }).click();
  await expect(page.getByRole("row")).toHaveCount(11);
});

test("Recharts renders real samples, tooltips and memory mode", async ({
  page,
}) => {
  await fixture(page);
  const end = Date.now();
  await page.route("**/api/v1/metrics/history**", (route) =>
    route.fulfill({
      json: {
        success: true,
        data: {
          resources: Array.from({ length: 60 }, (_, i) => ({
            timestamp: new Date(end - (59 - i) * 60000).toISOString(),
            system_cpu_percent: 20 + Math.sin(i / 4) * 8 + i / 4,
            system_memory_used_bytes: 536870912 + i * 1048576,
            system_memory_total_bytes: 1073741824,
            daemon_cpu_percent: 2 + Math.cos(i / 5),
            daemon_memory_bytes: 16777216 + i * 10000,
            processes: [],
          })),
          traffic: Array.from({ length: 60 }, (_, i) => ({
            timestamp: new Date(end - (59 - i) * 3000).toISOString(),
            environment_id: 1,
            profile_id: 1,
            source: "linux_tcp",
            counter_epoch: "test",
            received_bytes: i * 3072,
            sent_bytes: i * 6144,
            active_connections: 3,
          })),
        },
      },
    }),
  );
  await page.goto("/");
  const trafficChart = page.locator('[aria-label="Network traffic chart"]');
  await expect(trafficChart.locator(".recharts-line-curve")).toHaveCount(2);
  await trafficChart
    .locator(".recharts-wrapper")
    .hover({ position: { x: 180, y: 100 } });
  await expect(trafficChart.locator(".recharts-tooltip-wrapper")).toContainText(
    "KiB/s",
  );
  const chart = page.locator('[aria-label="Resource usage chart"]');
  await expect(chart.locator(".recharts-line-curve")).toHaveCount(2);
  await chart
    .locator(".recharts-wrapper")
    .hover({ position: { x: 180, y: 100 } });
  await expect(chart.locator(".recharts-tooltip-wrapper")).toBeVisible();
  await expect(chart.locator(".recharts-tooltip-wrapper")).toContainText("%");
  await page.getByRole("tab", { name: "Memory", exact: true }).click();
  await chart
    .locator(".recharts-wrapper")
    .hover({ position: { x: 220, y: 110 } });
  await expect(chart.locator(".recharts-tooltip-wrapper")).toContainText("MiB");
  await page.getByRole("combobox", { name: "Time range", exact: true }).click();
  await page
    .getByRole("option", { name: "Last 15 minutes", exact: true })
    .click();
  await expect(chart.locator(".recharts-line-curve")).toHaveCount(2);
  await page.getByRole("tab", { name: "CPU", exact: true }).click();
  await page.screenshot({
    path: "test-results/redesign-overview.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.locator("body")).toHaveJSProperty("scrollWidth", 390);
  await page.screenshot({
    path: "test-results/redesign-overview-mobile.png",
    fullPage: true,
  });
});

test("TLS options expand below their switch and preserve unsaved values", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  await page.goto("/#/profiles/new");
  const dialog = page.getByRole("dialog");
  const toggle = dialog.getByRole("switch", {
    name: "TLS Enable",
    exact: true,
  });
  const certificate = dialog.getByLabel("tls cert file", { exact: true });
  await toggle.uncheck();
  await expect(certificate).toHaveCount(0);
  await toggle.check();
  await expect(certificate).toBeVisible();
  const toggleBox = await toggle.boundingBox();
  const certificateBox = await certificate.boundingBox();
  expect(certificateBox!.y).toBeGreaterThan(toggleBox!.y + toggleBox!.height);
  await certificate.fill("/etc/frp/client.crt");
  await toggle.uncheck();
  await expect(certificate).toHaveCount(0);
  await toggle.check();
  await expect(certificate).toHaveValue("/etc/frp/client.crt");
  await page.screenshot({
    path: "test-results/tls-expanded.png",
    fullPage: true,
    animations: "disabled",
  });
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(writes).toHaveLength(0);
});

test("Unsupported traffic collection explains the cause", async ({ page }) => {
  await fixture(page);
  await page.route("**/api/v1/metrics/history**", (route) =>
    route.fulfill({
      json: {
        success: true,
        data: {
          resources: [],
          traffic: [],
          traffic_collection: {
            source: "linux_tcp",
            state: "ready",
            updated_at: new Date().toISOString(),
            profiles: [
              {
                profile_id: 1,
                environment_id: 1,
                state: "unsupported_transport",
              },
            ],
          },
        },
      },
    }),
  );
  await page.goto("/");
  await expect(
    page.getByText("Transport or upstream proxy is not supported", {
      exact: false,
    }),
  ).toBeVisible();
  await expect(
    page.getByText("No traffic samples available", { exact: true }),
  ).toBeVisible();
});

test("traffic scopes separate servers, local proxies and host interfaces", async ({
  page,
}) => {
  await fixture(page);
  const end = Date.now();
  const samples = (profile: number, amount: number, extra = {}) =>
    [0, 1].map((i) => ({
      timestamp: new Date(end - 1000 + i * 1000).toISOString(),
      environment_id: 1,
      profile_id: profile,
      received_bytes: i * amount,
      sent_bytes: i * amount * 2,
      active_connections: 1,
      source: "linux_tcp",
      counter_epoch: "one",
      ...extra,
    }));
  await page.route("**/api/v1/metrics/history**", (route) =>
    route.fulfill({
      json: {
        success: true,
        data: {
          resources: [],
          traffic: [...samples(1, 1024), ...samples(2, 2048)],
          proxy_traffic: [
            ...samples(1, 4096, { proxy_id: 10, source: "linux_local_tcp" }),
            ...samples(2, 8192, { proxy_id: 10, source: "linux_local_tcp" }),
          ],
          host_traffic: [
            ...samples(0, 16384, {
              interface: "eth0",
              source: "linux_interface",
            }),
            ...samples(0, 32768, {
              interface: "eth1",
              source: "linux_interface",
            }),
          ],
          traffic_collection: {
            state: "ready",
            source: "linux_tcp",
            updated_at: new Date().toISOString(),
            host_state: "collecting",
            profiles: [1, 2].map((profile_id) => ({
              profile_id,
              environment_id: 1,
              state: "collecting",
            })),
            proxies: [1, 2].map((profile_id) => ({
              profile_id,
              environment_id: 1,
              proxy_id: 10,
              name: "Local echo",
              state: "collecting",
            })),
          },
        },
      },
    }),
  );
  await page.goto("/");
  const inbound = page
    .getByText("Window inbound", { exact: true })
    .locator("..");
  await expect(inbound).toContainText("3 KiB");
  await page
    .getByRole("combobox", { name: "Traffic server", exact: true })
    .click();
  await page.getByRole("option", { name: "#2", exact: true }).click();
  await expect(inbound).toContainText("2 KiB");
  await page
    .getByRole("combobox", { name: "Traffic scope", exact: true })
    .click();
  await page
    .getByRole("option", { name: "Local proxies", exact: true })
    .click();
  await expect(inbound).toContainText("8 KiB");
  await expect(
    page.getByRole("cell", { name: "Local echo", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("combobox", { name: "Traffic scope", exact: true })
    .click();
  await page
    .getByRole("option", { name: "Host interfaces", exact: true })
    .click();
  await expect(inbound).toContainText("48 KiB");
  await page
    .getByRole("combobox", { name: "Traffic detail", exact: true })
    .click();
  await page.getByRole("option", { name: "eth0", exact: true }).click();
  await expect(inbound).toContainText("16 KiB");
  await page.screenshot({
    path: "test-results/traffic-host.png",
    fullPage: true,
  });
});

test("server version selection saves without restarting and exposes pending apply", async ({
  page,
}) => {
  const { writes } = await fixture(page);
  let version: string | null = null,
    applied = "0.70.1";
  await page.route("**/api/v1/profiles/1/frpc-version", async (route) => {
    if (route.request().method() === "PUT")
      version = route.request().postDataJSON().version;
    await route.fulfill({
      json: {
        success: true,
        data: {
          profile_id: 1,
          version,
          effective_version: version ?? "0.70.1",
          running_version: applied,
          running: true,
          restart_required: version !== null && version !== applied,
        },
      },
    });
  });
  await page.goto("/#/profiles");
  await page.getByRole("button", { name: "frpc 0.70.1", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog
    .getByRole("combobox", { name: "frpc version", exact: true })
    .click();
  await page.getByRole("option", { name: "0.69.0", exact: true }).click();
  await dialog
    .getByRole("button", { name: "Save version selection", exact: true })
    .click();
  await expect(
    dialog.getByRole("button", {
      name: "Restart and apply version",
      exact: true,
    }),
  ).toBeVisible();
  expect(
    writes.some((w) => w.path.endsWith("/start") || w.path.endsWith("/stop")),
  ).toBe(false);
  await expect(dialog).toContainText(
    "briefly interrupts all its proxy connections",
  );
  await page.screenshot({
    path: "test-results/profile-frpc-version.png",
    animations: "disabled",
  });
});

test("installer submits accelerator channel and shows background task progress", async ({
  page,
}) => {
  await fixture(page);
  let submitted: any = null;
  let task: any = null;
  await page.route("**/api/v1/frp/installations", async (route) => {
    if (route.request().method() === "POST") {
      submitted = route.request().postDataJSON();
      task = {
        id: "install-test",
        version: submitted.version,
        source:
          "https://ghfast.top/https://github.com/fatedier/frp/releases/download",
        status: "running",
        stage: "downloading",
        downloaded_bytes: 524288,
        total_bytes: 1048576,
        error: null,
      };
    }
    await route.fulfill({ json: { success: true, data: task } });
  });
  await page.goto("/#/status?tab=versions");
  await page.getByLabel("Version", { exact: true }).fill("0.70.1");
  await page
    .getByRole("combobox", { name: "Download channel", exact: true })
    .click();
  await page
    .getByRole("option", { name: "GHFast · ghfast.top", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Install version", exact: true })
    .click();
  await expect.poll(() => submitted?.channel).toBe("ghfast");
  await expect(
    page.getByRole("status").filter({ hasText: "Downloading archive" }),
  ).toContainText("50%");
  await expect(
    page.getByRole("button", { name: "Install version", exact: true }),
  ).toBeDisabled();
  await page.screenshot({
    path: "test-results/frpc-installation.png",
    animations: "disabled",
  });
  task = { ...task, status: "failed", error: "Checksum mismatch" };
  await expect(
    page.getByText("Checksum mismatch", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Install version", exact: true }),
  ).toBeEnabled();
});


test("profile deletion targets the selected ID after sorting and filtering and closes the dialog", async ({ page }) => {
  const { records, writes } = await fixture(page);
  records.profiles.push({ ...records.profiles[0]!, id: 42, name: "Another server" });
  await page.goto("/#/profiles");
  await page.getByRole("button", { name: "Name", exact: true }).click();
  const dialog = page.getByRole("alertdialog");
  await page.getByRole("row").filter({ hasText: "Another server" })
    .getByRole("button", { name: "Delete", exact: true }).click();
  await expect(dialog).toContainText("Another server");
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("row").filter({ hasText: "Another server" })).toHaveCount(0);
  await expect(page.getByRole("row").filter({ hasText: "Edge server" })).toHaveCount(1);
  expect(records.profiles.map(row => row.id)).toEqual([1]);
  await page.getByRole("textbox").fill("Edge");
  await page.getByRole("row").filter({ hasText: "Edge server" })
    .getByRole("button", { name: "Delete", exact: true }).click();
  await expect(dialog).toContainText("Edge server");
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("No records found", { exact: true })).toBeVisible();
  expect(writes.filter(w => w.method === "DELETE").map(w => w.path))
    .toEqual(["profiles/42", "profiles/1"]);
});

test("failed profile deletion keeps the confirmation and record for retry", async ({ page }) => {
  const { records } = await fixture(page);
  await page.route("**/api/v1/profiles/1", async route => {
    if (route.request().method() !== "DELETE") return route.fallback();
    return route.fulfill({ json: { success: false, error: { message: "Deletion refused" } } });
  });
  await page.goto("/#/profiles");
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  const dialog = page.getByRole("alertdialog");
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.getByText("Deletion refused", { exact: true })).toBeVisible();
  await expect(dialog).toBeVisible();
  expect(records.profiles.map(row => row.id)).toEqual([1]);
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toHaveCount(0);
});

test("version suggestions stay compact and support search, keyboard and manual entry", async ({ page }) => {
  await fixture(page);
  await page.route("**/api/v1/frp/releases", route => route.fulfill({ json: {
    success: true, data: Array.from({ length: 80 }, (_, i) => ({ version: `0.${100 - i}.0` })),
  } }));
  await page.goto("/#/status?tab=versions");
  const input = page.getByRole("combobox", { name: "Version", exact: true });
  await input.click();
  const list = page.getByRole("listbox", { name: "Available versions" });
  await expect(list.getByRole("option")).toHaveCount(80);
  const box = await list.boundingBox();
  expect(box!.height).toBeLessThanOrEqual(240);
  const field = await input.boundingBox();
  expect(box!.y).toBeGreaterThanOrEqual(field!.y + field!.height);
  expect(Math.abs(box!.width - field!.width)).toBeLessThan(4);
  await input.fill("0.99");
  await expect(list.getByRole("option")).toHaveCount(1);
  await input.press("ArrowDown");
  await input.press("Enter");
  await expect(input).toHaveValue("0.99.0");
  await expect(list).toHaveCount(0);
  await input.fill("0.999.1");
  await expect(page.getByText("No matches. Your entered version will be used.")).toBeVisible();
  await input.press("Escape");
  await expect(list).toHaveCount(0);
  await expect(input).toHaveValue("0.999.1");
});
