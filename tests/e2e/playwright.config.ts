import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./specs",
  globalSetup: "./global-setup.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [["list"]],
  outputDir: "test-results",
  use: {
    // Local runs can reuse an installed Chrome (PW_CHANNEL=chrome); Docker uses the bundled Chromium.
    channel: process.env.PW_CHANNEL || undefined,
    viewport: { width: 1440, height: 900 },
    acceptDownloads: true,
    trace: "retain-on-failure",
  },
});
