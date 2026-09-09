import { documentPathKey } from "../lib/documentTypes";
import { addRecent, loadRecents, persistRecents, type RecentEntry } from "../lib/recentFiles";
export type { RecentEntry } from "../lib/recentFiles";
const KEY = "simplepdf:recents";

function createRecentsStore() {
  let entries = $state<RecentEntry[]>(load());

  function load(): RecentEntry[] {
    try {
      return loadRecents(localStorage.getItem(KEY));
    } catch {
      return [];
    }
  }

  function persist() {
    persistRecents(localStorage, KEY, entries);
  }

  function add(path: string, title: string) {
    entries = addRecent(entries, path, title, Date.now());
    persist();
  }

  function remove(path: string) {
    entries = entries.filter((e) => documentPathKey(e.path) !== documentPathKey(path));
    persist();
  }

  function togglePin(path: string) {
    entries = entries.map((e) =>
      documentPathKey(e.path) === documentPathKey(path) ? { ...e, pinned: !e.pinned } : e
    );
    persist();
  }

  function setThumbnail(path: string, dataUrl: string) {
    entries = entries.map((e) =>
      documentPathKey(e.path) === documentPathKey(path) ? { ...e, thumbnail: dataUrl } : e
    );
    persist();
  }

  return {
    get entries() { return entries; },
    add,
    remove,
    togglePin,
    setThumbnail,
  };
}

export const recents = createRecentsStore();
