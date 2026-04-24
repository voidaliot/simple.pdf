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
      <button
        class="tab"
        class:active={tab.id === tabs.activeId}
        role="tab"
        aria-selected={tab.id === tabs.activeId}
        title={tab.path ?? tab.title}
        on:click={() => tabs.activate(tab.id)}
        on:mousedown={(e) => onMouseDown(e, tab.id)}
      >
        <span class="title">{tab.dirty ? "• " : ""}{tab.title}</span>
        {#if tabs.list.length > 1 || tab.kind !== "home"}
          <span
            class="close"
            aria-label="Close tab"
            role="button"
            tabindex="-1"
            on:click|stopPropagation={() => tabs.close(tab.id)}
            on:keydown|stopPropagation
          >×</span>
        {/if}
      </button>
    {/each}
    <button class="new-tab" aria-label="New tab" on:click={() => tabs.openHome()}>+</button>
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
    padding: 6px 10px;
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
    font-size: 16px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 4px;
    opacity: 0.6;
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
