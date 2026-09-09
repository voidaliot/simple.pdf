import { closeDocument } from "../lib/ipc";
import { documentPathKey } from "../lib/documentTypes";
export { documentPathKey } from "../lib/documentTypes";
import { disposeViewerStore } from "./viewer.svelte";
import { discardPrompt } from "./discard.svelte";

export type TabKind = "home" | "doc" | "settings";

export interface Tab {
  id: string;
  kind: TabKind;
  title: string;
  docId?: string;
  path?: string;
  pageCount?: number;
  dirty: boolean;
  changeVersion?: number;
}

let nextId = 1;
const genId = () => `t${nextId++}`;

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
      t.path !== undefined && documentPathKey(t.path) === key
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

  const closingTabs = new Map<string, Promise<boolean>>();

  function close(id: string): Promise<boolean> {
    const pending = closingTabs.get(id);
    if (pending) return pending;
    const closing = closeTab(id).finally(() => closingTabs.delete(id));
    closingTabs.set(id, closing);
    return closing;
  }

  async function closeTab(id: string): Promise<boolean> {
    let idx = list.findIndex((t) => t.id === id);
    if (idx === -1) return false;
    const removed = list[idx]!;
    if (removed.dirty) {
      const discard = await discardPrompt.request(`Close "${removed.title}" and discard its unsaved changes?`);
      if (!discard) return false;
    }

    idx = list.findIndex((tab) => tab.id === id);
    if (idx < 0) return false;

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
    if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) return;
    const copy = [...list];
    const [moved] = copy.splice(from, 1);
    copy.splice(to, 0, moved!);
    list = copy;
  }

  function markDirty(id: string, dirty: boolean) {
    list = list.map((t) => t.id === id ? { ...t, dirty, changeVersion: (t.changeVersion ?? 0) + (dirty ? 1 : 0) } : t);
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
