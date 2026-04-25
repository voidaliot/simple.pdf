<script lang="ts">
  import { onMount, untrack } from "svelte";
  import type { Tab } from "../stores/tabs.svelte";
  import { createViewerStore } from "../stores/viewer.svelte";
  import { getPageSizes } from "../lib/ipc";
  import Page from "../components/Page.svelte";

  interface Props {
    tab: Tab;
  }

  let { tab }: Props = $props();

  const docId = $derived(tab.docId ?? "");
  // untrack: this store is created once per mount; docId won't change for this instance
  const vstore = untrack(() => createViewerStore(docId));

  let container: HTMLElement | undefined = $state();
  let visibleSet = $state(new Set<number>());
  let loadingPages = $state(false);

  // Load page sizes once on mount
  onMount(() => {
    loadingPages = true;
    getPageSizes(docId).then((sizes) => {
      vstore.setPageSizes(sizes);
    }).catch((e: unknown) => {
      console.error("failed to get page sizes", e);
    }).finally(() => {
      loadingPages = false;
    });
  });

  // ResizeObserver for container width
  $effect(() => {
    if (!container) return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry) vstore.setContainerWidth(entry.contentRect.width);
    });
    ro.observe(container);
    vstore.setContainerWidth(container.clientWidth);
    return () => ro.disconnect();
  });

  // IntersectionObserver for page visibility (re-run when pageSizes change)
  $effect(() => {
    const pages = vstore.pageSizes;
    if (!pages.length || !container) return;

    const obs = new IntersectionObserver(
      (entries) => {
        const next = new Set(visibleSet);
        for (const entry of entries) {
          const idx = Number((entry.target as HTMLElement).dataset.pageIndex);
          if (entry.isIntersecting) {
            next.add(idx);
            if (idx > 0) next.add(idx - 1);
            if (idx < pages.length - 1) next.add(idx + 1);
          }
        }
        visibleSet = next;
        if (next.size > 0) {
          vstore.setCurrentPage(Math.min(...next));
        }
      },
      { root: container, rootMargin: "200px 0px", threshold: 0.01 }
    );

    // Give Svelte time to render the page elements
    requestAnimationFrame(() => {
      container?.querySelectorAll<HTMLElement>("[data-page-index]").forEach((el) => obs.observe(el));
    });

    return () => obs.disconnect();
  });

  function onKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey) {
      if (e.key === "=" || e.key === "+") { e.preventDefault(); vstore.setZoom(vstore.effectiveZoom * 1.1); }
      if (e.key === "-") { e.preventDefault(); vstore.setZoom(vstore.effectiveZoom * 0.9); }
      if (e.key === "0") { e.preventDefault(); vstore.setZoomMode("fit-width"); }
    }
  }

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    vstore.setZoom(vstore.effectiveZoom * (e.deltaY < 0 ? 1.1 : 0.9));
  }

  function onPageInput(e: Event) {
    const val = parseInt((e.target as HTMLInputElement).value, 10) - 1;
    if (val >= 0 && val < vstore.pageSizes.length) {
      container?.querySelector<HTMLElement>(`[data-page-index="${val}"]`)
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }

  const zoomPct = $derived(Math.round(vstore.effectiveZoom * 100));
</script>

<svelte:window onkeydown={onKeyDown} />

<section class="viewer" aria-label="PDF viewer">
  <!-- Toolbar -->
  <div class="toolbar">
    <div class="toolbar-left">
      <span class="doc-title" title={tab.path}>{tab.title}</span>
    </div>

    <div class="toolbar-center">
      {#if vstore.pageSizes.length > 0}
        <span class="page-nav">
          <label>
            Page
            <input
              type="number"
              min="1"
              max={vstore.pageSizes.length}
              value={vstore.currentPage + 1}
              onchange={onPageInput}
              aria-label="Current page"
            />
          </label>
          <span class="page-total">/ {vstore.pageSizes.length}</span>
        </span>
      {/if}
    </div>

    <div class="toolbar-right">
      <button onclick={() => vstore.setZoom(vstore.effectiveZoom * 0.8)} aria-label="Zoom out" title="Zoom out (Ctrl+-)">−</button>
      <span class="zoom-pct">{zoomPct}%</span>
      <button onclick={() => vstore.setZoom(vstore.effectiveZoom * 1.2)} aria-label="Zoom in" title="Zoom in (Ctrl++)">+</button>
      <button
        class:active={vstore.zoomMode === "fit-width"}
        onclick={() => vstore.setZoomMode("fit-width")}
        title="Fit width (Ctrl+0)"
      >⟺</button>
      <button
        class:active={vstore.zoomMode === "fit-page"}
        onclick={() => vstore.setZoomMode("fit-page")}
        title="Fit page"
      >□</button>
    </div>
  </div>

  <!-- Scrollable pages area -->
  <div
    class="pages-area"
    bind:this={container}
    onwheel={onWheel}
    role="document"
    tabindex="-1"
  >
    {#if loadingPages}
      <div class="center-msg">Loading…</div>
    {:else if vstore.pageSizes.length === 0}
      <div class="center-msg">No pages found.</div>
    {:else}
      <div class="pages-list">
        {#each vstore.pageSizes as size, i}
          <div class="page-entry" data-page-index={i}>
            <Page
              docId={docId}
              pageIndex={i}
              width={size.width}
              height={size.height}
              zoom={vstore.effectiveZoom}
              visible={visibleSet.has(i)}
            />
            <span class="page-label" aria-hidden="true">{i + 1}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }
  .toolbar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--bg-elev);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .toolbar-left { overflow: hidden; }
  .toolbar-center { display: flex; align-items: center; justify-content: center; }
  .toolbar-right { display: flex; align-items: center; gap: 4px; justify-content: flex-end; }
  .doc-title {
    font-size: 13px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }
  .page-nav { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--fg-muted); }
  .page-nav label { display: flex; align-items: center; gap: 4px; }
  .page-nav input {
    width: 44px;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    text-align: center;
    font: inherit;
    font-size: 13px;
  }
  .toolbar-right button {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 4px 8px;
    cursor: pointer;
    font-size: 14px;
    color: var(--fg-muted);
    line-height: 1;
    min-width: 28px;
  }
  .toolbar-right button:hover { background: var(--bg-chrome); color: var(--fg); }
  .toolbar-right button.active { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .zoom-pct { font-size: 13px; color: var(--fg-muted); min-width: 40px; text-align: center; }
  .pages-area {
    flex: 1;
    overflow: auto;
    background: var(--bg);
    outline: none;
  }
  .pages-list {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 24px 24px 48px;
  }
  .page-entry {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }
  .page-label { font-size: 11px; color: var(--fg-muted); opacity: 0.6; user-select: none; }
  .center-msg {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 200px;
    color: var(--fg-muted);
    font-size: 14px;
  }
</style>
