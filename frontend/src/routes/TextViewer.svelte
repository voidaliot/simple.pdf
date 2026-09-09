<script lang="ts">
  import { onDestroy } from "svelte";
  import TextContent from "../components/TextContent.svelte";
  import { tabs, type Tab } from "../stores/tabs.svelte";
  import { openTextDocument } from "../lib/ipc";
  import { notifications } from "../stores/notifications.svelte";

  let { tab }: { tab: Tab } = $props();
  let sourceVisible = $state(false);
  let reloading = $state(false);
  let disposed = false;
  onDestroy(() => { disposed = true; });

  async function reload() {
    if (reloading || !tab.path) return;
    reloading = true;
    try {
      const document = await openTextDocument(tab.path);
      if (!disposed) tabs.updateText(tab.id, document);
    } catch (error) { if (!disposed) notifications.error(error); }
    finally { if (!disposed) reloading = false; }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === "F5" || (event.ctrlKey && event.key.toLowerCase() === "r")) {
      event.preventDefault();
      void reload();
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="text-viewer">
  <div class="text-toolbar">
    <span class="document-type">{tab.textDocument?.format}</span>
    <span class="document-path" title={tab.path}>{tab.path}</span>
    <button onclick={() => sourceVisible = !sourceVisible} aria-pressed={sourceVisible}>{sourceVisible ? "Preview" : "View source"}</button>
    <button onclick={reload} disabled={reloading} title="Reload from disk (F5)">{reloading ? "Reloading…" : "Reload"}</button>
  </div>
  <div class="text-scroll">
    {#if tab.textDocument}
      {#if sourceVisible}
        <pre class="document-source"><code>{tab.textDocument.source}</code></pre>
      {:else}
        {#key tab.textDocument.source}
          <TextContent document={tab.textDocument} />
        {/key}
      {/if}
    {/if}
  </div>
</div>

<style>
  .text-viewer { display: flex; flex-direction: column; height: 100%; }
  .text-toolbar { display: flex; align-items: center; gap: 10px; padding: 9px 16px; border-bottom: 1px solid var(--border); background: var(--bg-elev); }
  .document-type { text-transform: uppercase; font-size: 11px; letter-spacing: 0.06em; color: var(--fg-muted); }
  .document-path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg-muted); font-size: 12px; }
  button { white-space: nowrap; border: 1px solid var(--border); background: var(--control-bg); padding: 6px 10px; border-radius: var(--radius); cursor: pointer; }
  button:hover { background: var(--control-hover); }
  button:disabled { opacity: 0.5; }
  .text-scroll { flex: 1; min-height: 0; overflow: auto; }
  .document-source { white-space: pre-wrap; overflow-wrap: anywhere; padding: 24px 36px; tab-size: 2; font-family: Consolas, monospace; line-height: 1.6; }
</style>
