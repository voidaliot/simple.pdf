import { invoke } from "@tauri-apps/api/core";

export interface OpenedDocument {
  id: string;
  path: string;
  title: string;
  page_count: number;
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

export async function pendingOpenFiles(): Promise<string[]> {
  return invoke<string[]>("pending_open_files");
}
