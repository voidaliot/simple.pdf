<script lang="ts">
  import { tabs } from "../stores/tabs.svelte";

  function onMouseDown(e: MouseEvent, id: string) {
    if (e.button === 1) {
      e.preventDefault();
      tabs.close(id);
    }
  }
</script>

<div class="tabbar" role="tablist">
  <div class="tabs">
    {#each tabs.list as tab (tab.id)}
      <!-- Use div+role=tab to avoid nested <button> (invalid HTML) -->
      <div
        class="tab"
        class:active={tab.id === tabs.activeId}
        role="tab"
        tabindex="0"
        aria-selected={tab.id === tabs.activeId}
        title={tab.path ?? tab.title}
        onclick={() => tabs.activate(tab.id)}
        onmousedown={(e) => onMouseDown(e, tab.id)}
        onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") tabs.activate(tab.id); }}
      >
        <span class="title">{tab.dirty ? "• " : ""}{tab.title}</span>
        {#if tabs.list.length > 1 || tab.kind !== "home"}
          <button
            class="close"
            aria-label="Close {tab.title} tab"
            tabindex="-1"
            onclick={(e) => { e.stopPropagation(); tabs.close(tab.id); }}
          >×</button>
        {/if}
      </div>
    {/each}
    <button class="new-tab" aria-label="New tab" onclick={() => tabs.openHome()}>+</button>
  </div>
  <div class="drag-region" data-tauri-drag-region></div>
</div>

<style>
  .tabbar {
    display: flex;
    align-items: flex-end;
    height: var(--chrome-h);
    background: var(--bg-chrome);
    border-bottom: 1px solid var(--border);
    padding: 6px 8px 0 8px;
    gap: 4px;
    user-select: none;
  }
  .tabs {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    overflow-x: auto;
    scrollbar-width: none;
    max-width: calc(100% - 100px);
  }
  .tabs::-webkit-scrollbar { display: none; }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: var(--tab-inactive-bg);
    border: 1px solid var(--border);
    border-bottom: none;
    border-radius: 8px 8px 0 0;
    padding: 6px 8px 6px 12px;
    max-width: 220px;
    min-width: 120px;
    cursor: pointer;
    color: var(--fg-muted);
    transition: background 120ms ease, color 120ms ease;
  }
  .tab.active {
    background: var(--tab-active-bg);
    color: var(--fg);
    box-shadow: var(--shadow-sm);
  }
  .tab:hover { color: var(--fg); }
  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  .close {
    font-size: 15px;
    line-height: 1;
    width: 20px;
    height: 20px;
    border: none;
    background: transparent;
    border-radius: 4px;
    padding: 0;
    cursor: pointer;
    color: inherit;
    opacity: 0.5;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .close:hover { background: rgba(127,127,127,0.2); opacity: 1; }
  .new-tab {
    background: transparent;
    border: none;
    font-size: 18px;
    width: 28px;
    height: 28px;
    border-radius: 4px;
    cursor: pointer;
    color: var(--fg-muted);
    margin-bottom: 4px;
  }
  .new-tab:hover { background: rgba(127,127,127,0.15); color: var(--fg); }
  .drag-region { flex: 1; align-self: stretch; }
</style>
