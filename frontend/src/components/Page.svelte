<script lang="ts">
  import { pageUrl } from "../lib/ipc";

  interface Props {
    docId: string;
    pageIndex: number;
    width: number;
    height: number;
    zoom: number;
    visible: boolean;
  }

  let { docId, pageIndex, width, height, zoom, visible }: Props = $props();

  const dpr = window.devicePixelRatio ?? 1;
  const renderScale = $derived(zoom * dpr);
  const cssW = $derived(Math.round(width * zoom));
  const cssH = $derived(Math.round(height * zoom));
  const src = $derived(visible ? pageUrl(docId, pageIndex, renderScale) : "");

  let loaded = $state(false);
  let error = $state(false);

  $effect(() => {
    // Reset states when src changes
    if (src) {
      loaded = false;
      error = false;
    }
  });
</script>

<div
  class="page-wrapper"
  style:width="{cssW}px"
  style:height="{cssH}px"
  aria-label="Page {pageIndex + 1}"
  role="img"
>
  {#if visible && src}
    <img
      {src}
      alt="Page {pageIndex + 1}"
      width={cssW}
      height={cssH}
      draggable="false"
      onload={() => { loaded = true; }}
      onerror={() => { error = true; }}
    />
    {#if !loaded && !error}
      <div class="skeleton" aria-hidden="true"></div>
    {/if}
    {#if error}
      <div class="error-overlay" aria-label="Failed to render page {pageIndex + 1}">
        <span>⚠ render failed</span>
      </div>
    {/if}
  {:else}
    <div class="skeleton" aria-hidden="true"></div>
  {/if}
</div>

<style>
  .page-wrapper {
    position: relative;
    background: white;
    box-shadow: var(--shadow);
    border-radius: 2px;
    overflow: hidden;
    flex-shrink: 0;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: fill;
    user-select: none;
  }
  .skeleton {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      var(--bg-elev) 25%,
      var(--border) 50%,
      var(--bg-elev) 75%
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s ease infinite;
  }
  @keyframes shimmer {
    0% { background-position: 200% center; }
    100% { background-position: -200% center; }
  }
  .error-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elev);
    color: var(--danger);
    font-size: 13px;
  }
</style>
