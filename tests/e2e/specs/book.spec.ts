import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const paths = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", ".e2e-paths.json"), "utf8")) as Record<string, string>;

async function openBook(page: Page, route = "", colorScheme: "light" | "dark" = "light") {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.emulateMedia({ colorScheme });
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
  await page.goto(paths.bookIndex + route);
  await expect(page.locator(".article h1")).toBeVisible();
  return errors;
}

const figure = (page: Page, id: string) => page.locator(`figure.figure[data-diagram="${id}"]`).first();

test("book overview renders nav, table of contents, stats and figure without errors", async ({ page }) => {
  const errors = await openBook(page);
  await expect(page).toHaveTitle(/Polyglot Shop/);
  await expect(page.locator(".article h1")).toHaveText("Polyglot Shop");
  const group = (title: string) => page.locator(".sidenav .nav-group", { has: page.locator(".nav-title", { hasText: title }) }).locator(".nav-link");
  await expect(group("Start")).toHaveCount(2);
  await expect(group("Containers")).toHaveCount(5);
  await expect(group("System")).toHaveText(["Data & integrations", "Critical flows", "Runtime & deployment"]);
  await expect(group("API reference")).not.toHaveCount(0);
  await expect(group("Product")).toHaveText(["Functional specification", "Business requirements"]);
  await expect(group("Trust")).toHaveText(["Evidence & unknowns"]);
  await expect(page.locator('.sidenav .nav-link[aria-current="page"]')).toHaveText("Overview");
  await expect(page.locator(".rail .toc a")).toHaveCount(3);
  await expect(page.locator(".stats .stat")).toHaveCount(4);
  await expect(figure(page, "system-context").locator("svg.nunki")).toBeVisible();
  await expect(page.locator(".chip.health")).toContainText("verified");
  await expect(page.locator(".cards .card")).not.toHaveCount(0);
  expect(errors).toEqual([]);
  await page.screenshot({ path: "test-results/book-overview.png", fullPage: true });
});

test("navigation via sidebar and cards updates hash, title and active item", async ({ page }) => {
  await openBook(page);
  await page.locator(".sidenav .nav-link", { hasText: "Architecture" }).click();
  await expect(page).toHaveURL(/#\/architecture$/);
  await expect(page.locator(".article h1")).toHaveText("Architecture");
  await expect(page).toHaveTitle(/^Architecture — Polyglot Shop/);
  await expect(page.locator('.sidenav .nav-link[aria-current="page"]')).toHaveText("Architecture");
  await page.locator(".pager a.prev").click();
  await expect(page.locator(".article h1")).toHaveText("Polyglot Shop");
  await page.locator(".cards .card", { hasText: "Critical flows" }).click();
  await expect(page).toHaveURL(/#\/flows$/);
  await expect(page.locator(".article h1")).toHaveText("Critical flows");
  // Heading anchors scroll within the page.
  await page.goto(paths.bookIndex + "#/architecture#relationships");
  await expect(page.locator("#relationships")).toBeInViewport();
});

test("architecture figure drills down and traces dependencies", async ({ page }) => {
  await openBook(page, "#/architecture");
  const fig = figure(page, "containers");
  await fig.locator('.ad-node[data-id="payments"]').hover();
  await expect(fig.locator("svg.nunki")).toHaveClass(/is-tracing/);
  await expect(fig.locator('.ad-node[data-id="stripe"]')).toHaveClass(/is-lit/);
  await expect(fig.locator('.ad-node[data-id="web"]')).toHaveClass(/is-lit/);
  await expect(fig.locator('.ad-node[data-id="redis"]')).not.toHaveClass(/is-lit/);
  await fig.locator('.ad-node[data-id="payments"]').click();
  await expect(page).toHaveURL(/#\/containers\/payments$/);
  await expect(page.locator(".article h1")).toHaveText("Payments");
  await expect(figure(page, "components-payments")).toBeVisible();
  await page.screenshot({ path: "test-results/book-container.png", fullPage: true });
});

test("infrastructure nodes open their evidence instead of a page", async ({ page }) => {
  await openBook(page, "#/architecture");
  await figure(page, "containers").locator('.ad-node[data-id="postgres"]').click();
  await expect(page.locator(".popover")).toBeVisible();
  await expect(page.locator(".popover .pop-path")).not.toBeEmpty();
  await expect(page).toHaveURL(/#\/architecture$/);
});

test("citation chips open a popover with verified source and a copyable permalink", async ({ page }) => {
  await openBook(page, "#/containers/payments");
  const chip = page.locator("table.data .cite", { hasText: "main.go:17–30" }).first();
  await chip.click();
  const pop = page.locator(".popover");
  await expect(pop).toBeVisible();
  await expect(pop.locator(".pop-path")).toHaveText("payments/cmd/payments/main.go");
  await expect(pop.locator(".pop-state")).toContainText("Verified");
  await expect(pop.locator(".snippet .ln").first()).toContainText("func main()");
  await expect(pop.locator(".snippet .no").first()).toHaveText("17");
  await expect(pop.getByRole("link", { name: /Open/ })).toHaveAttribute("href", /^https:\/\/github\.com\/acme\/polyglot-shop\/blob\/[0-9a-f]{40}\/payments\/cmd\/payments\/main\.go#L17-L30$/);
  await pop.getByRole("button", { name: "Copy reference" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__copied)).toMatch(/github\.com\/acme\/polyglot-shop\/blob\/[0-9a-f]{40}\/payments\/cmd\/payments\/main\.go#L17-L30/);
  const box = await pop.boundingBox();
  const vp = page.viewportSize()!;
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(vp.width);
  await page.keyboard.press("Escape");
  await expect(pop).toBeHidden();
});

test("ctrl+wheel zooms a figure while plain wheel scrolls the page; fullscreen toggles", async ({ page }) => {
  await openBook(page, "#/architecture");
  const fig = figure(page, "containers");
  const svg = fig.locator("svg.nunki");
  const vb0 = await svg.getAttribute("viewBox");
  const box = (await fig.locator(".fig-canvas").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  const scroll0 = await page.evaluate(() => window.scrollY);
  await page.mouse.wheel(0, 300);
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(scroll0);
  expect(await svg.getAttribute("viewBox")).toEqual(vb0);

  const box2 = (await fig.locator(".fig-canvas").boundingBox())!;
  await page.mouse.move(box2.x + box2.width / 2, box2.y + Math.min(box2.height / 2, 200));
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -200);
  await page.keyboard.up("Control");
  await expect.poll(async () => Number((await svg.getAttribute("viewBox"))!.split(" ")[2])).toBeLessThan(Number(vb0!.split(" ")[2]));

  await fig.getByRole("button", { name: "Fit diagram" }).click();
  const layoutWidth = await page.evaluate(() => (window as any).nunkiBook.book.diagrams.containers.width);
  await expect.poll(async () => Number((await svg.getAttribute("viewBox"))!.split(" ")[2])).toBeGreaterThanOrEqual(layoutWidth - 1);
  await fig.getByRole("button", { name: "Full screen" }).click();
  await expect(fig).toHaveClass(/is-fullscreen/);
  await page.keyboard.press("Escape");
  await expect(fig).not.toHaveClass(/is-fullscreen/);
});

test("critical flow walkthrough frames each hop with both cards whole", async ({ page }) => {
  await openBook(page, "#/flows");
  // The page now has one walkthrough per drawn flow as well as the primary
  // path; this is the primary path, the only one over the container diagram.
  const walk = page.locator('.walk:has(.figure[data-diagram="containers"])');
  const svg = walk.locator("svg.nunki");
  const steps = walk.locator(".step");
  const count = await steps.count();
  expect(count).toBeGreaterThanOrEqual(2);
  let previous: string | null = null;
  for (let i = 0; i < count; i++) {
    await walk.getByRole("button", { name: "Next step" }).click();
    await expect(steps.nth(i)).toHaveClass(/is-active/);
    const edgeId = (await steps.nth(i).getAttribute("data-edge"))!;
    const edge = svg.locator(`.ad-edges .ad-edge[data-id="${edgeId}"]`);
    await expect(edge).toHaveClass(/is-step/);
    if (previous) await expect(svg.locator(`.ad-edges .ad-edge[data-id="${previous}"]`)).not.toHaveClass(/is-step/);
    const source = (await edge.getAttribute("data-source"))!;
    const target = (await edge.getAttribute("data-target"))!;
    await page.waitForTimeout(400); // eased framing transition
    const canvas = (await walk.locator(".fig-canvas svg").boundingBox())!;
    for (const node of [source, target]) {
      const card = (await svg.locator(`.ad-node[data-id="${node}"] .ad-card`).boundingBox())!;
      expect(card.x, `${node} left in step ${i + 1}`).toBeGreaterThanOrEqual(canvas.x - 0.5);
      expect(card.y, `${node} top in step ${i + 1}`).toBeGreaterThanOrEqual(canvas.y - 0.5);
      expect(card.x + card.width, `${node} right in step ${i + 1}`).toBeLessThanOrEqual(canvas.x + canvas.width + 0.5);
      expect(card.y + card.height, `${node} bottom in step ${i + 1}`).toBeLessThanOrEqual(canvas.y + canvas.height + 0.5);
    }
    if (i === 0) await page.screenshot({ path: "test-results/book-flows-step1.png" });
    previous = edgeId;
  }
  await page.screenshot({ path: "test-results/book-flows-step3.png" });
  await walk.getByRole("button", { name: /Play/ }).click();
  await expect(walk.locator(".step.is-active")).toHaveCount(1);
  await page.screenshot({ path: "test-results/book-flows.png", fullPage: true });
});

// A request flow is a story with an order, so the book lets a reader be walked
// through it rather than made to match numbers in a table against arrows in a
// picture (#56). What makes it true is the correspondence: the message lit on
// the diagram is always the step the reader is on, whether they stepped there or
// Play did. Reduced motion means the reader steps; nothing moves on its own.
test("a request flow plays message by message with the lit message matching the step", async ({ page }) => {
  const errors = await openBook(page, "#/flows");
  const walk = page.locator('.walk:has(.figure[data-diagram^="flow-"])').first();
  const svg = walk.locator("svg.nunki");
  const steps = walk.locator(".step");
  const count = await steps.count();
  expect(count, "a traced request flow has several messages").toBeGreaterThanOrEqual(3);

  const lit = async () => {
    const ids = await svg.locator(".ad-edges .ad-edge.is-step").evaluateAll((els) =>
      els.map((e) => e.getAttribute("data-id"))
    );
    return ids;
  };

  // Keyboard: focus a step and press Enter, then walk on with Next.
  await steps.first().focus();
  await page.keyboard.press("Enter");
  for (let i = 0; i < count; i++) {
    if (i > 0) await walk.getByRole("button", { name: "Next step" }).click();
    await expect(steps.nth(i)).toHaveClass(/is-active/);
    await expect(steps.nth(i)).toHaveAttribute("aria-current", "step");
    const edgeId = (await steps.nth(i).getAttribute("data-edge"))!;
    await expect(svg.locator(`.ad-edges .ad-edge[data-id="${edgeId}"]`)).toHaveClass(/is-step/);
    // Exactly one message is lit, and it is this step's: a walkthrough that lit
    // two messages, or a stale one, would look like the flow branches.
    expect(await lit(), `step ${i + 1} of ${count}`).toEqual([edgeId]);
    // And you can see it. A step that highlights a message scrolled out of the
    // panel tells the reader nothing, which is the failure a walkthrough over a
    // tall sequence invites.
    const canvas = (await walk.locator(".fig-canvas svg").boundingBox())!;
    const line = (await svg.locator(`.ad-edge[data-id="${edgeId}"] .ad-edge-line`).boundingBox())!;
    expect(line.y, `step ${i + 1} is above the panel`).toBeGreaterThanOrEqual(canvas.y - 0.5);
    expect(line.y + line.height, `step ${i + 1} is below the panel`).toBeLessThanOrEqual(canvas.y + canvas.height + 0.5);
  }
  // A sequence names its participants once, along the top, so a walkthrough
  // holds still rather than scrolling them away — for as long as the diagram
  // fits the panel at a legible scale. Past that the reader can pan and fit.
  const figure = walk.locator(".figure");
  if (!(await figure.evaluate((el) => el.classList.contains("is-cropped")))) {
    const canvas = (await walk.locator(".fig-canvas svg").boundingBox())!;
    for (const head of await svg.locator(".ad-node .ad-card").all()) {
      const box = (await head.boundingBox())!;
      expect(box.y, "a lifeline head scrolled off the panel").toBeGreaterThanOrEqual(canvas.y - 0.5);
    }
  }
  await page.screenshot({ path: "test-results/book-flow-walk.png", fullPage: true });

  // Play advances on its own, and the lit message keeps up with the step.
  const index = () => steps.evaluateAll((els) => els.findIndex((e) => e.classList.contains("is-active")));
  await walk.getByRole("button", { name: /Play/ }).click();
  await expect(walk.getByRole("button", { name: /Pause/ })).toBeVisible();
  await expect.poll(index, { timeout: 8000 }).toBeGreaterThan(0);
  // Paused before comparing: the next tick is 1.6s away and would otherwise be
  // free to move the step between reading it and reading the diagram.
  await walk.getByRole("button", { name: /Pause/ }).click();
  await expect(walk.getByRole("button", { name: /Play/ })).toBeVisible();
  const active = await index();
  expect(await lit(), `Play stopped on step ${active + 1}`).toEqual([
    await steps.nth(active).getAttribute("data-edge")
  ]);
  expect(errors, errors.join("\n")).toEqual([]);
});

test("reduced motion walks a flow but never advances it on its own", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openBook(page, "#/flows");
  const walk = page.locator('.walk:has(.figure[data-diagram^="flow-"])').first();
  const steps = walk.locator(".step");
  const index = () => steps.evaluateAll((els) => els.findIndex((e) => e.classList.contains("is-active")));
  await walk.getByRole("button", { name: /Play/ }).click();
  expect(await index(), "Play still shows the first step").toBe(0);
  // Nothing moves by itself: the button stays Play, and after longer than the
  // auto-advance interval the same step is current.
  await expect(walk.getByRole("button", { name: /Play/ })).toBeVisible();
  await page.waitForTimeout(2000);
  expect(await index(), "reduced motion must not auto-advance").toBe(0);
  await walk.getByRole("button", { name: "Next step" }).click();
  expect(await index()).toBe(1);
});

test("search jumps to a page from the keyboard", async ({ page }) => {
  await openBook(page);
  await page.keyboard.press("/");
  await expect(page.locator(".search input")).toBeFocused();
  await page.keyboard.type("Payments");
  await expect(page.locator(".results .result").first()).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/#\/containers\/payments/);
  await expect(page.locator(".article h1")).toHaveText("Payments");
});

test("theme toggle switches canvas color and persists across reloads", async ({ page }) => {
  await openBook(page, "#/architecture");
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(await bg()).toBe("rgb(248, 249, 250)");
  await page.getByRole("button", { name: "Toggle light or dark theme" }).click();
  expect(await bg()).toBe("rgb(11, 15, 23)");
  await expect(figure(page, "containers").locator("svg.nunki")).toHaveAttribute("data-theme", "editorial-dark");
  await page.reload();
  await expect(page.locator(".article h1")).toBeVisible();
  expect(await bg()).toBe("rgb(11, 15, 23)");
  await page.screenshot({ path: "test-results/book-architecture-dark.png", fullPage: true });
});

test("wide figures stay legible and open on the focal node", async ({ page }) => {
  await openBook(page, "#/architecture");
  const fig = figure(page, "containers");
  const label = fig.locator(".ad-node-label").first();
  const hLabel = (await label.boundingBox())!.height;
  expect(hLabel).toBeGreaterThanOrEqual(10);
  await expect(fig).toHaveClass(/is-cropped/);
  await expect(fig.locator(".pan-hint")).toBeVisible();
  // The focal card is inside the visible canvas.
  const canvas = (await fig.locator(".fig-canvas").boundingBox())!;
  const focal = (await fig.locator(".ad-node.is-focal .ad-card").boundingBox())!;
  expect(focal.x).toBeGreaterThanOrEqual(canvas.x - 1);
  expect(focal.x + focal.width).toBeLessThanOrEqual(canvas.x + canvas.width + 1);
  expect(focal.y).toBeGreaterThanOrEqual(canvas.y - 1);
  expect(focal.y + focal.height).toBeLessThanOrEqual(canvas.y + canvas.height + 1);
  const vh = page.viewportSize()!.height;
  expect(canvas.height).toBeLessThanOrEqual(vh * 0.72 + 2);
  await fig.locator(".pan-hint").click();
  await expect(fig).not.toHaveClass(/is-cropped/);
});

test("light architecture page screenshot", async ({ page }) => {
  await openBook(page, "#/architecture");
  await page.screenshot({ path: "test-results/book-architecture.png", fullPage: true });
});

test("small screens: no horizontal overflow, nav becomes a drawer", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openBook(page, "#/architecture");
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  const nav = page.locator(".sidenav");
  const hiddenBox = await nav.boundingBox();
  expect(hiddenBox!.x + hiddenBox!.width).toBeLessThanOrEqual(1);
  await page.getByRole("button", { name: "Open navigation" }).click();
  await expect.poll(async () => (await nav.boundingBox())!.x).toBeGreaterThanOrEqual(0);
  await nav.getByRole("link", { name: /^Payments Go/ }).click();
  await expect(page.locator(".article h1")).toHaveText("Payments");
  await expect(page.getByRole("button", { name: "Open navigation" })).toHaveAttribute("aria-expanded", "false");
});

test("behaviour pages: sequence flows, data model, API contract, functional spec and business requirements", async ({ page }) => {
  const errors = await openBook(page, "#/flows");
  const seq = figure(page, "flow-api-gateway-post-checkout");
  await expect(seq.locator("svg.nunki.ad-sequence")).toBeVisible();
  await expect(seq.locator(".ad-lifeline")).not.toHaveCount(0);
  await expect(seq.locator(".ad-edge-label text").first()).toHaveText(/^1\. POST \/checkout/);

  await page.locator(".sidenav .nav-link", { hasText: "Data & integrations" }).click();
  await expect(figure(page, "data-model").locator("svg.nunki.ad-er .ad-node.is-entity")).not.toHaveCount(0);
  await expect(figure(page, "lifecycle-orders-status").locator(".ad-node.is-state")).not.toHaveCount(0);

  await page.locator(".sidenav .nav-group", { hasText: "API reference" }).locator(".nav-link", { hasText: "API Gateway" }).click();
  await expect(page).toHaveURL(/#\/api\/api-gateway$/);
  await expect(page.locator(".article h2", { hasText: "POST /checkout" })).toBeVisible();
  await expect(page.locator(".callout", { hasText: "Contract coverage" })).toBeVisible();

  await page.locator(".sidenav .nav-link", { hasText: "Functional specification" }).click();
  // Identified by what it describes, not by position: `FR-<operation>-<digest>`.
  await expect(page.locator(".article h3", { hasText: /^FR-[a-z0-9-]+-[0-9a-f]{4}\b/ }).first()).toBeVisible();
  await expect(page.locator(".badge.gap").first()).toContainText("needs input");
  await expect(page.locator(".article h2", { hasText: "Business rules" })).toBeVisible();

  await page.locator(".sidenav .nav-link", { hasText: "Business requirements" }).click();
  await expect(page.locator(".article h2")).toHaveCount(9);
  await expect(page.locator(".callout.warning")).toContainText("authored, not generated");
  expect(errors).toEqual([]);
  await page.screenshot({ path: "test-results/book-functional.png", fullPage: true });
});

test("runtime page shows the declared environment, entry ports and startup order", async ({ page }) => {
  const errors = await openBook(page, "#/runtime");
  await expect(page.locator(".article h1")).toHaveText("Runtime & deployment");
  const fig = page.locator("figure.figure").first();
  await expect(fig.locator("svg.nunki .ad-node").first()).toBeVisible();
  await expect(fig.locator('svg.nunki .ad-node[data-id="external"]')).toBeVisible();
  await expect(page.locator(".article h3", { hasText: "How traffic gets in" }).first()).toBeVisible();
  await expect(page.locator("table").first()).toContainText("api-gateway");
  expect(errors).toEqual([]);
});
