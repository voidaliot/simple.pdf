export type TabKind = "home" | "doc";

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

function createTabsStore() {
  let list = $state<Tab[]>([
    { id: genId(), kind: "home", title: "New Tab", dirty: false },
  ]);
  let activeId = $state<string>(list[0]!.id);
  const active = $derived(list.find((t) => t.id === activeId) ?? null);

  function openHome(): Tab {
    const tab: Tab = { id: genId(), kind: "home", title: "New Tab", dirty: false };
    list = [...list, tab];
    activeId = tab.id;
    return tab;
  }

  function openDoc(info: { id: string; path: string; title: string; pageCount: number }): Tab {
    const existing = list.find((t) => t.path === info.path);
    if (existing) {
      activeId = existing.id;
      return existing;
    }
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

  function close(id: string) {
    const idx = list.findIndex((t) => t.id === id);
    if (idx === -1) return;
    const removed = list[idx]!;
    list = list.filter((t) => t.id !== id);
    if (list.length === 0) {
      const home = openHome();
      activeId = home.id;
      return;
    }
    if (activeId === removed.id) {
      const fallback = list[Math.min(idx, list.length - 1)]!;
      activeId = fallback.id;
    }
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

  return {
    get list() { return list; },
    get activeId() { return activeId; },
    get active() { return active; },
    openHome,
    openDoc,
    close,
    activate,
    reorder,
  };
}

export const tabs = createTabsStore();
