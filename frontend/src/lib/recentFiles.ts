import { documentPathKey } from "./documentTypes.ts";

export interface RecentEntry {
  path: string;
  title: string;
  lastOpened: number;
  pinned: boolean;
  thumbnail?: string;
}

export const MAX_RECENTS = 50;

export function loadRecents(raw: string | null): RecentEntry[] {
  try {
    const value: unknown = JSON.parse(raw ?? "[]");
    if (!Array.isArray(value)) return [];
    const seen = new Set<string>();
    return value.filter((entry): entry is RecentEntry => {
      if (!entry || typeof entry.path !== "string" || !entry.path || typeof entry.title !== "string"
        || !Number.isFinite(entry.lastOpened) || typeof entry.pinned !== "boolean") return false;
      const key = documentPathKey(entry.path);
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    }).slice(0, MAX_RECENTS).map((entry) => ({
      path: entry.path, title: entry.title, lastOpened: entry.lastOpened, pinned: entry.pinned,
      thumbnail: typeof entry.thumbnail === "string" && entry.thumbnail.startsWith("data:image/jpeg;base64,")
        && entry.thumbnail.length < 500_000 ? entry.thumbnail : undefined,
    }));
  } catch { return []; }
}

export function addRecent(entries: RecentEntry[], path: string, title: string, now: number): RecentEntry[] {
  const key = documentPathKey(path);
  const existing = entries.find((entry) => documentPathKey(entry.path) === key);
  const next: RecentEntry = { path, title, lastOpened: now, pinned: existing?.pinned ?? false, thumbnail: existing?.thumbnail };
  const remaining = entries.filter((entry) => documentPathKey(entry.path) !== key);
  // Evict the oldest unpinned file; opening a file must not silently remove a pin.
  const result = [next, ...remaining];
  while (result.length > MAX_RECENTS) {
    let index = result.length - 1;
    while (index > 0 && result[index]!.pinned) index--;
    if (index === 0) return remaining; // All slots pinned: keep those pins.
    result.splice(index, 1);
  }
  return result;
}

export function persistRecents(storage: Pick<Storage, "setItem">, key: string, entries: RecentEntry[]): void {
  try { storage.setItem(key, JSON.stringify(entries)); }
  catch {
    // Thumbnail quota exhaustion must never prevent opening a document or leak
    // the native PDF handle created just before updating recents.
    try { storage.setItem(key, JSON.stringify(entries.map(({ thumbnail: _thumbnail, ...entry }) => entry))); }
    catch { /* The in-memory recent list still works when storage is unavailable. */ }
  }
}
