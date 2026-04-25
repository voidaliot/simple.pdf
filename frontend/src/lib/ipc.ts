import { invoke } from "@tauri-apps/api/core";

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

export async function appVersion(): Promise<AppVersion> {
  return invoke<AppVersion>("app_version");
}

export async function openDocument(path: string): Promise<OpenedDocument> {
  return invoke<OpenedDocument>("open_document", { path });
}

export async function closeDocument(id: string): Promise<void> {
  return invoke("close_document", { id });
}

export async function getPageSizes(id: string): Promise<PageSize[]> {
  return invoke<PageSize[]>("get_page_sizes", { id });
}

export async function pendingOpenFiles(): Promise<string[]> {
  return invoke<string[]>("pending_open_files");
}

/** Build the pdf:// URL for a page image. */
export function pageUrl(docId: string, pageIndex: number, scale: number): string {
  const s = scale.toFixed(3);
  return `pdf://localhost/page/${docId}/${pageIndex}?scale=${s}`;
}
