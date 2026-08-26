<script lang="ts">
  import { signatures, type SavedSignature } from "../stores/signatures.svelte";

  interface Props {
    onClose: () => void;
    onPlace: (paths: [number, number][][]) => Promise<void>;
  }

  let { onClose, onPlace }: Props = $props();

  type Tab = "draw" | "saved";
  let activeTab = $state<Tab>(signatures.list.length > 0 ? "saved" : "draw");

  // ── Draw tab ──────────────────────────────────────────────────────────────────
  let canvas: HTMLCanvasElement | undefined = $state();
  let drawing = $state(false);
  let currentPath = $state<[number, number][]>([]);
  let allPaths = $state<[number, number][][]>([]);
  let placing = $state(false);
  let saveAfterPlace = $state(false);
  let placeError = $state("");

  function getCtx() { return canvas?.getContext("2d") ?? null; }

  function normPos(e: PointerEvent) {
    if (!canvas) return { nx: 0, ny: 0 };
    const r = canvas.getBoundingClientRect();
    return { nx: (e.clientX - r.left) / r.width, ny: (e.clientY - r.top) / r.height };
  }

  function redraw() {
    const ctx = getCtx();
    if (!ctx || !canvas) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = "#1a1a1a";
    ctx.lineWidth = 2;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const path of [...allPaths, currentPath]) {
      if (path.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(path[0]![0] * canvas.width, path[0]![1] * canvas.height);
      for (let i = 1; i < path.length; i++) {
        ctx.lineTo(path[i]![0] * canvas.width, path[i]![1] * canvas.height);
      }
      ctx.stroke();
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (!canvas) return;
    drawing = true;
    canvas.setPointerCapture(e.pointerId);
    const { nx, ny } = normPos(e);
    currentPath = [[nx, ny]];
  }

  function onPointerMove(e: PointerEvent) {
    if (!drawing) return;
    const { nx, ny } = normPos(e);
    currentPath = [...currentPath, [nx, ny]];
    redraw();
  }

  function onPointerUp() {
    if (!drawing) return;
    drawing = false;
    if (currentPath.length > 1) allPaths = [...allPaths, currentPath];
    currentPath = [];
    redraw();
  }

  function clear() {
    allPaths = [];
    currentPath = [];
    placeError = "";
    redraw();
  }

  function makeThumbnail(paths: [number, number][][]): string {
    const c = document.createElement("canvas");
    c.width = 160;
    c.height = 64;
    const ctx = c.getContext("2d");
    if (!ctx) return "";
    ctx.strokeStyle = "#1a1a1a";
    ctx.lineWidth = 1.5;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const path of paths) {
      if (path.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(path[0]![0] * c.width, path[0]![1] * c.height);
      for (let i = 1; i < path.length; i++) {
        ctx.lineTo(path[i]![0] * c.width, path[i]![1] * c.height);
      }
      ctx.stroke();
    }
    return c.toDataURL();
  }

  async function place(paths: [number, number][][]) {
    placing = true;
    placeError = "";
    try {
      await onPlace(paths);
      if (saveAfterPlace && activeTab === "draw") {
        const thumb = makeThumbnail(paths);
        signatures.save(paths, thumb);
      }
      onClose();
    } catch (error) {
      placeError = error instanceof Error ? error.message : String(error);
    } finally {
      placing = false;
    }
  }

  async function placeDrawn() {
    if (allPaths.length === 0) return;
    await place(allPaths);
  }

  // ── Saved tab ─────────────────────────────────────────────────────────────────
  async function useSaved(sig: SavedSignature) {
    await place(sig.paths);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="backdrop" role="dialog" aria-modal="true" aria-label="Signature" tabindex="-1"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  onkeydown={() => {}}>
  <div class="modal">
    <div class="modal-header">
      <h2>Signature</h2>
      <button onclick={onClose} aria-label="Close">✕</button>
    </div>

    <!-- Tabs -->
    <div class="tabs" role="tablist">
      <button role="tab" class:active={activeTab === "draw"}
        onclick={() => activeTab = "draw"} aria-selected={activeTab === "draw"}>Draw new</button>
      <button role="tab" class:active={activeTab === "saved"}
        onclick={() => activeTab = "saved"} aria-selected={activeTab === "saved"}>
        Saved{signatures.list.length > 0 ? ` (${signatures.list.length})` : ""}
      </button>
    </div>

    {#if activeTab === "draw"}
      <p class="hint">Draw your signature below. Double-click or Clear to reset.</p>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <canvas
        bind:this={canvas}
        width={480}
        height={200}
        class="sig-canvas"
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointerleave={onPointerUp}
        ondblclick={clear}
      ></canvas>
      <label class="save-checkbox">
        <input type="checkbox" bind:checked={saveAfterPlace} />
        Save signature for reuse
      </label>
      {#if placeError}<p class="place-error" role="alert">Could not place signature: {placeError}</p>{/if}
      <div class="modal-footer">
        <button onclick={clear}>Clear</button>
        <button class="primary" onclick={placeDrawn} disabled={allPaths.length === 0 || placing}>
          {placing ? "Placing…" : "Place on page"}
        </button>
      </div>

    {:else}
      {#if signatures.list.length === 0}
        <p class="empty-state">No saved signatures yet. Draw one and check "Save signature for reuse".</p>
      {:else}
        <div class="sig-grid">
          {#each signatures.list as sig (sig.id)}
            <div class="sig-card" class:is-default={sig.isDefault}>
              {#if sig.thumbnail}
                <img src={sig.thumbnail} alt="Signature preview" class="sig-thumb" />
              {:else}
                <div class="sig-thumb sig-no-thumb">No preview</div>
              {/if}
              <div class="sig-actions">
                <button class="primary" onclick={() => useSaved(sig)} disabled={placing}>Use</button>
                <button
                  class:starred={sig.isDefault}
                  title={sig.isDefault ? "Default signature" : "Set as default"}
                  onclick={() => signatures.setDefault(sig.id)}
                  aria-label="Set as default"
                >★</button>
                <button
                  class="danger"
                  onclick={() => signatures.remove(sig.id)}
                  aria-label="Delete"
                >✕</button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
      {#if placeError}<p class="place-error" role="alert">Could not place signature: {placeError}</p>{/if}
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; background: rgba(0,0,0,0.5);
    display: flex; align-items: center; justify-content: center; z-index: 500;
  }
  .modal {
    background: var(--bg-elev); border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 24px; width: 540px;
    box-shadow: var(--shadow); max-height: 80vh; overflow-y: auto;
  }
  .modal-header {
    display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px;
  }
  .modal-header h2 { margin: 0; font-size: 18px; font-weight: 500; }
  .modal-header button {
    background: none; border: none; cursor: pointer; font-size: 18px; color: var(--fg-muted); padding: 2px;
  }
  .modal-header button:hover { color: var(--fg); }

  /* Tabs */
  .tabs { display: flex; gap: 2px; margin-bottom: 14px; border-bottom: 1px solid var(--border); }
  .tabs button {
    background: none; border: none; border-bottom: 2px solid transparent;
    padding: 6px 14px; cursor: pointer; font: inherit; font-size: 13px; color: var(--fg-muted);
    margin-bottom: -1px;
  }
  .tabs button.active { color: var(--fg); border-bottom-color: var(--accent); font-weight: 500; }

  /* Draw tab */
  .hint { font-size: 12px; color: var(--fg-muted); margin: 0 0 12px; }
  .sig-canvas {
    display: block; width: 100%; border: 2px dashed var(--border);
    border-radius: var(--radius); background: white; cursor: crosshair;
    touch-action: none;
  }
  .save-checkbox {
    display: flex; align-items: center; gap: 6px;
    font-size: 13px; color: var(--fg-muted); margin: 10px 0 0;
    cursor: pointer;
  }
  .place-error { margin: 10px 0 0; color: var(--danger, #c00); font-size: 12px; }
  .modal-footer { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
  .modal-footer button {
    padding: 8px 16px; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--bg-elev);
    cursor: pointer; font: inherit; font-size: 13px;
  }
  .modal-footer button:disabled { opacity: 0.5; cursor: not-allowed; }
  .modal-footer .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }

  /* Saved tab */
  .empty-state { text-align: center; padding: 32px 0; color: var(--fg-muted); font-size: 13px; }
  .sig-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; padding: 4px 0; }
  .sig-card {
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg); padding: 8px; display: flex; flex-direction: column; gap: 8px;
  }
  .sig-card.is-default { border-color: var(--accent); }
  .sig-thumb {
    display: block; width: 100%; height: 64px; object-fit: contain;
    background: white; border-radius: 2px;
  }
  .sig-no-thumb {
    display: flex; align-items: center; justify-content: center;
    height: 64px; color: var(--fg-muted); font-size: 12px;
  }
  .sig-actions { display: flex; gap: 4px; }
  .sig-actions button {
    flex: 1; padding: 4px 6px; font: inherit; font-size: 12px;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg-elev); cursor: pointer; color: var(--fg);
  }
  .sig-actions button.primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); flex: 2; }
  .sig-actions button.danger { color: var(--danger, #c00); }
  .sig-actions button.starred { color: gold; }
  .sig-actions button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
