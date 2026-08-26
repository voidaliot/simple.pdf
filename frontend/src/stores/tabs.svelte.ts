import { closeDocument } from "../lib/ipc";
import { disposeViewerStore } from "./viewer.svelte";

export type TabKind = "home" | "doc" | "settings";

export interface Tab {
  id: string;
  kind: TabKind;
  title: string;
  docId?: string;
  path?: string;
  pageCount?: number;
  dirty: boolean;
}

let nextId = 1;
const genId = () => `t${nextId++}`;

/** Normalize a Windows document path for open-tab comparisons. */
export function documentPathKey(path: string): string {
  return path.replaceAll("/", "\\").toLowerCase();
}

// Pre-compute the initial tab id so activeId doesn't read a $state during
// its own initializer (avoids the state_referenced_locally Svelte warning).
const _initialTabId = genId();

function createTabsStore() {
  let list = $state<Tab[]>([
    { id: _initialTabId, kind: "home", title: "New Tab", dirty: false },
  ]);
  let activeId = $state<string>(_initialTabId);
  const active = $derived(list.find((t) => t.id === activeId) ?? null);

  function openHome(): Tab {
    const tab: Tab = { id: genId(), kind: "home", title: "New Tab", dirty: false };
    list = [...list, tab];
    activeId = tab.id;
    return tab;
  }

  function openSettings(): Tab {
    const existing = list.find((t) => t.kind === "settings");
    if (existing) { activeId = existing.id; return existing; }
    const tab: Tab = { id: genId(), kind: "settings", title: "Settings", dirty: false };
    list = [...list, tab];
    activeId = tab.id;
    return tab;
  }

  function findByPath(path: string): Tab | undefined {
    const key = documentPathKey(path);
    return list.find((t) =>
      t.kind === "doc" && t.path !== undefined && documentPathKey(t.path) === key
    );
  }

  function activatePath(path: string): Tab | undefined {
    const existing = findByPath(path);
    if (existing) activeId = existing.id;
    return existing;
  }

  function openDoc(info: { id: string; path: string; title: string; pageCount: number }): Tab {
    const existing = findByPath(info.path);
    if (existing) { activeId = existing.id; return existing; }
    const tab: Tab = {
      id: genId(),
      kind: "doc",
      title: info.title,
      docId: info.id,
      path: info.path,
      pageCount: info.pageCount,
      dirty: false,
    };
    list = [...list, tab];
    activeId = tab.id;
    return tab;
  }

  function close(id: string): boolean {
    const idx = list.findIndex((t) => t.id === id);
    if (idx === -1) return false;
    const removed = list[idx]!;
    if (removed.dirty) {
      const discard = confirm(`"${removed.title}" has unsaved changes.\nDiscard changes and close?`);
      if (!discard) return false;
    }

    list = list.filter((t) => t.id !== id);

    if (removed.docId) {
      disposeViewerStore(removed.docId);
      void closeDocument(removed.docId).catch((error: unknown) => {
        console.error("failed to close document", error);
      });
    }

    if (list.length === 0) {
      const home = openHome();
      activeId = home.id;
      return true;
    }
    if (activeId === removed.id) {
      const fallback = list[Math.min(idx, list.length - 1)]!;
      activeId = fallback.id;
    }
    return true;
  }

  function activate(id: string) {
    if (list.some((t) => t.id === id)) activeId = id;
  }

  function reorder(from: number, to: number) {
    if (from === to) return;
    const copy = [...list];
    const [moved] = copy.splice(from, 1);
    copy.splice(to, 0, moved!);
    list = copy;
  }

  function markDirty(id: string, dirty: boolean) {
    list = list.map((t) => t.id === id ? { ...t, dirty } : t);
  }

  return {
    get list() { return list; },
    get activeId() { return activeId; },
    get active() { return active; },
    openHome,
    openSettings,
    findByPath,
    activatePath,
    openDoc,
    close,
    activate,
    reorder,
    markDirty,
  };
}

export const tabs = createTabsStore();
