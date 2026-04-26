import { open } from "@tauri-apps/plugin-dialog";
import { openDocument, listFolderPdfs, downloadUrlToTemp, thumbUrl } from "./ipc";
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

export async function pickFolderAndOpen(): Promise<void> {
  const selected = await open({ directory: true, multiple: false });
  if (typeof selected !== "string") return;
  const pdfs = await listFolderPdfs(selected);
  for (const p of pdfs) {
    await openPath(p).catch(console.error);
  }
}

export async function openFromUrl(url: string): Promise<void> {
  const tmpPath = await downloadUrlToTemp(url);
  await openPath(tmpPath);
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
  // Cache a page-0 thumbnail asynchronously
  cacheThumbnail(doc.path);
}

/** Fetch the thumbnail image and store it as a data URL in the recents store. */
async function cacheThumbnail(path: string): Promise<void> {
  // Only cache if no thumbnail yet
  const existing = recents.entries.find((e) => e.path === path);
  if (existing?.thumbnail) return;

  try {
    const url = thumbUrl(path, 240);
    const resp = await fetch(url);
    if (!resp.ok) return;
    const blob = await resp.blob();
    const reader = new FileReader();
    reader.onloadend = () => {
      if (typeof reader.result === "string") {
        recents.setThumbnail(path, reader.result);
      }
    };
    reader.readAsDataURL(blob);
  } catch {
    // thumbnail failure is non-fatal
  }
}
