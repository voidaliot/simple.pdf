<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import TabBar from "./components/TabBar.svelte";
  import Home from "./routes/Home.svelte";
  import Viewer from "./routes/Viewer.svelte";
  import Settings from "./routes/Settings.svelte";
  import TextViewer from "./routes/TextViewer.svelte";
  import { tabs } from "./stores/tabs.svelte";
  import { pendingOpenFiles } from "./lib/ipc";
  import { pickAndOpen, openPath } from "./lib/open";
  import { documentFormat } from "./lib/documentTypes";
  import { notifications } from "./stores/notifications.svelte";
  import { discardPrompt } from "./stores/discard.svelte";
  import DiscardDialog from "./components/DiscardDialog.svelte";
  // Import theme store to run its init side-effect
  import "./stores/theme.svelte";

  async function drainPending() {
    const files = await pendingOpenFiles();
    for (const f of files) {
      await openPath(f).catch(console.error);
    }
  }

  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    const unlisteners: (() => void)[] = [];
    let draining = Promise.resolve();
    const drain = () => {
      draining = draining.then(() => disposed ? undefined : drainPending()).catch(notifications.error);
    };
    const register = (unlisten: () => void) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    };
    let closingWindow = false;
    const appWindow = getCurrentWindow();
    void appWindow.onCloseRequested(async (event) => {
      event.preventDefault();
      if (closingWindow) return;
      closingWindow = true;
      try {
        if (tabs.list.some((tab) => tab.dirty) && !await discardPrompt.request("Close the window and discard all unsaved document changes?")) return;
        await appWindow.destroy();
      } catch (error) { notifications.error(error); }
      finally { closingWindow = false; }
    }).then(register).catch(notifications.error);
    // Subscribe before draining, so second-instance opens cannot get stranded.
    void listen("files-queued", drain).then((unlisten) => {
      register(unlisten);
      drain();
    }).catch(notifications.error);

    void listen<{ paths: string[] }>("tauri://drag-drop", async (event) => {
      for (const path of event.payload.paths) {
        if (documentFormat(path)) {
          await openPath(path).catch(console.error);
        }
      }
    }).then(register).catch(notifications.error);

    return () => { disposed = true; for (const u of unlisteners) u(); };
  });

  function onKeyDown(e: KeyboardEvent) {
    if (discardPrompt.current) return;
    if (!e.ctrlKey) return;
    switch (e.key.toLowerCase()) {
      case "t": e.preventDefault(); tabs.openHome(); break;
      case "w": {
        e.preventDefault();
        void tabs.close(tabs.activeId).catch(notifications.error);
        break;
      }
      case "o": e.preventDefault(); pickAndOpen(); break;
      case "tab": {
        e.preventDefault();
        const list = tabs.list;
        const idx = list.findIndex((t) => t.id === tabs.activeId);
        const next = list[(idx + (e.shiftKey ? -1 : 1) + list.length) % list.length];
        if (next) tabs.activate(next.id);
        break;
      }
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />
<DiscardDialog />

<div class="shell">
  <TabBar />
  {#if notifications.message}
    <div class="notification" role="alert">
      <span>{notifications.message}</span>
      <button onclick={notifications.clear} aria-label="Dismiss notification">Dismiss</button>
    </div>
  {/if}
  <main class="content">
    <div
      class="tab-panel"
      id={tabs.active ? `panel-${tabs.active.id}` : undefined}
      role="tabpanel"
      aria-labelledby={tabs.active ? `tab-${tabs.active.id}` : undefined}
    >
      {#if tabs.active}
        {#if tabs.active.kind === "home"}
          <Home />
        {:else if tabs.active.kind === "settings"}
          <Settings />
        {:else if tabs.active.kind === "text"}
          {#key tabs.active.id}
            <TextViewer tab={tabs.active} />
          {/key}
        {:else}
          {#key tabs.active.id}
            <Viewer tab={tabs.active} />
          {/key}
        {/if}
      {/if}
    </div>
  </main>
</div>

<style>
  .shell { display: flex; flex-direction: column; height: 100%; background: var(--bg); }
  .content { flex: 1; overflow: hidden; background: var(--bg); }
  .tab-panel { height: 100%; overflow: hidden; }
  .notification { display: flex; gap: 16px; align-items: center; justify-content: space-between; padding: 10px 16px; background: var(--bg-elev); border-bottom: 1px solid var(--danger); }
  .notification span { overflow-wrap: anywhere; }
  .notification button { background: var(--control-bg); border: 1px solid var(--border); border-radius: var(--radius); padding: 5px 10px; }
</style>
