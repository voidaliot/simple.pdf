const STORAGE_KEY = "simplepdf-signatures";

export interface SavedSignature {
  id: string;
  paths: [number, number][][];
  thumbnail: string;
  isDefault: boolean;
  createdAt: number;
}

function load(): SavedSignature[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as SavedSignature[]) : [];
  } catch {
    return [];
  }
}

function persist(list: SavedSignature[]) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(list)); } catch { /**/ }
}

function createSignaturesStore() {
  let list = $state<SavedSignature[]>(load());

  function save(paths: [number, number][][], thumbnail: string) {
    const id = crypto.randomUUID();
    const isDefault = list.length === 0;
    list = [...list, { id, paths, thumbnail, isDefault, createdAt: Date.now() }];
    persist(list);
  }

  function remove(id: string) {
    const wasDefault = list.find((s) => s.id === id)?.isDefault ?? false;
    list = list.filter((s) => s.id !== id);
    if (wasDefault && list.length > 0) {
      list = list.map((s, i) => (i === 0 ? { ...s, isDefault: true } : s));
    }
    persist(list);
  }

  function setDefault(id: string) {
    list = list.map((s) => ({ ...s, isDefault: s.id === id }));
    persist(list);
  }

  return {
    get list() { return list; },
    get default() { return list.find((s) => s.isDefault) ?? null; },
    save,
    remove,
    setDefault,
  };
}

export const signatures = createSignaturesStore();
