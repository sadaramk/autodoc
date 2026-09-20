import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const paths = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", ".e2e-paths.json"), "utf8")) as Record<string, string>;

async function open(page: Page, which = "containers") {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.addInitScript(() => {
    (window as any).__copied = null;
    const capture = (t: string) => ((window as any).__copied = t);
    if (navigator.clipboard) navigator.clipboard.writeText = async (t: string) => void capture(t);
    const exec = document.execCommand.bind(document);
    document.execCommand = (cmd: string, ...rest: any[]) => {
      if (cmd === "copy") {
        capture((document.activeElement as HTMLTextAreaElement).value);
        return true;
      }
      return exec(cmd, ...rest);
    };
  });
  await page.goto(paths[which]);
  await expect(page.locator("svg.nunki")).toBeVisible();
  return errors;
}

const node = (page: Page, id: string) => page.locator(`.ad-nodes .ad-node[data-id="${id}"]`);
const transform = (page: Page) => page.locator(".ad-viewport").getAttribute("transform");
const scaleOf = (t: string | null) => Number(/scale\(([\d.]+)\)/.exec(t ?? "")?.[1]);

test("renders the container diagram fitted to the viewport without errors", async ({ page }) => {
  const errors = await open(page);
  await expect(page).toHaveTitle("Polyglot Shop — containers");
  await expect(page.locator(".ad-nodes .ad-node")).toHaveCount(10);
  await expect(page.locator(".ad-edges .ad-edge")).toHaveCount(11);
  await expect(page.locator(".ad-node.is-focal")).toHaveCount(1);
  await expect(node(page, "api-gateway")).toHaveClass(/is-focal/);
  const box = await page.locator(".ad-viewport").boundingBox();
  const viewport = page.viewportSize()!;
  expect(box!.width).toBeLessThanOrEqual(viewport.width);
  expect(box!.height).toBeLessThanOrEqual(viewport.height);
  expect(errors).toEqual([]);
  await page.screenshot({ path: "test-results/containers.png" });
});

test("hovering a node traces its upstream and downstream paths", async ({ page }) => {
  await open(page);
  await node(page, "payments").hover();
  await expect(page.locator("svg.nunki")).toHaveClass(/is-tracing/);
  for (const id of ["web", "api-gateway", "payments", "postgres", "stripe"]) {
    await expect(node(page, id)).toHaveClass(/is-lit/);
  }
  for (const id of ["redis", "kafka", "fulfillment", "ledger-audit", "sendgrid"]) {
    await expect(node(page, id)).not.toHaveClass(/is-lit/);
  }
  await expect(page.locator('.ad-edges .ad-edge[data-id="payments--stripe"]')).toHaveClass(/is-lit/);
  await expect(page.locator('.ad-edges .ad-edge[data-id="api-gateway--redis"]')).not.toHaveClass(/is-lit/);
  await page.mouse.move(5, 5);
  await expect(page.locator("svg.nunki")).not.toHaveClass(/is-tracing/);
});

test("clicking a node opens the evidence drawer with verified source and a git reference", async ({ page }) => {
  await open(page);
  await node(page, "payments").click();
  const drawer = page.locator(".drawer");
  await expect(drawer).toHaveClass(/is-open/);
  await expect(drawer.locator("h2")).toHaveText("Payments");
  await expect(drawer.locator(".file")).toHaveText("payments/cmd/payments/main.go");
  await expect(drawer.locator(".lines")).toContainText("Lines 17–30 · main");
  await expect(drawer.locator(".state")).toHaveClass(/verified/);
  await expect(drawer.locator("pre.snippet .ln").first()).toContainText("func main()");
  await expect(drawer.locator("pre.snippet .ln")).toHaveCount(14);
  await expect(drawer.getByRole("link", { name: /Open permalink/ })).toHaveAttribute("href", /^https:\/\/github\.com\/acme\/polyglot-shop\/blob\/[0-9a-f]{40}\/payments\/cmd\/payments\/main\.go#L17-L30$/);

  await drawer.getByRole("button", { name: "Copy Git Reference" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__copied)).toMatch(/github\.com\/acme\/polyglot-shop\/blob\/[0-9a-f]{40}\/payments\/cmd\/payments\/main\.go#L17-L30/);
  await expect(page.locator(".toast")).toHaveText("Copied git reference");
  await page.screenshot({ path: "test-results/drawer.png" });

  // Navigate along a relationship from the drawer.
  await drawer.locator("section", { hasText: "Downstream" }).getByRole("button", { name: /Stripe/ }).click();
  await expect(drawer.locator("h2")).toHaveText("Stripe");
  await expect(node(page, "stripe")).toHaveClass(/is-selected/);

  await page.keyboard.press("Escape");
  await expect(drawer).not.toHaveClass(/is-open/);
  await expect(page.locator(".ad-node.is-selected")).toHaveCount(0);
});

test("nodes are keyboard operable", async ({ page }) => {
  await open(page);
  await node(page, "kafka").focus();
  await expect(page.locator("svg.nunki")).toHaveClass(/is-tracing/);
  await page.keyboard.press("Enter");
  await expect(page.locator(".drawer h2")).toHaveText("Kafka");
  await expect(page.locator(".drawer .body")).toContainText("order.placed");
});

test("wheel zooms, drag pans, and fit restores the view", async ({ page }) => {
  await open(page);
  const initial = await transform(page);
  await page.mouse.move(700, 450);
  await page.mouse.wheel(0, -400);
  await expect.poll(async () => scaleOf(await transform(page))).toBeGreaterThan(scaleOf(initial));

  const before = await transform(page);
  await page.mouse.move(40, 860);
  await page.mouse.down();
  await page.mouse.move(240, 760, { steps: 5 });
  await page.mouse.up();
  expect(await transform(page)).not.toEqual(before);

  await page.getByRole("button", { name: /Fit to screen/ }).click();
  await expect.poll(() => transform(page)).toEqual(initial);
  await page.keyboard.press("+");
  await expect.poll(async () => scaleOf(await transform(page))).toBeGreaterThan(scaleOf(initial));
});

test("theme toggles between editorial light and dark without redrawing", async ({ page }) => {
  await open(page);
  const svg = page.locator("svg.nunki");
  const cardsBefore = await page.locator(".ad-node").evaluateAll((els) => els.map((e) => e.outerHTML.length));
  const canvasFill = () => page.locator(".ad-canvas").evaluate((el) => getComputedStyle(el).fill);
  await expect(svg).toHaveAttribute("data-theme", "editorial-light");
  expect(await canvasFill()).toBe("rgb(248, 249, 250)");
  await page.getByRole("button", { name: /Toggle light\/dark/ }).click();
  await expect(svg).toHaveAttribute("data-theme", "editorial-dark");
  expect(await canvasFill()).toBe("rgb(11, 15, 23)");
  expect(await page.locator(".ad-node").evaluateAll((els) => els.map((e) => e.outerHTML.length))).toEqual(cardsBefore);
  await page.screenshot({ path: "test-results/containers-dark.png" });
});

test("exports a standalone SVG and a high-resolution PNG", async ({ page }) => {
  await open(page);
  await node(page, "web").hover();
  const [svgDownload] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Download SVG" }).click()]);
  expect(svgDownload.suggestedFilename()).toBe("polyglot-shop-containers.svg");
  const svg = readFileSync((await svgDownload.path())!, "utf8");
  expect(svg).toContain("<svg");
  expect(svg).toMatch(/viewBox="0 0 [\d.]+ [\d.]+"/);
  expect(svg).not.toContain("tabindex");
  expect(svg).not.toContain("is-tracing");
  expect(svg).not.toContain("transform=\"translate");

  const [pngDownload] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Download PNG" }).click()]);
  const png = readFileSync((await pngDownload.path())!);
  expect(png.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  const width = png.readUInt32BE(16);
  const layoutWidth = await page.evaluate(() => (window as any).nunki.data.layout.width);
  expect(width).toBeGreaterThanOrEqual(Math.round(layoutWidth * 2));
});

test("component and system views render and respond", async ({ page }) => {
  for (const which of ["components", "system"]) {
    const errors = await open(page, which);
    const nodes = page.locator(".ad-nodes .ad-node");
    expect(await nodes.count()).toBeGreaterThan(2);
    await nodes.first().click();
    await expect(page.locator(".drawer")).toHaveClass(/is-open/);
    expect(errors).toEqual([]);
    await page.screenshot({ path: `test-results/${which}.png` });
  }
});

test("dark theme with coral accent is honoured from the IR", async ({ page }) => {
  await open(page, "dark");
  await expect(page.locator("svg.nunki")).toHaveAttribute("data-theme", "editorial-dark");
  const stroke = await page.locator(".ad-node.is-focal .ad-card").evaluate((el) => getComputedStyle(el).stroke);
  expect(stroke).toBe("rgb(251, 113, 133)");
});

test("small screens keep the canvas and drawer usable", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  await node(page, "web").click();
  const drawer = await page.locator(".drawer").boundingBox();
  expect(Math.round(drawer!.width)).toBe(390);
  await expect(page.getByRole("button", { name: "Close inspector" })).toBeVisible();
});

test("sequence diagram: numbered messages on lifelines open their verified evidence", async ({ page }) => {
  const errors = await open(page, "sequence");
  await expect(page.locator("svg.nunki")).toHaveAttribute("data-diagram-type", "ad-sequence");
  await expect(page.locator(".ad-lifeline")).toHaveCount(4);
  const labels = page.locator(".ad-edge-label text:not(.ad-edge-detail)");
  await expect(labels).toHaveText([
    "1. POST /api/checkout",
    "2. POST /charges",
    "3. 201 Charge",
    "4. publish",
    "5. mark order paid",
    "6. 201 Order",
  ]);
  await expect(page.locator('.ad-edges .ad-edge[data-id="m3"]')).toHaveClass(/is-reply/);

  // Messages stack top to bottom in sequence order.
  const ys = await page.locator(".ad-edges .ad-edge").evaluateAll((els) =>
    els
      .map((el) => ({ id: el.getAttribute("data-id")!, y: (el.querySelector(".ad-edge-line") as SVGPathElement).getBBox().y }))
      .sort((a, b) => a.id.localeCompare(b.id))
      .map((m) => m.y),
  );
  expect([...ys].sort((a, b) => a - b)).toEqual(ys);

  await page.locator('.ad-edge-label.has-evidence[data-edge="m1"]').click();
  const drawer = page.locator(".drawer");
  await expect(drawer).toHaveClass(/is-open/);
  await expect(drawer.locator(".eyebrow")).toHaveText("Message 1 · sync");
  await expect(drawer.locator("#drawer-title")).toHaveText("POST /api/checkout");
  await expect(drawer.locator(".subtitle")).toHaveText("Web → API Gateway");
  await expect(drawer.locator(".file")).toHaveText("web/src/api/client.ts");
  await expect(drawer.locator(".state")).toHaveClass(/verified/);
  await expect(drawer.locator(".snippet")).toContainText("submitCheckout");
  await expect(drawer.locator("dd")).toHaveText(["CheckoutRequest"]);
  await drawer.getByRole("button", { name: "Copy Git Reference" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__copied)).toContain("client.ts#L9-L19");

  await drawer.getByRole("button", { name: /API Gateway/ }).click();
  await expect(drawer.locator("#drawer-title")).toContainText("API Gateway");
  await page.keyboard.press("Escape");
  await expect(drawer).not.toHaveClass(/is-open/);
  expect(errors).toEqual([]);
});

test("entity and state diagrams render keys, cardinality and state markers", async ({ page }) => {
  const errors = await open(page, "entities");
  await expect(page.locator(".ad-node.is-entity")).toHaveCount(2);
  await expect(page.locator('.ad-node[data-id="order_items"] .ad-row-key')).toHaveText(["PK FK", "PK"]);
  await expect(page.locator('.ad-edges .ad-edge[data-id="r1"]')).toHaveClass(/c-s-one c-t-many/);
  const markers = await page
    .locator('.ad-edges .ad-edge[data-id="r1"] .ad-edge-line')
    .evaluate((el) => [getComputedStyle(el).markerStart, getComputedStyle(el).markerEnd]);
  expect(markers[0]).toContain("ad-one");
  expect(markers[1]).toContain("ad-many");
  // Tracing must not swap crow's feet for arrowheads.
  await node(page, "orders").hover();
  const traced = await page.locator('.ad-edges .ad-edge[data-id="r1"] .ad-edge-line').evaluate((el) => getComputedStyle(el).markerEnd);
  expect(traced).toContain("ad-many");

  errors.push(...(await open(page, "lifecycle")));
  await expect(page.locator(".ad-node.state-initial .ad-state-mark")).toHaveCount(1);
  await expect(page.locator(".ad-node.state-terminal .ad-state-inner")).toHaveCount(1);
  await expect(page.locator('.ad-edge-label[data-edge="t2"] text')).toHaveText("ship_order [status = 'paid']");
  expect(errors).toEqual([]);
});
