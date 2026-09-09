import { renderDiagram } from "../../src/lib/diagrams";
import type { DiagramFormat } from "../../src/lib/documentTypes";

export async function diagnose(format: DiagramFormat, source: string) {
  let app;
  try { app = await renderDiagram(format, source, new AbortController().signal); }
  catch (error) { app = { error: String(error) }; }
  let raw;
  if (format === "mermaid") {
    const { default: mermaid } = await import("mermaid");
    mermaid.initialize({ startOnLoad: false, securityLevel: "strict", suppressErrorRendering: true });
    const host = document.createElement("div");
    document.body.append(host);
    try { raw = await mermaid.render(`raw-${crypto.randomUUID()}`, source, host); }
    catch (error) { raw = { error: String(error) }; }
    finally { host.remove(); }
  }
  function stats(svg?: string) {
    if (!svg) return undefined;
    const parsed = new DOMParser().parseFromString(svg, "text/html");
    return {
      text: [...parsed.querySelectorAll("text, foreignObject")].map((node) => node.textContent).join(" | "),
      shapes: parsed.querySelectorAll("path, rect, circle, polygon").length,
      foreignObjects: parsed.querySelectorAll("foreignObject").length,
    };
  }
  return { app, raw, appStats: stats("svg" in app ? app.svg : undefined), rawStats: stats(raw && "svg" in raw ? raw.svg : undefined) };
}
