// The reader against a book of a different shape.
//
// Every other spec runs on the demo repository: four services, one compose
// file, one API page. A Maven multi-module JVM system produces pages the demo
// never has — several containers, a gateway and a config server, an API
// reference and a runtime page — and the reader had never been opened on one.
import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const paths = JSON.parse(
  readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", ".e2e-paths.json"), "utf8"),
) as Record<string, string>;

async function openJvmBook(page: Page, route = ""): Promise<string[]> {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(paths.bookJvm + route);
  await expect(page.locator(".article h1")).toBeVisible();
  return errors;
}

test("a JVM system's book opens, navigates and reports itself verified", async ({ page }) => {
  const errors = await openJvmBook(page);

  // The trust header states the commit and the evidence it verified.
  await expect(page.locator("header")).toContainText("verified");

  // Each service the system declares has its own page, reachable from the nav.
  for (const name of ["Accounts", "Gateway", "Config", "Stats"]) {
    await expect(page.locator("nav").getByText(name, { exact: true }).first()).toBeVisible();
  }

  await page.locator("nav").getByText("Accounts", { exact: true }).first().click();
  await expect(page.locator(".article h1")).toContainText("Accounts");
  expect(errors).toEqual([]);
});

test("the runtime page describes the environment the compose file declares", async ({ page }) => {
  const errors = await openJvmBook(page, "#/runtime");
  await expect(page.locator(".article h1")).toHaveText("Runtime & deployment");
  // Workloads are listed with what they run, not only drawn.
  await expect(page.locator(".article table").first()).toBeVisible();
  expect(errors).toEqual([]);
});

test("a citation in a JVM book opens its verified source", async ({ page }) => {
  const errors = await openJvmBook(page, "#/containers/accounts");
  const chip = page.locator(".cite").first();
  await chip.scrollIntoViewIfNeeded();
  await chip.click();
  const pop = page.locator(".popover");
  await expect(pop).toBeVisible();
  await expect(pop.locator(".pop-path")).toContainText(/\.(java|yml|xml)$/);
  await expect(pop.locator(".pop-state")).toContainText("Verified");
  await expect(pop.getByRole("link", { name: /Open/ })).toHaveAttribute(
    "href",
    /^https:\/\/github\.com\/acme\/shop-cloud\/blob\/[0-9a-f]{40}\//,
  );
  expect(errors).toEqual([]);
});

test("search finds a service by name", async ({ page }) => {
  const errors = await openJvmBook(page);
  await page.keyboard.press("/");
  await expect(page.locator(".search input")).toBeFocused();
  await page.keyboard.type("gateway");
  await expect(page.locator(".results .result").first()).toBeVisible();
  expect(errors).toEqual([]);
});
