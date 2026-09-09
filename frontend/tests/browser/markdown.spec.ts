import { expect, test, type Page } from "@playwright/test";

const docs = {
  "C:\\mixed.md": { title: "mixed", format: "markdown", source: '# Architecture\n\nText before.\n\n```mermaid\nsequenceDiagram\nAlice->>Bob: Hello\n```\n\n> ```plantuml\n> @startuml\n> A -> B : Nested\n> @enduml\n> ```\n\nText after.' },
  "C:\\unsafe.md": { title: "unsafe", format: "markdown", source: '# Safe heading\n\n<script>window.pwned=true</script>\n\n<img src="https://example.com/track" onerror="window.pwned=true">\n\n![remote](https://example.com/tracker)\n\n[bad](javascript:alert(1))\n\n```plantuml\n!include https://example.com/source.puml\n```' },
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

test("Markdown preserves prose and displays diagram fences as code", async ({ page }, testInfo) => {
  const external: string[] = [];
  page.on("request", request => { if (/^https?:/.test(request.url()) && !request.url().startsWith("http://127.0.0.1:")) external.push(request.url()); });
  await setup(page);
  await openRecent(page, "mixed");
  await expect(page.getByRole("heading", { name: "Architecture" })).toBeVisible();
  await expect(page.locator(".text-content pre code")).toHaveCount(2);
  await expect(page.locator(".text-content pre code").first()).toContainText("Alice->>Bob: Hello");
  await expect(page.locator(".text-content blockquote pre code")).toContainText("A -> B : Nested");
  await expect(page.getByText("Text after.", { exact: true })).toBeVisible();
  await expect(page.locator(".diagram, iframe, .text-content svg")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Export SVG" })).toHaveCount(0);
  expect(external).toEqual([]);
  await page.screenshot({ path: testInfo.outputPath("markdown.png"), fullPage: true });
});

test("untrusted Markdown and code fences cannot execute scripts or make network requests", async ({ page }) => {
  const external: string[] = [];
  page.on("request", (request) => { if (request.url().startsWith("https://example.com")) external.push(request.url()); });
  await setup(page);
  await openRecent(page, "unsafe");
  await expect(page.getByRole("heading", { name: "Safe heading" })).toBeVisible();
  await expect(page.locator(".text-content pre code")).toContainText("!include https://example.com/source.puml");
  expect(await page.evaluate(() => (window as any).pwned)).toBeUndefined();
  expect(external).toEqual([]);
  await expect(page.locator(".text-content script, .text-content img")).toHaveCount(0);
  expect(await page.locator(".text-content a").getAttribute("href")).toBeNull();
});

test("Markdown source view and reload remain available", async ({ page }) => {
  await setup(page);
  await openRecent(page, "mixed");
  await page.getByRole("button", { name: "View source", exact: true }).click();
  await expect(page.locator(".document-source")).toContainText("# Architecture");
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Architecture" })).toBeVisible();
  await expect(page.locator(".text-content pre code")).toHaveCount(2);
  await expect(page.getByRole("tab", { name: "mixed", exact: true })).toHaveCount(1);
});
