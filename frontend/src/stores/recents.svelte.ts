export interface RecentEntry {
  path: string;
  title: string;
  lastOpened: number;
  pinned: boolean;
}

const MAX_RECENTS = 50;
const KEY = "simplepdf:recents";

function createRecentsStore() {
  let entries = $state<RecentEntry[]>(load());

  function load(): RecentEntry[] {
    try {
      return JSON.parse(localStorage.getItem(KEY) ?? "[]");
    } catch {
      return [];
    }
  }

  function persist() {
    localStorage.setItem(KEY, JSON.stringify(entries));
  }

  function add(path: string, title: string) {
    entries = [
      { path, title, lastOpened: Date.now(), pinned: false },
      ...entries.filter((e) => e.path !== path),
    ].slice(0, MAX_RECENTS);
    persist();
  }

  function remove(path: string) {
    entries = entries.filter((e) => e.path !== path);
    persist();
  }

  function togglePin(path: string) {
    entries = entries.map((e) =>
      e.path === path ? { ...e, pinned: !e.pinned } : e
    );
    persist();
  }

  return {
    get entries() {
      return entries;
    },
    add,
    remove,
    togglePin,
  };
}

export const recents = createRecentsStore();
