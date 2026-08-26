<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import TabBar from "./components/TabBar.svelte";
  import Home from "./routes/Home.svelte";
  import Viewer from "./routes/Viewer.svelte";
  import Settings from "./routes/Settings.svelte";
  import { tabs } from "./stores/tabs.svelte";
  import { pendingOpenFiles } from "./lib/ipc";
  import { pickAndOpen, openPath } from "./lib/open";
  // Import theme store to run its init side-effect
  import "./stores/theme.svelte";

  async function drainPending() {
    const files = await pendingOpenFiles();
    for (const f of files) {
      await openPath(f).catch(console.error);
    }
  }

  onMount(() => {
    drainPending();

    const unlisteners: (() => void)[] = [];

    listen("files-queued", drainPending).then((u) => unlisteners.push(u));

    // Tauri 2 drag-drop event (M7: drag-drop PDFs onto window)
    // Payload in Tauri 2: { paths: string[], position: { x, y } }
    listen<{ paths: string[] }>("tauri://drag-drop", async (event) => {
      for (const path of event.payload.paths) {
        if (path.toLowerCase().endsWith(".pdf")) {
          await openPath(path).catch(console.error);
        }
      }
    }).then((u) => unlisteners.push(u));

    return () => { for (const u of unlisteners) u(); };
  });

  function onKeyDown(e: KeyboardEvent) {
    if (!e.ctrlKey) return;
    switch (e.key) {
      case "t": e.preventDefault(); tabs.openHome(); break;
      case "w": {
        e.preventDefault();
        tabs.close(tabs.activeId);
        break;
      }
      case "o": e.preventDefault(); pickAndOpen(); break;
      case "Tab": {
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

<div class="shell">
  <TabBar />
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
</style>
