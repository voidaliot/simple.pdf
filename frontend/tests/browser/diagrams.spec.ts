import { expect, test, type Page } from "@playwright/test";

const docs = {
  "C:\\flow.mmd": { title: "flow", format: "mermaid", source: 'flowchart LR\nA[Start] --> B{Ready?}\nB -->|Yes| C[Done]\nB -->|No| A' },
  "C:\\sequence.puml": { title: "sequence", format: "plantuml", source: '@startuml\nAlice -> Bob : Hello 👋\nBob --> Alice : Grüß\n@enduml' },
  "C:\\classes.puml": { title: "classes", format: "plantuml", source: '@startuml\nclass Animal\nclass Dog\nAnimal <|-- Dog\n@enduml' },
  "C:\\mixed.md": { title: "mixed", format: "markdown", source: '# Architecture\n\nText before.\n\n```mermaid\nsequenceDiagram\nAlice->>Bob: Hello\n```\n\n> ```plantuml\n> @startuml\n> A -> B : Nested\n> @enduml\n> ```\n\nText after.' },
  "C:\\broken.md": { title: "broken", format: "markdown", source: '```mermaid\nthis is not a diagram\n```\n\n```plantuml\nthis is invalid plantuml\n```\n\n```mermaid\ngraph TD\nA-->B\n```' },
  "C:\\unsafe.md": { title: "unsafe", format: "markdown", source: '# Safe heading\n\n<script>window.pwned=true</script>\n\n<img src="https://example.com/track" onerror="window.pwned=true">\n\n![remote](https://example.com/tracker)\n\n[bad](javascript:alert(1))\n\n```plantuml\n!include https://example.com/source.puml\n```' },
  "C:\\labels.mmd": { title: "labels", format: "mermaid", source: 'flowchart LR\nA["Line one<br>Line two #dagger; #nbsp; "] --> B["<b>Done</b>"]' },
  "C:\\preamble.puml": { title: "preamble", format: "plantuml", source: "' Explanatory preamble\n\n@startuml\nentity A {\nid\n}\nentity B {\nid\n}\nA ||--o{ B\n@enduml\nText after the diagram" },
  "C:\\icons.puml": { title: "icons", format: "plantuml", source: '@startuml\nrectangle "<&heart> Local icons"\n@enduml' },
  "C:\\styles.puml": { title: "styles", format: "plantuml", source: "skinparam monochrome true" },
  "C:\\wireframe.puml": { title: "wireframe", format: "plantuml", source: "@startsalt\n{ [Button] }\n@endsalt" },
} as const;

async function setup(page: Page) {
  await page.addInitScript((documents) => {
    localStorage.setItem("simplepdf:recents", JSON.stringify(Object.entries(documents).map(([path, doc]) => ({ path, title: doc.title, pinned: false, lastOpened: 1 }))));
    localStorage.setItem("simplepdf:theme", "light");
    (window as any).__TAURI_INTERNALS__ = {
      async invoke(command: string, args: { path?: string }) {
        if (command === "open_text_document") {
          const doc = (documents as Record<string, unknown>)[args.path!];
          if (!doc) throw new Error("File not found");
          return { ...(doc as object), path: args.path };
        }
        if (command === "open_external_uri") return;
        throw new Error(`Unexpected IPC command: ${command}`);
      },
    };
  }, docs);
  await page.goto("/");
}

async function openRecent(page: Page, title: string) {
  await page.locator(".card").filter({ has: page.getByRole("heading", { name: title, exact: true }) }).click();
}

for (const title of ["flow", "sequence", "classes", "preamble", "icons"]) {
  test(`${title} renders actual SVG with the production CSP, zooms and exports`, async ({ page }, testInfo) => {
    const external: string[] = [];
    page.on("request", (request) => { if (/^https?:/.test(request.url()) && !request.url().startsWith("http://127.0.0.1:")) external.push(request.url()); });
    await setup(page);
    await openRecent(page, title);
    const diagram = page.locator(".diagram");
    await expect(diagram.locator("img")).toBeVisible();
    await expect(diagram.getByRole("alert")).toHaveCount(0);
    expect(await diagram.locator("img").evaluate((img: HTMLImageElement) => img.naturalWidth > 0)).toBe(true);
    await diagram.getByRole("button", { name: "Zoom in" }).click();
    await expect(diagram.locator(".zoom-label")).toHaveText("125%");
    await diagram.getByRole("button", { name: "Fit", exact: true }).click();
    const downloadPromise = page.waitForEvent("download");
    await diagram.getByRole("button", { name: "Export SVG" }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe(`${title}-1.svg`);
    await download.saveAs(testInfo.outputPath(`${title}.svg`));
    await page.screenshot({ path: testInfo.outputPath(`${title}.png`) });
    expect(external).toEqual([]);
    await page.keyboard.press("Control+w");
    await expect(page.locator("iframe")).toHaveCount(0);
  });
}

test("Markdown preserves prose and renders nested fences from both engines", async ({ page }, testInfo) => {
  await setup(page);
  await openRecent(page, "mixed");
  await expect(page.getByRole("heading", { name: "Architecture" })).toBeVisible();
  await expect(page.locator(".diagram")).toHaveCount(2);
  for (const diagram of await page.locator(".diagram").all()) {
    await diagram.scrollIntoViewIfNeeded();
    await expect(diagram.locator("img")).toBeVisible();
  }
  await expect(page.getByText("Text after.", { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("mixed.png"), fullPage: true });
});

test("HTML labels and entities survive sanitizing and standalone SVG export", async ({ page }) => {
  await setup(page);
  await openRecent(page, "labels");
  await expect(page.locator(".diagram img")).toBeVisible();
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export SVG" }).click();
  const download = await downloadPromise;
  const stream = await download.createReadStream();
  const chunks: Buffer[] = [];
  for await (const chunk of stream!) chunks.push(chunk);
  const svg = Buffer.concat(chunks).toString("utf8");
  const contents = await page.evaluate((svg) => {
    const parsed = new DOMParser().parseFromString(svg, "image/svg+xml");
    return { error: !!parsed.querySelector("parsererror"), labels: Array.from(parsed.querySelectorAll("foreignObject"), (node) => node.textContent), breaks: parsed.querySelectorAll("br").length };
  }, svg);
  expect(contents.error).toBe(false);
  expect(contents.labels.join(" ")).toContain("Line oneLine two †");
  expect(contents.labels.join(" ")).toContain("Done");
  expect(contents.breaks).toBeGreaterThan(0);
});

test("empty style fragments and unsupported types show explanations instead of fake previews", async ({ page }) => {
  await setup(page);
  await openRecent(page, "styles");
  await expect(page.getByRole("alert")).toContainText("styles-only");
  await expect(page.locator(".diagram img")).toHaveCount(0);
  await page.getByRole("tab", { name: "New Tab", exact: true }).click();
  await openRecent(page, "wireframe");
  await expect(page.getByRole("alert")).toContainText("not supported by the bundled PlantUML");
  await expect(page.locator(".diagram img")).toHaveCount(0);
});

test("a malformed diagram does not stop the next renderer job", async ({ page }) => {
  await setup(page);
  await openRecent(page, "broken");
  await expect(page.locator(".diagram").nth(0).getByRole("alert")).toBeVisible();
  await page.locator(".diagram").nth(1).scrollIntoViewIfNeeded();
  await expect(page.locator(".diagram").nth(1).getByRole("alert")).toBeVisible();
  await page.locator(".diagram").nth(2).scrollIntoViewIfNeeded();
  await expect(page.locator(".diagram").nth(2).locator("img")).toBeVisible();
});

test("untrusted Markdown and includes cannot execute scripts or make network requests", async ({ page }) => {
  const external: string[] = [];
  page.on("request", (request) => { if (request.url().startsWith("https://example.com")) external.push(request.url()); });
  await setup(page);
  await openRecent(page, "unsafe");
  await expect(page.getByRole("heading", { name: "Safe heading" })).toBeVisible();
  await expect(page.getByText(/unavailable offline/)).toBeVisible();
  expect(await page.evaluate(() => (window as any).pwned)).toBeUndefined();
  expect(external).toEqual([]);
  await expect(page.locator(".text-content script, .text-content img")).toHaveCount(0);
  expect(await page.locator(".text-content a").getAttribute("href")).toBeNull();
});

test("source view, reload and rapid tab switches clean up rendering work", async ({ page }) => {
  await setup(page);
  await openRecent(page, "sequence");
  await page.getByRole("button", { name: "View source", exact: true }).click();
  await expect(page.locator(".document-source")).toContainText("Alice -> Bob");
  await expect(page.locator("iframe")).toHaveCount(0);
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await page.getByRole("tab", { name: "New Tab" }).click();
  await expect(page.locator("iframe")).toHaveCount(0);
  await openRecent(page, "flow");
  await expect(page.locator(".diagram img")).toBeVisible();
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(page.locator(".diagram img")).toBeVisible();
  await expect(page.getByRole("tab", { name: "flow" })).toHaveCount(1);
});
