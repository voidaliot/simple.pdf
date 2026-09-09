// Optional local corpus diagnostic; does not copy input sources into the repo.
// Start Vite first, then: node scripts/audit-diagrams.mjs <folder> [...folders]
import fs from "node:fs";
import path from "node:path";
import { chromium } from "@playwright/test";
import { parseTextDocument } from "../src/lib/diagramSource.ts";

const output = path.resolve("../dist/diagram-audit");
fs.mkdirSync(output, { recursive: true });
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
await page.route(/https?:\/\/(?!127\.0\.0\.1:5173)/, (route) => route.abort());
await page.goto("http://127.0.0.1:5173");
const results = [];
try {
  for (const root of process.argv.slice(2)) {
    for (const name of fs.readdirSync(root, { recursive: true }).filter((name) => /\.(mmd|mermaid|puml|plantuml|md)$/.test(name))) {
      const file = path.join(root, name);
      const source = fs.readFileSync(file, "utf8");
      const format = /\.md$/.test(name) ? "markdown" : /\.(mmd|mermaid)$/.test(name) ? "mermaid" : "plantuml";
      const { diagrams } = parseTextDocument(format, source);
      for (const [index, diagram] of diagrams.entries()) {
        const result = await page.evaluate(async ({ format, source }) => {
          const { diagnose } = await import("/tests/diagnostics/renderer.ts");
          return diagnose(format, source);
        }, diagram);
        const id = `${results.length + 1}-${path.basename(file).replaceAll(".", "-")}-${index + 1}`;
        for (const kind of ["app", "raw"]) {
          if (result[kind]?.svg) fs.writeFileSync(path.join(output, `${id}-${kind}.svg`), result[kind].svg);
        }
        const row = { id, file, index: index + 1, format: diagram.format, appError: result.app?.error, rawError: result.raw?.error, appStats: result.appStats, rawStats: result.rawStats };
        results.push(row);
        fs.writeFileSync(path.join(output, "results.json"), JSON.stringify(results, null, 2));
        console.log(JSON.stringify({ id, file: name, index: index + 1, appError: row.appError, rawError: row.rawError, shapes: row.appStats?.shapes, textLength: row.appStats?.text?.length }));
      }
    }
  }
} finally { await browser.close(); }
