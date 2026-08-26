import { invoke } from "@tauri-apps/api/core";

// ── Basic types ────────────────────────────────────────────────────────────────

export interface OpenedDocument {
  id: string;
  path: string;
  title: string;
  page_count: number;
}

export interface PageSize {
  width: number;
  height: number;
}

export interface AppVersion {
  name: string;
  version: string;
  pdfium_version: string | null;
}

/** Word-level text span with normalized bounding box ([0,1] relative to page). */
export interface TextSpan {
  text: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

// ── Annotation types ──────────────────────────────────────────────────────────

export interface AnnRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Annotation {
  index: number;
  kind: "highlight" | "underline" | "squiggly" | "strikeout" | "text" | "ink" | "link" | "widget" | "stamp" | "freetext" | "other";
  rect: AnnRect;
  /** RGBA each 0-255 */
  color: [number, number, number, number];
  contents: string | null;
  author: string | null;
}

// ── Document lifecycle ─────────────────────────────────────────────────────────

export async function appVersion(): Promise<AppVersion> {
  return invoke<AppVersion>("app_version");
}

export async function openDocument(path: string): Promise<OpenedDocument> {
  return invoke<OpenedDocument>("open_document", { path });
}

export async function closeDocument(id: string): Promise<void> {
  return invoke("close_document", { id });
}

export async function pendingOpenFiles(): Promise<string[]> {
  return invoke<string[]>("pending_open_files");
}

// ── Page data ─────────────────────────────────────────────────────────────────

export async function getPageSizes(id: string): Promise<PageSize[]> {
  return invoke<PageSize[]>("get_page_sizes", { id });
}

export async function getPageTextSpans(id: string, pageIndex: number): Promise<TextSpan[]> {
  return invoke<TextSpan[]>("get_page_text_spans", { id, pageIndex });
}

/**
 * Render one page and return raw RGBA pixel data via binary IPC.
 *
 * The Rust command packs: 4 bytes width (u32 LE) + 4 bytes height (u32 LE)
 * + width×height×4 bytes of RGBA with alpha=255 everywhere.
 *
 * Returns an object ready for `new ImageData(data, width, height)` and
 * `ctx.putImageData(imageData, 0, 0)`.  No image codec, no base64, no
 * transparency-group blackout.
 */
export async function renderPagePixels(
  docId: string,
  pageIndex: number,
  scale: number,
): Promise<{ width: number; height: number; data: Uint8ClampedArray<ArrayBuffer> }> {
  // Tauri's custom-protocol transport returns an ArrayBuffer. If WebView2
  // blocks that protocol, Tauri falls back to postMessage and serializes the
  // same Rust Vec<u8> as a number[]. Normalize both transports here; the
  // generic passed to invoke is only a TypeScript assertion and performs no
  // runtime conversion.
  const payload = await invoke<ArrayBuffer | Uint8Array | number[]>("render_page_pixels", {
    id: docId,
    pageIndex,
    scale,
  });
  const bytes = normalizeBinaryResponse(payload);
  if (bytes.byteLength < 8) throw new Error("Renderer returned an incomplete page frame");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const pixelBytes = width * height * 4;
  if (width === 0 || height === 0 || !Number.isSafeInteger(pixelBytes)) {
    throw new Error(`Renderer returned invalid page dimensions (${width} x ${height})`);
  }
  if (bytes.byteLength !== 8 + pixelBytes) {
    throw new Error(
      `Renderer returned ${bytes.byteLength - 8} pixel bytes; expected ${pixelBytes}`,
    );
  }
  const data = new Uint8ClampedArray(
    bytes.buffer,
    bytes.byteOffset + 8,
    pixelBytes,
  );
  return { width, height, data };
}

function normalizeBinaryResponse(payload: unknown): Uint8Array<ArrayBuffer> {
  if (payload instanceof ArrayBuffer) return new Uint8Array(payload);

  if (ArrayBuffer.isView(payload)) {
    // Copy views so the returned buffer is always a plain ArrayBuffer and the
    // ImageData constructor receives a stable, tightly-owned byte range.
    const view = new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength);
    return Uint8Array.from(view);
  }

  if (Array.isArray(payload)) {
    if (!payload.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) {
      throw new Error("Renderer returned invalid binary page data");
    }
    return Uint8Array.from(payload);
  }

  throw new Error("Renderer returned an unsupported binary response");
}

/**
 * Render page 0 of an on-disk PDF at thumbnail size, returned as a data URL.
 * Uses IPC instead of the thumb:// custom scheme.
 */
export async function renderThumbB64(filePath: string, maxW = 240): Promise<string> {
  const b64 = await invoke<string>("render_thumb_b64", { path: filePath, maxW });
  return `data:image/jpeg;base64,${b64}`;
}

// ── Forms ─────────────────────────────────────────────────────────────────────

export interface FormField {
  index: number;
  /** "text" | "checkbox" | "radio" | "combo" | "list" | "push" | "signature" | "other" */
  kind: string;
  name: string;
  value: string;
  options: string[];
  checked: boolean;
  multiline: boolean;
  rect: AnnRect;
  /** For push buttons: "reset" | "submit" | "other". "none" for non-button fields. */
  action_type: string;
}

export async function getFormType(id: string): Promise<string> {
  return invoke<string>("get_form_type", { id });
}

export async function getFormFields(id: string, pageIndex: number): Promise<FormField[]> {
  return invoke<FormField[]>("get_form_fields", { id, pageIndex });
}

export async function setFieldTextValue(
  id: string,
  pageIndex: number,
  annotIndex: number,
  value: string,
): Promise<void> {
  return invoke("set_field_text_value", { id, pageIndex, annotIndex, value });
}

export async function setFieldChecked(
  id: string,
  pageIndex: number,
  annotIndex: number,
  checked: boolean,
): Promise<void> {
  return invoke("set_field_checked", { id, pageIndex, annotIndex, checked });
}

export async function resetFormFields(id: string, pageIndex: number): Promise<void> {
  return invoke("reset_form_fields", { id, pageIndex });
}

export async function resetAllFormFields(id: string): Promise<void> {
  return invoke("reset_all_form_fields", { id });
}

// ── Annotations ───────────────────────────────────────────────────────────────

export async function getPageAnnotations(id: string, pageIndex: number): Promise<Annotation[]> {
  return invoke<Annotation[]>("get_page_annotations", { id, pageIndex });
}

export async function addHighlightAnnotation(
  id: string,
  pageIndex: number,
  rects: AnnRect[],
  color: [number, number, number],
  opacity: number,
): Promise<number> {
  return invoke<number>("add_highlight_annotation", { id, pageIndex, rects, color, opacity });
}

export async function addUnderlineAnnotation(
  id: string,
  pageIndex: number,
  rects: AnnRect[],
  color: [number, number, number],
): Promise<number> {
  return invoke<number>("add_underline_annotation", { id, pageIndex, rects, color });
}

export async function addStrikeoutAnnotation(
  id: string,
  pageIndex: number,
  rects: AnnRect[],
  color: [number, number, number],
): Promise<number> {
  return invoke<number>("add_strikeout_annotation", { id, pageIndex, rects, color });
}

export async function addTextAnnotation(
  id: string,
  pageIndex: number,
  left: number,
  top: number,
  contents: string,
  author: string | null,
  color: [number, number, number],
): Promise<number> {
  return invoke<number>("add_text_annotation", { id, pageIndex, left, top, contents, author, color });
}

export async function addInkAnnotation(
  id: string,
  pageIndex: number,
  paths: [number, number][][],
  color: [number, number, number],
  width: number,
): Promise<number> {
  return invoke<number>("add_ink_annotation", { id, pageIndex, paths, color, width });
}

export async function removeAnnotation(
  id: string,
  pageIndex: number,
  annotIndex: number,
): Promise<void> {
  return invoke("remove_annotation", { id, pageIndex, annotIndex });
}

export async function undoAnnotation(id: string): Promise<number | null> {
  return invoke<number | null>("undo_annotation", { id });
}

export async function saveDocument(id: string): Promise<void> {
  return invoke("save_document", { id });
}

// ── File system ───────────────────────────────────────────────────────────────

export async function listFolderPdfs(path: string): Promise<string[]> {
  return invoke<string[]>("list_folder_pdfs", { path });
}

export async function revealInExplorer(path: string): Promise<void> {
  return invoke("reveal_in_explorer", { path });
}

// ── File association ──────────────────────────────────────────────────────────

export async function getPdfAssociation(): Promise<boolean> {
  return invoke<boolean>("get_pdf_association");
}

export async function setPdfAssociation(enable: boolean): Promise<void> {
  return invoke("set_pdf_association", { enable });
}

// ── Network ───────────────────────────────────────────────────────────────────

export async function downloadUrlToTemp(url: string): Promise<string> {
  return invoke<string>("download_url_to_temp", { url });
}
