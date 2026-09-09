import { expect, test } from "@playwright/test";

test("home excludes legacy text recents and only offers PDF files", async ({ page }) => {
  await page.addInitScript(() => {
    const paths = ["C:\\sample.pdf", "C:\\notes.md", "C:\\notes.markdown", "C:\\flow.mmd", "C:\\class.puml"];
    localStorage.setItem("simplepdf:recents", JSON.stringify(paths.map(path => ({ path, title: path.split("\\").pop(), pinned: true, lastOpened: 1 }))));
    (window as any).__TAURI_INTERNALS__ = {
      async invoke(command: string, args: any) {
        if (command === "plugin:dialog|open") {
          (window as any).pickerOptions = args.options;
          return "C:\\notes.md";
        }
        throw new Error(`Unexpected IPC command: ${command}`);
      },
    };
  });
  await page.goto("/");
  await expect(page.locator(".card")).toHaveCount(1);
  await expect(page.locator(".card")).toContainText("sample.pdf");
  await page.getByRole("button", { name: "Open file…", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Choose a PDF file");
  expect(await page.evaluate(() => (window as any).pickerOptions.filters)).toEqual([{ name: "PDF", extensions: ["pdf"] }]);
  await expect(page.getByRole("tab")).toHaveCount(1);
});
