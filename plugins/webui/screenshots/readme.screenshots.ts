import { test, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { capturedAt, profiles, proxies, bindings, status, metrics, versions, logs } from "./demo-data";

const images = resolve(import.meta.dirname, "../../../docs/images");
test("capture populated documentation pages without contacting a daemon", async ({ page }) => {
  test.setTimeout(90000);
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.clock.setFixedTime(capturedAt);
  await page.addInitScript(() => {
    localStorage.setItem("api_token", "documentation-demo-token");
    localStorage.setItem("rustfrp-locale", "zh");
    localStorage.setItem("rustfrp-ui-preferences", JSON.stringify({ version: 2, mode: "light" }));
  });
  await page.route("**/api/v1/**", async route => {
    const request = route.request();
    const path = new URL(request.url()).pathname.replace("/api/v1/", "");
    const ok = (data: unknown) => route.fulfill({ json: { success: true, data } });
    if (request.method() !== "GET") {
      errors.push(`Unexpected mutation: ${request.method()} ${path}`);
      return route.abort();
    }
    const id = Number(path.split("/")[1]);
    if (path.endsWith("/frpc-version")) return ok({
      profile_id: id, version: id === 2 ? "0.69.1" : null,
      effective_version: id === 2 ? "0.69.1" : "0.70.1",
      running_version: id === 2 ? "0.69.1" : "0.70.1", running: true, restart_required: false,
    });
    if (path.endsWith("/proxy-status")) return ok({
      profile_id: id, available: true,
      proxies: bindings.filter(b => b.profile_id === id).map(b => ({
        proxy_type: proxies.find(p => p.id === b.proxy_id)?.type ?? "tcp",
        name: proxies.find(p => p.id === b.proxy_id)?.name ?? "",
        status: "running", local_addr: "127.0.0.1:8080",
        remote_addr: `example.com:${19000 + b.proxy_id!}`,
      })),
    });
    if (path.endsWith("/runtime")) return ok({ profile_id: id, running: true, desired_running: true, config_pending: false,
      enabled_proxy_count: bindings.filter(b => b.profile_id === id).length, process_status: "running" });
    if (path.startsWith("logs/")) return ok({ profile_id: id, profile_name: profiles.find(p => p.id === id)?.name,
      content: logs, lines: logs.split("\n").length, exists: true, log_type: "combined" });
    const responses: Record<string, unknown> = {
      status, profiles, proxies, bindings, visitors: [], "metrics/history": metrics,
      environments: [{ id: 1, name: "演示环境", description: "README 示例数据", profile_ids: [1,2,3], is_default: true, color: "#2563eb" }],
      "frp/versions": versions, "frp/installations": [],
      certificates: [{
        id: 1, primary_domain: "example.com", sans: ["example.com", "*.example.com"],
        not_after: "2027-01-14T00:00:00Z", days_remaining: 56, source: "certd_pull",
        source_detail: "certd.example.com", key_algo: "ECDSA P-256",
        fingerprint_sha256: "aa11bb33cc55dd77ee99ff11223344556677889900aabbccddeeff1122334455",
        bindings: [],
      }],
      "cert-ingest": { enabled: true, expose: false, port: 7901, listen: "127.0.0.1:7901", address: "127.0.0.1:7901", error: null, url: "http://127.0.0.1:7901/ingest/v1/certs" },
      "cert-ingest-tokens": [],
      "cert-pull-sources": [{ id: 1, name: "演示 Certd", kind: "certd", base_url: "https://certd.example.com", domain: "example.com", last_run_at: "2026-11-18T09:30:00Z", last_error: null }],
      "frp/releases": ["0.71.1", "0.70.1", "0.70.0", "0.69.1", "0.69.0", "0.68.1"].map((version, i) => ({
        version, tag: `v${version}`, published_at: ["2026-10-14", "2026-08-14", "2026-07-23", "2026-06-01", "2026-04-18", "2026-02-09"][i],
        asset_size: 13500000 + i * 420_000, download_count: 166_576 - i * 24_000,
        asset_name: `frp_${version}_linux_amd64.tar.gz`,
      })),
      health: { status: "ok", version: "0.4.0" },
    };
    if (path in responses) return ok(responses[path]);
    errors.push(`Unmocked API: ${path}`);
    return route.abort();
  });
  await mkdir(images, { recursive: true });
  const capture = async (filename: string) => {
    await page.evaluate(() => document.fonts.ready);
    await page.mouse.move(0, 0);
    // Wait for the real chart and virtualized table layout to settle.
    await page.waitForTimeout(800);
    await expect(page.getByRole("alert")).toHaveCount(0);
    expect(errors).toEqual([]);
    await page.screenshot({ path: resolve(images, filename), fullPage: true, animations: "disabled" });
  };
  await page.goto("/#/");
  await expect(page.getByRole("heading", { name: "仪表盘", exact: true })).toBeVisible();
  await expect(page.locator(".recharts-line-curve")).toHaveCount(4);
  await expect(page.getByText("等待指标数据", { exact: true })).toHaveCount(0);
  await expect(page.getByText("香港 · 主节点").first()).toBeVisible();
  await capture("webui-dashboard.png");

  await page.goto("/#/profiles");
  await expect(page.getByRole("cell", { name: "香港 · 主节点", exact: true })).toBeVisible();
  await capture("webui-servers.png");

  await page.goto("/#/proxies");
  await expect(page.getByRole("cell", { name: "团队文档", exact: true })).toBeVisible();
  await capture("webui-proxies.png");

  await page.goto("/#/bindings");
  await expect(page.getByText("香港 · 主节点", { exact: true })).toBeVisible();
  await capture("webui-proxy-assignments.png");

  await page.goto("/#/downloads");
  await expect(page.getByRole("heading", { name: "v0.71.1", exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "下载渠道" }).click();
  await page.getByRole("option", { name: "GHFast · ghfast.top" }).click();
  await page.keyboard.press("Escape");
  await capture("webui-versions.png");

  await page.goto("/#/certificates");
  await expect(page.getByRole("cell", { name: "example.com *.example.com" })).toBeVisible();
  await capture("webui-certificates.png");

  await page.goto("/#/logs");
  await expect(page.getByText(/login to server success/)).toBeVisible();
  await capture("webui-logs.png");
  expect(errors).toEqual([]);
});
