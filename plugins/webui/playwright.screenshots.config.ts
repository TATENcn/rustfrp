import { defineConfig } from "@playwright/test";
import base from "./playwright.config";

export default defineConfig({
  ...base,
  testDir: "./screenshots",
  testMatch: "**/*.screenshots.ts",
  workers: 1,
  use: {
    ...base.use,
    viewport: { width: 1600, height: 1000 },
    deviceScaleFactor: 1,
    locale: "zh-CN",
    timezoneId: "Asia/Shanghai",
    colorScheme: "light",
    reducedMotion: "reduce",
  },
});
