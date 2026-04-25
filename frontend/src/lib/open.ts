import { open } from "@tauri-apps/plugin-dialog";
import { openDocument } from "./ipc";
import { tabs } from "../stores/tabs.svelte";
import { recents } from "../stores/recents.svelte";

export async function pickAndOpen(): Promise<void> {
  const selected = await open({
    multiple: false,
    filters: [{ name: "PDF", extensions: ["pdf"] }],
  });
  if (typeof selected === "string") {
    await openPath(selected);
  }
}

export async function openPath(path: string): Promise<void> {
  const doc = await openDocument(path);
  recents.add(doc.path, doc.title);
  tabs.openDoc({
    id: doc.id,
    path: doc.path,
    title: doc.title,
    pageCount: doc.page_count,
  });
}
