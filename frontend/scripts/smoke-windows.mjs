// Run against a built executable with no other simple.pdf instance running.
// Uses real Tauri IPC and a separate WebView2 data folder; no IPC mocks.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { chromium, expect } from "@playwright/test";

const root = fileURLToPath(new URL("../../", import.meta.url));
const executable = path.resolve(process.argv[2] ?? path.join(root, "dist/portable/simple.pdf.exe"));
const output = path.join(root, "dist/native-smoke");
fs.mkdirSync(output, { recursive: true });
const diagrams = path.join(output, "smoke-diagrams.md");
fs.writeFileSync(diagrams, '# Native renderer check\n\n```mermaid\nflowchart LR\nA["Line one<br>Line two #dagger;"] --> B[Done]\n```\n\n```plantuml\n\' A commented preamble\n@startuml\nrectangle "<&heart> Local icons"\n@enduml\n```\n');
const pdfPath = path.join(output, "native-close-check.pdf");
const objects = [
  "<< /Type /Catalog /Pages 2 0 R >>",
  "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
  "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] >>",
];
let pdf = "%PDF-1.7\n";
const offsets = [0];
for (const [index, object] of objects.entries()) {
  offsets.push(pdf.length);
  pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
}
const xref = pdf.length;
pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
for (const offset of offsets.slice(1)) pdf += `${String(offset).padStart(10, "0")} 00000 n \n`;
pdf += `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
fs.writeFileSync(pdfPath, pdf);

async function run(files, inspect) {
  const child = spawn(executable, files, {
    cwd: path.dirname(executable), windowsHide: true, stdio: "ignore",
    env: {
      ...process.env,
      WEBVIEW2_USER_DATA_FOLDER: path.join(output, "webview-profile"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9223 --remote-debugging-address=127.0.0.1",
    },
  });
  let exited = false;
  const exit = new Promise((resolve) => child.once("exit", (code) => { exited = true; resolve(code); }));
  let browser;
  try {
    for (let attempt = 0; attempt < 60 && !browser && !exited; attempt++) {
      try { browser = await chromium.connectOverCDP("http://127.0.0.1:9223"); }
      catch { await new Promise((resolve) => setTimeout(resolve, 250)); }
    }
    assert.ok(browser, "Native app did not start; close any existing simple.pdf instance before this test.");
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? await context.waitForEvent("page");
    await page.waitForURL("http://tauri.localhost/**");
    await expect(page.getByRole("button", { name: "Close", exact: true })).toBeVisible();
    assert.equal(await page.evaluate(() => window.isTauri), true);
    await inspect(page);
    let closeTimeout;
    let code;
    try {
      code = await Promise.race([exit, new Promise((_, reject) => { closeTimeout = setTimeout(() => reject(new Error("Native close did not terminate the app")), 10_000); })]);
    } finally { clearTimeout(closeTimeout); }
    assert.equal(code, 0);
  } finally {
    await browser?.close().catch(() => undefined);
    if (!exited) child.kill(); // Only the process created by this test.
  }
}

await run([diagrams], async (page) => {
  await expect(page.getByRole("tab", { name: "smoke-diagrams", exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "smoke-diagrams", exact: true }).click();
  await expect(page.locator(".diagram")).toHaveCount(2);
  for (const diagram of await page.locator(".diagram").all()) {
    await diagram.scrollIntoViewIfNeeded();
    await expect(diagram.locator("img")).toBeVisible({ timeout: 30_000 });
    assert.ok(await diagram.locator("img").evaluate((image) => image.naturalWidth > 0));
  }
  await expect(page.locator(".diagram-error")).toHaveCount(0);
  await page.screenshot({ path: path.join(output, "native-diagrams.png") });
  await page.getByRole("button", { name: "Close", exact: true }).click();
});
console.log("PASS: packaged Mermaid and PlantUML render; clean native window closes.");

await run([pdfPath], async (page) => {
  await expect(page.locator(".page-wrapper")).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "Markup tools", exact: true }).click();
  await page.getByRole("button", { name: "Sticky note", exact: true }).click();
  page.once("dialog", (dialog) => dialog.accept("Native close regression"));
  await page.locator(".page-wrapper").click({ position: { x: 100, y: 100 } });
  await expect(page.getByRole("button", { name: "Save document", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: "Close native-close-check tab", exact: true }).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("tab", { name: /native-close-check/ })).toBeVisible();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("button", { name: "Save document", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Close", exact: true }).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.getByRole("button", { name: "Discard changes", exact: true }).click();
});
console.log("PASS: cancel preserves dirty tabs and windows; discard closes the native process.");
