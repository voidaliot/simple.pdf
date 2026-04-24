<script lang="ts">
  import type { Tab } from "../stores/tabs.svelte";

  interface Props {
    tab: Tab;
  }

  let { tab }: Props = $props();
</script>

<section class="viewer">
  <div class="toolbar">
    <span class="path">{tab.path ?? ""}</span>
    <span class="pages">{tab.pageCount ?? 0} page{tab.pageCount === 1 ? "" : "s"}</span>
  </div>
  <div class="pages-area">
    <div class="placeholder">
      <p>Page rendering lands in M1.</p>
      <p class="hint">Doc ID: <code>{tab.docId ?? "—"}</code></p>
    </div>
  </div>
</section>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-elev);
    font-size: 12px;
    color: var(--fg-muted);
  }
  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pages-area {
    flex: 1;
    overflow: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg);
  }
  .placeholder {
    text-align: center;
    color: var(--fg-muted);
  }
  .placeholder code {
    background: var(--bg-elev);
    padding: 2px 6px;
    border-radius: 4px;
    border: 1px solid var(--border);
  }
  .hint { font-size: 12px; opacity: 0.7; }
</style>
