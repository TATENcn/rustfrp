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
    if (path.endsWith("/runtime")) return ok({ profile_id: id, running: true, desired_running: true, config_pending: false,
      enabled_proxy_count: bindings.filter(b => b.profile_id === id).length, process_status: "running" });
    if (path.startsWith("logs/")) return ok({ profile_id: id, profile_name: profiles.find(p => p.id === id)?.name,
      content: logs, lines: logs.split("\n").length, exists: true, log_type: "combined" });
    const responses: Record<string, unknown> = {
      status, profiles, proxies, bindings, visitors: [], "metrics/history": metrics,
      environments: [{ id: 1, name: "演示环境", description: "README 示例数据", profile_ids: [1,2,3], is_default: true, color: "#2563eb" }],
      "frp/versions": versions, "frp/installations": null,
      "frp/releases": ["0.70.1", "0.70.0", "0.69.1", "0.69.0", "0.68.1"].map(version => ({ version, published_at: null })),
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
  await expect(page.getByText("0.69.1", { exact: false }).first()).toBeVisible();
  await capture("webui-proxy-assignments.png");

  await page.goto("/#/status?tab=versions");
  await expect(page.getByRole("cell", { name: "0.69.1", exact: false })).toBeVisible();
  await page.getByRole("combobox", { name: "下载渠道" }).click();
  await page.getByRole("option", { name: "GHFast · ghfast.top" }).click();
  await page.getByRole("heading", { name: "系统状态", exact: true }).click();
  await capture("webui-versions.png");

  await page.goto("/#/logs");
  await expect(page.getByText(/login to server success/)).toBeVisible();
  await capture("webui-logs.png");
  expect(errors).toEqual([]);
});
