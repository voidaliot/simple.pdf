import { expect, test } from "@playwright/test";

test("form edits, page text and printing all pages", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("simplepdf:recents", JSON.stringify([{ path: "C:\\sample.pdf", title: "sample", pinned: false, lastOpened: 1 }]));
    const calls: { command: string; args: any }[] = [];
    (window as any).pdfCalls = calls;
    let value = "Before";
    (window as any).__TAURI_INTERNALS__ = {
      async invoke(command: string, args: any) {
        calls.push({ command, args });
        if (command === "open_document") return { id: "pdf", path: "C:\\sample.pdf", title: "sample", page_count: 3 };
        if (command === "get_page_sizes") return Array.from({ length: 3 }, () => ({ width: 600, height: 800 }));
        if (command === "get_form_type") return "acroform";
        if (command === "get_form_fields") return args.pageIndex === 0 ? [{ index: 0, kind: "text", name: "Name", value, options: [], checked: false, multiline: false, read_only: false, rect: { left: .1, top: .1, width: .3, height: .04 }, action_type: "none" }] : [];
        if (command === "set_field_text_value") { value = args.value; return; }
        if (command === "add_page_text") return 1;
        if (command === "render_page_pixels" || command === "render_page_tile_pixels") {
          const width = args.width ?? Math.round(600 * args.scale);
          const height = args.height ?? Math.round(800 * args.scale);
          const bytes = new Uint8Array(8 + width * height * 4).fill(255);
          const view = new DataView(bytes.buffer); view.setUint32(0, width, true); view.setUint32(4, height, true);
          return bytes;
        }
        if (command === "render_thumb_b64") return "";
        return [];
      },
    };
    // Patch only the print frame's native dialog, retaining actual rendering and layout.
    new MutationObserver(() => {
      const frame = document.querySelector<HTMLIFrameElement>('iframe[title="Print document"]');
      if (!frame?.contentWindow) return;
      frame.contentWindow.print = () => {
        (window as any).printedText = frame.contentDocument!.body.textContent;
        (window as any).printedPages = frame.contentDocument!.querySelectorAll("section img").length;
        frame.contentWindow!.dispatchEvent(new Event("afterprint"));
      };
    }).observe(document, { childList: true, subtree: true });
  });
  await page.goto("/");
  await page.locator(".card").filter({ has: page.getByRole("heading", { name: "sample", exact: true }) }).click();
  await page.getByRole("textbox", { name: "Name", exact: true }).fill("After");
  await page.getByRole("button", { name: "Edit form fields", exact: true }).click();
  await page.getByRole("button", { name: "Edit form fields", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Name", exact: true })).toHaveValue("After");
  await page.getByRole("button", { name: "Add text", exact: true }).click();
  await page.locator(".click-capture").first().click({ position: { x: 120, y: 180 } });
  await page.getByRole("textbox", { name: "Page text", exact: true }).fill("Hello page");
  await page.getByRole("button", { name: "Add to page", exact: true }).click();
  await expect(page.getByRole("form", { name: "Add page text" })).toHaveCount(0);
  await page.getByRole("button", { name: "Save document", exact: true }).click();
  await page.getByRole("button", { name: "Print document", exact: true }).click();
  await expect.poll(() => page.evaluate(() => (window as any).printedPages)).toBe(3);
  expect(await page.evaluate(() => (window as any).printedText)).toContain("After");
  const calls = await page.evaluate(() => (window as any).pdfCalls);
  expect(calls.find((call: any) => call.command === "add_page_text").args.contents).toBe("Hello page");
  expect(calls.some((call: any) => call.command === "save_document")).toBe(true);
  expect(calls.filter((call: any) => call.command === "render_page_pixels" && call.args.forPrint).map((call: any) => call.args.pageIndex)).toEqual([0, 1, 2]);
  await expect(page.locator('iframe[title="Print document"]')).toHaveCount(0);
});
