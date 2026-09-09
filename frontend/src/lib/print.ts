import { getFormFields, renderPagePixels, type PageSize } from "./ipc";

/** Render every page independently of the viewer's virtualized viewport. */
export async function printDocument(id: string, title: string, sizes: PageSize[], progress: (page: number) => void) {
  const frame = document.createElement("iframe");
  frame.title = "Print document";
  frame.style.cssText = "position:fixed;left:-10000px;top:0;width:800px;height:1000px;border:0";
  document.body.append(frame);
  const urls: string[] = [];
  const cleanup = () => { frame.remove(); urls.forEach(URL.revokeObjectURL); };
  try {
    const doc = frame.contentDocument!;
    doc.title = title;
    const style = doc.createElement("style");
    style.textContent = "html,body{margin:0;padding:0}img{display:block;max-width:100%;max-height:100%}section{break-after:page;overflow:hidden}section:last-child{break-after:auto}";
    doc.head.append(style);
    for (let i = 0; i < sizes.length; i++) {
      progress(i + 1);
      const size = sizes[i];
      // 150 dpi, capped for unusually large pages.
      const scale = Math.min(150 / 72, 4096 / Math.max(size.width, size.height), Math.sqrt(5_900_000 / (size.width * size.height)));
      const pixels = await renderPagePixels(id, i, scale, true);
      const canvas = document.createElement("canvas");
      canvas.width = pixels.width; canvas.height = pixels.height;
      canvas.getContext("2d")!.putImageData(new ImageData(pixels.data, pixels.width, pixels.height), 0, 0);
      const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob(b => b ? resolve(b) : reject(new Error("Could not prepare page for printing")), "image/png"));
      canvas.width = canvas.height = 0;
      const url = URL.createObjectURL(blob); urls.push(url);
      const section = doc.createElement("section");
      section.style.cssText = `position:relative;page:sheet${i};width:${size.width}pt;height:${size.height}pt`;
      style.textContent += `@page sheet${i}{size:${size.width}pt ${size.height}pt;margin:0}`;
      const img = doc.createElement("img");
      img.src = url; img.style.cssText = "width:100%;height:100%";
      section.append(img); doc.body.append(section);
      await img.decode();
      // PDFium's value setters do not regenerate every widget appearance.
      // Paint current values over their old appearances, as the viewer does.
      for (const field of await getFormFields(id, i)) {
        if (!["text", "checkbox", "radio", "combo", "list"].includes(field.kind)) continue;
        const el = doc.createElement("div");
        const rect = field.rect;
        const height = rect.height * size.height;
        el.style.cssText = `position:absolute;box-sizing:border-box;left:${rect.left * 100}%;top:${rect.top * 100}%;width:${rect.width * 100}%;height:${rect.height * 100}%;background:white;color:black;overflow:hidden;font-family:Arial,sans-serif;font-size:${Math.max(6, field.multiline ? 12 : height * .65)}pt;white-space:pre-wrap;line-height:1.1;padding:1pt 2pt`;
        if (field.kind === "checkbox" || field.kind === "radio") {
          el.style.cssText += `;border:1pt solid black;border-radius:${field.kind === "radio" ? "50%" : "0"};display:flex;align-items:center;justify-content:center;padding:0`;
          el.textContent = field.checked ? (field.kind === "radio" ? "●" : "✓") : "";
        } else { el.textContent = field.value; }
        section.append(el);
      }
    }
    const win = frame.contentWindow!;
    await new Promise<void>((resolve, reject) => {
      win.addEventListener("afterprint", () => { cleanup(); resolve(); }, { once: true });
      try { win.focus(); win.print(); } catch (error) { reject(error); }
    });
    // Keep the document alive until the native dialog closes.
  } catch (error) { cleanup(); throw error; }
}
