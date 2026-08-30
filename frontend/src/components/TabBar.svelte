<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { tabs, type Tab } from "../stores/tabs.svelte";
  import Icon from "./Icon.svelte";

  const appWindow = isTauri() ? getCurrentWindow() : null;

  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);
  let maximized = $state(false);
  let tabMenuOpen = $state(false);
  let tablistEl: HTMLDivElement | undefined = $state();
  let tabMenuEl: HTMLDivElement | undefined = $state();
  let tabContextMenu = $state<{ x: number; y: number; tab: Tab } | null>(null);
  let tabContextMenuEl: HTMLDivElement | undefined = $state();
  let tabContextMenuButton: HTMLButtonElement | undefined = $state();
  let clipboardAnnouncement = $state("");

  $effect(() => {
    const activeId = tabs.activeId;
    requestAnimationFrame(() => {
      document.getElementById(`tab-${activeId}`)?.scrollIntoView({
        behavior: "smooth",
        block: "nearest",
        inline: "nearest",
      });
    });
  });

  onMount(() => {
    let disposed = false;
    let unlistenResize: (() => void) | undefined;

    async function updateMaximized() {
      if (!appWindow) return;
      try {
        const next = await appWindow.isMaximized();
        if (!disposed) maximized = next;
      } catch (error) {
        console.error("failed to read window state", error);
      }
    }

    void updateMaximized();
    if (appWindow) {
      void appWindow.onResized(() => void updateMaximized()).then((unlisten) => {
        if (disposed) unlisten();
        else unlistenResize = unlisten;
      });
    }

    return () => {
      disposed = true;
      unlistenResize?.();
    };
  });

  function focusTab(index: number) {
    const count = tabs.list.length;
    if (count === 0) return;
    const targetIndex = (index + count) % count;
    tabs.activate(tabs.list[targetIndex]!.id);
    requestAnimationFrame(() => {
      tablistEl?.querySelectorAll<HTMLButtonElement>(".tab-main")[targetIndex]?.focus();
    });
  }

  function onTabKeyDown(e: KeyboardEvent, index: number, tab: Tab) {
    if ((e.shiftKey && e.key === "F10") || e.key === "ContextMenu") {
      if (!tab.path) return;
      e.preventDefault();
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      openTabContextMenu(rect.left + 12, rect.bottom - 2, tab);
      return;
    }
    let target: number | undefined;
    if (e.key === "ArrowRight") target = index + 1;
    else if (e.key === "ArrowLeft") target = index - 1;
    else if (e.key === "Home") target = 0;
    else if (e.key === "End") target = tabs.list.length - 1;
    if (target === undefined) return;
    e.preventDefault();
    focusTab(target);
  }

  function closeTab(id: string) {
    if (!tabs.close(id)) return;
    requestAnimationFrame(() => {
      const activeIndex = tabs.list.findIndex((tab) => tab.id === tabs.activeId);
      if (activeIndex >= 0) {
        tablistEl?.querySelectorAll<HTMLButtonElement>(".tab-main")[activeIndex]?.focus();
      }
    });
  }

  function onMouseDown(e: MouseEvent, id: string) {
    if (e.button === 1) {
      e.preventDefault();
      closeTab(id);
    }
  }

  function onDragStart(e: DragEvent, idx: number) {
    dragFrom = idx;
    tabMenuOpen = false;
    tabContextMenu = null;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      e.dataTransfer.setData("text/plain", tabs.list[idx]?.id ?? "");
    }
  }

  function onDragOver(e: DragEvent, idx: number) {
    if (dragFrom === null || dragFrom === idx) return;
    e.preventDefault();
    dragOver = idx;
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  }

  function onDrop(e: DragEvent, idx: number) {
    e.preventDefault();
    if (dragFrom !== null && dragFrom !== idx) tabs.reorder(dragFrom, idx);
    dragFrom = null;
    dragOver = null;
  }

  function onDragEnd() {
    dragFrom = null;
    dragOver = null;
  }

  function activateFromMenu(id: string) {
    tabs.activate(id);
    tabMenuOpen = false;
  }

  function onWindowPointerDown(e: PointerEvent) {
    if (tabMenuOpen && !tabMenuEl?.contains(e.target as Node)) tabMenuOpen = false;
    if (tabContextMenu && !tabContextMenuEl?.contains(e.target as Node)) tabContextMenu = null;
  }

  function onWindowKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      tabMenuOpen = false;
      closeTabContextMenu(true);
    }
  }

  function openTabContextMenu(x: number, y: number, tab: Tab) {
    if (!tab.path) return;
    tabMenuOpen = false;
    tabContextMenu = {
      x: Math.max(4, Math.min(x, window.innerWidth - 196)),
      y: Math.max(4, Math.min(y, window.innerHeight - 52)),
      tab,
    };
    requestAnimationFrame(() => tabContextMenuButton?.focus());
  }

  function closeTabContextMenu(restoreTabFocus = false) {
    const tabId = tabContextMenu?.tab.id;
    tabContextMenu = null;
    if (restoreTabFocus && tabId) {
      requestAnimationFrame(() => document.getElementById(`tab-${tabId}`)?.focus());
    }
  }

  function onTabContextMenu(e: MouseEvent, tab: Tab) {
    if (!tab.path) return;
    e.preventDefault();
    openTabContextMenu(e.clientX, e.clientY, tab);
  }

  async function copyTabPath() {
    const path = tabContextMenu?.tab.path;
    if (!path) return;
    try {
      await navigator.clipboard.writeText(path);
      clipboardAnnouncement = "Path copied";
    } catch (error) {
      console.error("failed to copy document path", error);
      clipboardAnnouncement = "Could not copy path";
    } finally {
      closeTabContextMenu(true);
    }
  }

  async function minimizeWindow() {
    if (!appWindow) return;
    try {
      await appWindow.minimize();
    } catch (error) {
      console.error("failed to minimize window", error);
    }
  }

  async function toggleMaximizeWindow() {
    if (!appWindow) return;
    try {
      await appWindow.toggleMaximize();
      maximized = await appWindow.isMaximized();
    } catch (error) {
      console.error("failed to toggle window size", error);
    }
  }

  async function closeWindow() {
    if (!appWindow) return;
    try {
      await appWindow.close();
    } catch (error) {
      console.error("failed to close window", error);
    }
  }

</script>

<svelte:window onpointerdown={onWindowPointerDown} onkeydown={onWindowKeyDown} />

<header class="titlebar" data-tauri-drag-region aria-label="Application title bar">
  <div class="tab-actions-wrap" bind:this={tabMenuEl}>
    <button
      class="tab-actions"
      class:open={tabMenuOpen}
      aria-label="Show open tabs"
      aria-haspopup="menu"
      aria-expanded={tabMenuOpen}
      title="Open tabs"
      onclick={() => tabMenuOpen = !tabMenuOpen}
    ><Icon name="chevron-down" size={16} /></button>

    {#if tabMenuOpen}
      <div class="tab-menu" role="menu" aria-label="Open tabs">
        <div class="menu-heading">Open tabs</div>
        {#each tabs.list as tab (tab.id)}
          <button
            class="menu-tab"
            class:active={tab.id === tabs.activeId}
            role="menuitem"
            title={tab.path ?? tab.title}
            onclick={() => activateFromMenu(tab.id)}
          >
            <span class="menu-icon" aria-hidden="true">
              <Icon name={tab.kind === "home" ? "home" : tab.kind === "settings" ? "settings" : "file"} size={15} />
            </span>
            <span class="menu-title">{tab.dirty ? "• " : ""}{tab.title}</span>
            {#if tab.id === tabs.activeId}<span class="active-dot" aria-hidden="true"></span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="tabs" bind:this={tablistEl} role="tablist" aria-label="Open tabs" data-tauri-drag-region>
    {#each tabs.list as tab, i (tab.id)}
      <div
        class="tab-shell"
        class:active-shell={tab.id === tabs.activeId}
        class:closable={tabs.list.length > 1 || tab.kind !== "home"}
        class:drag-over={dragOver === i}
        role="presentation"
        draggable="true"
        ondragstart={(e) => onDragStart(e, i)}
        ondragover={(e) => onDragOver(e, i)}
        ondrop={(e) => onDrop(e, i)}
        ondragend={onDragEnd}
        oncontextmenu={(e) => onTabContextMenu(e, tab)}
      >
        <button
          class="tab-main"
          class:active={tab.id === tabs.activeId}
          id={`tab-${tab.id}`}
          role="tab"
          tabindex={tab.id === tabs.activeId ? 0 : -1}
          aria-selected={tab.id === tabs.activeId}
          aria-controls={`panel-${tab.id}`}
          title={tab.path ?? tab.title}
          onclick={() => tabs.activate(tab.id)}
          onmousedown={(e) => onMouseDown(e, tab.id)}
          onkeydown={(e) => onTabKeyDown(e, i, tab)}
        >
          <span class="tab-icon" aria-hidden="true">
            <Icon name={tab.kind === "home" ? "home" : tab.kind === "settings" ? "settings" : "file"} size={15} />
          </span>
          <span class="title" title={tab.path ?? tab.title}>{tab.dirty ? "• " : ""}{tab.title}</span>
        </button>
        {#if tabs.list.length > 1 || tab.kind !== "home"}
          <button
            class="close-tab"
            tabindex={tab.id === tabs.activeId ? 0 : -1}
            aria-label="Close {tab.title} tab"
            onclick={() => closeTab(tab.id)}
          ><Icon name="close" size={14} strokeWidth={1.7} /></button>
        {/if}
      </div>
    {/each}
  </div>

  <button class="new-tab" aria-label="New tab" title="New tab (Ctrl+T)" onclick={() => tabs.openHome()}>
    <Icon name="plus" size={17} />
  </button>

  <div class="window-drag" data-tauri-drag-region aria-hidden="true"></div>

  <button class="settings-btn" aria-label="Settings" title="Settings" onclick={() => tabs.openSettings()}>
    <Icon name="settings" size={17} />
  </button>

  <div class="window-controls" aria-label="Window controls">
    <button class="caption-button" aria-label="Minimize" title="Minimize" onclick={minimizeWindow}>
      <Icon name="minus" size={15} strokeWidth={1.5} />
    </button>
    <button
      class="caption-button"
      aria-label={maximized ? "Restore" : "Maximize"}
      title={maximized ? "Restore" : "Maximize"}
      onclick={toggleMaximizeWindow}
    >
      <Icon name={maximized ? "restore" : "maximize"} size={14} strokeWidth={1.45} />
    </button>
    <button class="caption-button close-window" aria-label="Close" title="Close" onclick={closeWindow}>
      <Icon name="close" size={15} strokeWidth={1.55} />
    </button>
  </div>

  {#if tabContextMenu}
    <div
      class="tab-context-menu"
      bind:this={tabContextMenuEl}
      style:left="{tabContextMenu.x}px"
      style:top="{tabContextMenu.y}px"
      role="menu"
      aria-label="Tab actions for {tabContextMenu.tab.title}"
    >
      <button bind:this={tabContextMenuButton} role="menuitem" onclick={copyTabPath}>Copy path</button>
    </div>
  {/if}
  <span class="sr-only" aria-live="polite">{clipboardAnnouncement}</span>
</header>

<style>
  .titlebar {
    position: relative; z-index: 200;
    display: flex; align-items: stretch; flex: 0 0 var(--chrome-h);
    width: 100%; height: var(--chrome-h); min-width: 0;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-chrome); user-select: none;
  }
  .tab-actions-wrap {
    position: relative; z-index: 3; display: flex; align-items: flex-end;
    flex: 0 0 38px; width: 38px;
  }
  .tab-actions, .new-tab, .settings-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 32px; height: 32px; padding: 0; flex: 0 0 32px;
    border: 0; border-radius: 9px; background: transparent;
    color: var(--fg-muted); cursor: pointer;
    transition: color 100ms ease, background 100ms ease;
  }
  .tab-actions:hover, .tab-actions.open, .new-tab:hover, .settings-btn:hover {
    color: var(--fg); background: var(--control-hover);
  }
  .tab-menu {
    position: absolute; top: calc(var(--chrome-h) - 5px); left: 0; width: min(280px, calc(100vw - 20px));
    max-height: min(420px, calc(100vh - 60px)); overflow: auto;
    padding: 7px; border: 1px solid var(--border); border-radius: 12px;
    background: color-mix(in srgb, var(--bg-elev) 96%, transparent);
    box-shadow: var(--shadow-lg); backdrop-filter: blur(18px);
  }
  .menu-heading {
    padding: 5px 9px 7px; color: var(--fg-muted);
    font-size: 11px; font-weight: 600; letter-spacing: .04em; text-transform: uppercase;
  }
  .menu-tab {
    width: 100%; height: 34px; display: flex; align-items: center; gap: 9px;
    padding: 0 9px; border: 0; border-radius: 7px; background: transparent;
    color: var(--fg-muted); cursor: pointer; text-align: left;
  }
  .menu-tab:hover, .menu-tab.active { background: var(--control-hover); color: var(--fg); }
  .menu-icon { display: inline-flex; flex: 0 0 auto; }
  .menu-title { min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .active-dot { width: 5px; height: 5px; flex: 0 0 5px; border-radius: 50%; background: var(--accent); }

  .tab-context-menu {
    position: fixed; z-index: 400; min-width: 188px; padding: 4px;
    border: 1px solid var(--border); border-radius: 9px;
    background: var(--bg-elev); box-shadow: var(--shadow-lg);
  }
  .tab-context-menu button {
    display: block; width: 100%; padding: 7px 10px;
    border: 0; border-radius: 6px; background: transparent;
    color: var(--fg); cursor: pointer; font: inherit; font-size: 12px; text-align: left;
  }
  .tab-context-menu button:hover,
  .tab-context-menu button:focus-visible { background: var(--control-hover); }
  .sr-only {
    position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
    overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0;
  }

  .tabs {
    display: flex; align-items: flex-end; flex: 0 1 auto;
    min-width: 0; height: 100%;
    padding-top: 8px; overflow-x: auto; overflow-y: hidden;
    scrollbar-width: none; overscroll-behavior-x: contain;
    touch-action: pan-x; -webkit-overflow-scrolling: touch;
  }
  .tabs::-webkit-scrollbar { display: none; }
  .tab-shell {
    position: relative; display: flex; align-items: center;
    width: 240px; min-width: 96px; max-width: 240px; height: 32px;
    flex: 1 1 240px; container-type: inline-size; touch-action: pan-x;
  }
  .tab-shell::before {
    content: ""; position: absolute; z-index: 0; left: 0; top: 8px;
    width: 1px; height: 16px; background: var(--border); opacity: .72;
    transition: opacity 100ms ease;
  }
  .tab-shell:first-child::before,
  .tab-shell.active-shell::before,
  .tab-shell.active-shell + .tab-shell::before,
  .tab-shell:hover::before,
  .tab-shell:hover + .tab-shell::before { opacity: 0; }
  .tab-main {
    position: relative; z-index: 1; display: inline-flex; align-items: center;
    width: 100%; height: 32px; min-width: 0; flex: 1; gap: 8px;
    padding: 0 12px; border: 1px solid transparent; border-bottom: 0;
    border-radius: 9px 9px 0 0; background: transparent;
    color: var(--fg-muted); cursor: pointer; outline: none; font: inherit;
    touch-action: pan-x;
    transition: background 100ms ease, border-color 100ms ease, color 100ms ease;
  }
  .tab-shell.closable .tab-main { padding-right: 34px; }
  .tab-main.active {
    border-color: var(--border-subtle); background: var(--tab-active-bg); color: var(--fg);
    box-shadow: 0 -1px 2px rgba(18, 23, 33, .035);
  }
  .tab-shell:hover .tab-main:not(.active) { background: var(--control-hover); color: var(--fg); }
  .tab-main:focus-visible { box-shadow: inset 0 0 0 2px var(--accent); }
  .tab-shell.drag-over { box-shadow: inset 2px 0 var(--accent); }
  .tab-icon { display: inline-flex; flex-shrink: 0; color: var(--fg-muted); }
  .title {
    min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; text-align: left; font-size: 12px;
  }
  .close-tab {
    position: absolute; z-index: 2; right: 3px; top: 2px;
    display: flex; align-items: center; justify-content: center;
    width: 28px; height: 28px; padding: 0; flex-shrink: 0;
    border: 0; border-radius: 7px; background: transparent;
    color: inherit; opacity: 0; pointer-events: none; cursor: pointer;
    touch-action: manipulation;
  }
  .tab-shell:hover .close-tab, .tab-main.active + .close-tab, .close-tab:focus-visible {
    opacity: .62; pointer-events: auto;
  }
  .close-tab:hover { background: var(--control-hover); opacity: 1 !important; }

  .new-tab, .settings-btn {
    align-self: center; margin: 0 3px;
    transform: translateY(4px);
  }
  .window-drag {
    min-width: 64px; flex: 1 1 120px;
    touch-action: none;
  }
  .window-controls { display: flex; align-items: stretch; flex: 0 0 auto; height: var(--chrome-h); margin-left: 2px; }
  .caption-button {
    display: inline-grid; place-items: center;
    width: 46px; height: var(--chrome-h); padding: 0;
    border: 0; border-radius: 0; background: transparent;
    color: var(--fg); cursor: default;
    transition: color 80ms ease, background 80ms ease;
  }
  .caption-button:hover { background: var(--control-hover); }
  .caption-button:active { background: color-mix(in srgb, var(--control-hover) 72%, var(--fg) 8%); }
  .close-window:hover, .close-window:active { color: #fff; background: #c42b1c; }

  @container (max-width: 92px) {
    .tab-icon { display: none; }
    .tab-main { padding-left: 10px; }
  }

  @media (max-width: 760px) {
    .settings-btn { margin-left: 0; }
  }

  @media (forced-colors: active) {
    .titlebar { border-bottom-color: CanvasText; }
    .tab-main.active { border: 1px solid Highlight; border-bottom: 0; }
    .caption-button:hover, .close-window:hover { color: HighlightText; background: Highlight; }
  }
</style>
