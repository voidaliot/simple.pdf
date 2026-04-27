<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import type { Tab } from "../stores/tabs.svelte";
  import { tabs } from "../stores/tabs.svelte";
  import { createViewerStore } from "../stores/viewer.svelte";
  import {
    getPageSizes,
    getPageTextSpans,
    getPageAnnotations,
    addHighlightAnnotation,
    addUnderlineAnnotation,
    addStrikeoutAnnotation,
    addTextAnnotation,
    addInkAnnotation,
    removeAnnotation,
    undoAnnotation,
    saveDocument,
    getFormType,
    getFormFields,
    setFieldTextValue,
    setFieldChecked,
    type TextSpan,
    type Annotation,
    type AnnRect,
    type FormField,
  } from "../lib/ipc";
  import Page, { type Highlight } from "../components/Page.svelte";
  import SignatureCapture from "../components/SignatureCapture.svelte";

  interface Props { tab: Tab; }
  let { tab }: Props = $props();

  const docId = $derived(tab.docId ?? "");
  const vstore = untrack(() => createViewerStore(docId));

  let container: HTMLElement | undefined = $state();
  let visibleSet = $state(new Set<number>());
  let loadingPages = $state(false);

  // ── Text spans ──────────────────────────────────────────────────────────────
  let textSpansByPage = $state<(TextSpan[] | undefined)[]>([]);

  async function loadTextSpans(pageIndex: number) {
    if (textSpansByPage[pageIndex] !== undefined) return;
    textSpansByPage[pageIndex] = [];
    try { textSpansByPage[pageIndex] = await getPageTextSpans(docId, pageIndex); }
    catch { textSpansByPage[pageIndex] = []; }
  }

  $effect(() => { for (const idx of visibleSet) loadTextSpans(idx); });

  // ── Forms ────────────────────────────────────────────────────────────────────
  let formType = $state("none");
  let formFieldsByPage = $state<(FormField[] | undefined)[]>([]);

  onMount(async () => {
    try { formType = await getFormType(docId); } catch { formType = "none"; }
  });

  async function loadFormFields(pageIndex: number) {
    if (formType === "none" || formFieldsByPage[pageIndex] !== undefined) return;
    formFieldsByPage[pageIndex] = [];
    try { formFieldsByPage[pageIndex] = await getFormFields(docId, pageIndex); }
    catch { formFieldsByPage[pageIndex] = []; }
  }

  $effect(() => { for (const idx of visibleSet) loadFormFields(idx); });

  async function handleFieldText(pageIndex: number, annotIndex: number, value: string) {
    await setFieldTextValue(docId, pageIndex, annotIndex, value).catch(console.error);
    tabs.markDirty(tab.id, true);
  }

  async function handleFieldChecked(pageIndex: number, annotIndex: number, checked: boolean) {
    await setFieldChecked(docId, pageIndex, annotIndex, checked).catch(console.error);
    tabs.markDirty(tab.id, true);
  }

  // ── Annotations ─────────────────────────────────────────────────────────────
  let annotsByPage = $state<(Annotation[] | undefined)[]>([]);

  async function loadAnnotations(pageIndex: number) {
    try { annotsByPage[pageIndex] = await getPageAnnotations(docId, pageIndex); }
    catch { annotsByPage[pageIndex] = []; }
  }

  async function refreshAnnotations(pageIndex: number) {
    annotsByPage[pageIndex] = await getPageAnnotations(docId, pageIndex).catch(() => []);
  }

  $effect(() => { for (const idx of visibleSet) {
    if (annotsByPage[idx] === undefined) loadAnnotations(idx);
  }});

  // ── Annotation sidebar ──────────────────────────────────────────────────────
  let sidebarOpen = $state(false);

  const allAnnotations = $derived.by(() => {
    const list: { pageIndex: number; annot: Annotation }[] = [];
    annotsByPage.forEach((anns, pi) => {
      if (!anns) return;
      for (const a of anns) if (a.kind !== "widget") list.push({ pageIndex: pi, annot: a });
    });
    return list;
  });

  function scrollToAnnotation(pageIndex: number) {
    container?.querySelector<HTMLElement>(`[data-page-index="${pageIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  // ── Annotation tools ────────────────────────────────────────────────────────
  type AnnotTool = "none" | "highlight" | "underline" | "strikeout" | "text" | "ink";
  let activeTool = $state<AnnotTool>("none");
  let toolColor = $state<[number, number, number]>([255, 214, 0]);
  let inkWidth = $state(2);

  async function handlePageClick(
    e: MouseEvent,
    pageIndex: number,
    pageEl: HTMLElement,
    cssW: number,
    cssH: number,
  ) {
    if (activeTool !== "text") return;
    const rect = pageEl.getBoundingClientRect();
    const left = (e.clientX - rect.left) / cssW;
    const top = (e.clientY - rect.top) / cssH;
    const contents = prompt("Sticky note text:");
    if (!contents) return;
    await addTextAnnotation(docId, pageIndex, left, top, contents, null, toolColor);
    tabs.markDirty(tab.id, true);
    await refreshAnnotations(pageIndex);
  }

  async function handleTextSelection(
    pageIndex: number,
    selRects: AnnRect[],
  ) {
    if (activeTool === "none" || activeTool === "text" || activeTool === "ink") return;
    if (selRects.length === 0) return;

    if (activeTool === "highlight") {
      await addHighlightAnnotation(docId, pageIndex, selRects, toolColor, 0.4);
    } else if (activeTool === "underline") {
      await addUnderlineAnnotation(docId, pageIndex, selRects, toolColor);
    } else if (activeTool === "strikeout") {
      await addStrikeoutAnnotation(docId, pageIndex, selRects, toolColor);
    }
    tabs.markDirty(tab.id, true);
    await refreshAnnotations(pageIndex);
    window.getSelection()?.removeAllRanges();
  }

  async function handleInkStroke(pageIndex: number, paths: [number, number][][]) {
    await addInkAnnotation(docId, pageIndex, paths, toolColor, inkWidth);
    tabs.markDirty(tab.id, true);
    await refreshAnnotations(pageIndex);
  }

  async function handleDeleteAnnotation(pageIndex: number, annotIndex: number) {
    await removeAnnotation(docId, pageIndex, annotIndex);
    tabs.markDirty(tab.id, true);
    await refreshAnnotations(pageIndex);
  }

  // ── Signing ─────────────────────────────────────────────────────────────────
  let signOpen = $state(false);

  // ── Find-in-page ─────────────────────────────────────────────────────────────
  interface FindMatch { pageIndex: number; left: number; top: number; width: number; height: number; }

  let findOpen = $state(false);
  let findQuery = $state("");
  let findCurrentMatch = $state(0);
  let findInput: HTMLInputElement | undefined = $state();

  const findMatches = $derived.by((): FindMatch[] => {
    const q = findQuery.trim().toLowerCase();
    if (!q) return [];
    const result: FindMatch[] = [];
    for (let p = 0; p < textSpansByPage.length; p++) {
      const spans = textSpansByPage[p];
      if (!spans) continue;
      for (const span of spans) {
        if (span.text.toLowerCase().includes(q))
          result.push({ pageIndex: p, left: span.left, top: span.top, width: span.width, height: span.height });
      }
    }
    return result;
  });

  const pageHighlights = $derived.by(() => {
    const map = new Map<number, Highlight[]>();
    for (const m of findMatches)
      (map.get(m.pageIndex) ?? map.set(m.pageIndex, []).get(m.pageIndex)!).push(m);
    return map;
  });

  const activeByPage = $derived.by(() => {
    const cur = findMatches[findCurrentMatch];
    if (!cur) return new Map<number, number>();
    let idx = 0;
    for (const m of findMatches.slice(0, findCurrentMatch + 1))
      if (m.pageIndex === cur.pageIndex) idx++;
    return new Map([[cur.pageIndex, idx - 1]]);
  });

  $effect(() => { findQuery; findCurrentMatch = 0; });

  async function openFindBar() {
    findOpen = true;
    for (let i = 0; i < vstore.pageSizes.length; i++) loadTextSpans(i);
    await tick();
    findInput?.focus();
    findInput?.select();
  }

  function closeFindBar() { findOpen = false; findQuery = ""; }

  function nextMatch() {
    if (!findMatches.length) return;
    findCurrentMatch = (findCurrentMatch + 1) % findMatches.length;
    const m = findMatches[findCurrentMatch];
    if (m) container?.querySelector<HTMLElement>(`[data-page-index="${m.pageIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  function prevMatch() {
    if (!findMatches.length) return;
    findCurrentMatch = (findCurrentMatch - 1 + findMatches.length) % findMatches.length;
    const m = findMatches[findCurrentMatch];
    if (m) container?.querySelector<HTMLElement>(`[data-page-index="${m.pageIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  // ── Page loading ─────────────────────────────────────────────────────────────
  onMount(() => {
    loadingPages = true;
    getPageSizes(docId)
      .then((sizes) => { vstore.setPageSizes(sizes); })
      .catch((e: unknown) => { console.error("failed to get page sizes", e); })
      .finally(() => { loadingPages = false; });
  });

  $effect(() => {
    if (!container) return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry) vstore.setContainerWidth(entry.contentRect.width);
    });
    ro.observe(container);
    vstore.setContainerWidth(container.clientWidth);
    return () => ro.disconnect();
  });

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
            for (let d = 1; d <= 2; d++) {
              if (idx - d >= 0) next.add(idx - d);
              if (idx + d < pages.length) next.add(idx + d);
            }
          }
        }
        visibleSet = next;
        if (next.size > 0) vstore.setCurrentPage(Math.min(...next));
      },
      { root: container, rootMargin: "1200px 0px", threshold: 0.01 }
    );
    requestAnimationFrame(() => {
      container?.querySelectorAll<HTMLElement>("[data-page-index]").forEach((el) => obs.observe(el));
    });
    return () => obs.disconnect();
  });

  // ── Keyboard ──────────────────────────────────────────────────────────────────
  async function onKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey) {
      if (e.key === "f") { e.preventDefault(); openFindBar(); return; }
      if (e.key === "s") { e.preventDefault(); await handleSave(); return; }
      if (e.key === "z") { e.preventDefault(); await handleUndo(); return; }
      if (e.key === "=" || e.key === "+") { e.preventDefault(); vstore.setZoom(vstore.effectiveZoom * 1.1); return; }
      if (e.key === "-") { e.preventDefault(); vstore.setZoom(vstore.effectiveZoom * 0.9); return; }
      if (e.key === "0") { e.preventDefault(); vstore.setZoomMode("fit-width"); return; }
    }
    if (e.key === "Escape") {
      if (findOpen) { e.preventDefault(); closeFindBar(); }
      else if (activeTool !== "none") { activeTool = "none"; }
    }
  }

  function onFindKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); e.shiftKey ? prevMatch() : nextMatch(); }
    else if (e.key === "Escape") { e.preventDefault(); closeFindBar(); }
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

  async function handleSave() {
    await saveDocument(docId);
    tabs.markDirty(tab.id, false);
  }

  async function handleUndo() {
    const didUndo = await undoAnnotation(docId);
    if (didUndo) {
      for (const idx of visibleSet) await refreshAnnotations(idx);
    }
  }

  const zoomPct = $derived(Math.round(vstore.effectiveZoom * 100));
  const TOOL_ICONS: Record<AnnotTool, string> = {
    none: "✏️", highlight: "🖊", underline: "U̲", strikeout: "S̶", text: "💬", ink: "🖊️",
  };
  const TOOL_LABELS: Record<AnnotTool, string> = {
    none: "No tool", highlight: "Highlight", underline: "Underline", strikeout: "Strikethrough",
    text: "Sticky note", ink: "Freehand",
  };
</script>

<svelte:window onkeydown={onKeyDown} />

<section class="viewer" aria-label="PDF viewer">
  <!-- ── Toolbar ── -->
  <div class="toolbar">
    <div class="toolbar-left">
      <span class="doc-title" title={tab.path}>{tab.dirty ? "• " : ""}{tab.title}</span>
    </div>

    <div class="toolbar-center">
      {#if vstore.pageSizes.length > 0}
        <span class="page-nav">
          <label>
            Page
            <input type="number" min="1" max={vstore.pageSizes.length}
              value={vstore.currentPage + 1} onchange={onPageInput} aria-label="Current page" />
          </label>
          <span class="page-total">/ {vstore.pageSizes.length}</span>
        </span>
      {/if}
    </div>

    <div class="toolbar-right">
      <!-- Zoom -->
      <button onclick={() => vstore.setZoom(vstore.effectiveZoom * 0.8)} title="Zoom out (Ctrl+-)">−</button>
      <span class="zoom-pct">{zoomPct}%</span>
      <button onclick={() => vstore.setZoom(vstore.effectiveZoom * 1.2)} title="Zoom in (Ctrl++)">+</button>
      <button class:active={vstore.zoomMode === "fit-width"} onclick={() => vstore.setZoomMode("fit-width")} title="Fit width">⟺</button>
      <button class:active={vstore.zoomMode === "fit-page"} onclick={() => vstore.setZoomMode("fit-page")} title="Fit page">□</button>
      <span class="sep"></span>
      <!-- Rotate -->
      <button onclick={() => vstore.rotateCcw()} title="Rotate left">↺</button>
      <button onclick={() => vstore.rotateCw()} title="Rotate right">↻</button>
      <span class="sep"></span>
      <!-- Annotation tools -->
      {#each (["highlight","underline","strikeout","text","ink"] as const) as t}
        <button class:active={activeTool === t} onclick={() => activeTool = activeTool === t ? "none" : t}
          title="{TOOL_LABELS[t]}" aria-pressed={activeTool === t}>{TOOL_ICONS[t]}</button>
      {/each}
      <input type="color" class="color-pick"
        value="#{toolColor.map(c => c.toString(16).padStart(2,'0')).join('')}"
        title="Annotation color"
        oninput={(e) => {
          const v = (e.target as HTMLInputElement).value.slice(1);
          toolColor = [parseInt(v.slice(0,2),16), parseInt(v.slice(2,4),16), parseInt(v.slice(4,6),16)];
        }}
      />
      <span class="sep"></span>
      <!-- Find, sidebar, sign, save -->
      <button class:active={findOpen} onclick={openFindBar} title="Find (Ctrl+F)">🔍</button>
      <button class:active={sidebarOpen} onclick={() => sidebarOpen = !sidebarOpen} title="Comments">💬</button>
      <button onclick={() => signOpen = true} title="Sign">✍️</button>
      {#if tab.dirty}
        <button onclick={handleSave} title="Save (Ctrl+S)" class="save-btn">💾</button>
      {/if}
    </div>
  </div>

  <!-- ── XFA warning ── -->
  {#if formType === "xfa_full" || formType === "xfa_foreground"}
    <div class="xfa-banner" role="alert">
      ⚠ This PDF uses XFA forms, which are not supported. Fields are displayed read-only.
    </div>
  {/if}

  <!-- ── Find bar ── -->
  {#if findOpen}
    <div class="find-bar" role="search">
      <input type="search" placeholder="Find in document…" bind:value={findQuery}
        bind:this={findInput} onkeydown={onFindKeyDown} aria-label="Search"
        autocomplete="off" spellcheck="false" />
      <span class="find-count" aria-live="polite">
        {#if findQuery.trim()}
          {findMatches.length > 0 ? `${findCurrentMatch + 1} / ${findMatches.length}` : "No results"}
        {/if}
      </span>
      <button onclick={prevMatch} disabled={findMatches.length === 0} title="Previous (Shift+Enter)">↑</button>
      <button onclick={nextMatch} disabled={findMatches.length === 0} title="Next (Enter)">↓</button>
      <button onclick={closeFindBar} title="Close (Escape)">✕</button>
    </div>
  {/if}

  <!-- ── Main content ── -->
  <div class="content-row">
    <!-- Pages -->
    <div class="pages-area" bind:this={container} onwheel={onWheel} role="document" tabindex="-1">
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
                rotation={vstore.rotation}
                textSpans={textSpansByPage[i]}
                highlights={pageHighlights.get(i)}
                activeHighlight={activeByPage.get(i) ?? -1}
                annotations={annotsByPage[i]}
                formFields={formFieldsByPage[i]}
                xfaReadOnly={formType === "xfa_full" || formType === "xfa_foreground"}
                activeTool={activeTool}
                onPageClick={(e, el, cw, ch) => handlePageClick(e, i, el, cw, ch)}
                onTextSelected={(rects) => handleTextSelection(i, rects)}
                onInkStroke={(paths) => handleInkStroke(i, paths)}
                onDeleteAnnotation={(idx) => handleDeleteAnnotation(i, idx)}
                onFieldText={(annotIdx, val) => handleFieldText(i, annotIdx, val)}
                onFieldChecked={(annotIdx, val) => handleFieldChecked(i, annotIdx, val)}
                inkColor={toolColor}
                inkWidth={inkWidth}
              />
              <span class="page-label" aria-hidden="true">{i + 1}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Sidebar -->
    {#if sidebarOpen}
      <aside class="sidebar" aria-label="Annotations">
        <div class="sidebar-header">
          <span>Comments</span>
          <button onclick={() => sidebarOpen = false} aria-label="Close sidebar">✕</button>
        </div>
        <div class="sidebar-list">
          {#if allAnnotations.length === 0}
            <p class="sidebar-empty">No annotations yet.</p>
          {:else}
            {#each allAnnotations as { pageIndex, annot }}
              <div class="sidebar-item" role="button" tabindex="0"
                onclick={() => scrollToAnnotation(pageIndex)}
                onkeydown={(e) => { if (e.key === "Enter") scrollToAnnotation(pageIndex); }}>
                <span class="sidebar-kind">{annot.kind}</span>
                <span class="sidebar-page">p.{pageIndex + 1}</span>
                {#if annot.contents}
                  <p class="sidebar-text">{annot.contents}</p>
                {/if}
                {#if annot.author}
                  <p class="sidebar-author">{annot.author}</p>
                {/if}
              </div>
            {/each}
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</section>

<!-- Signature capture modal -->
{#if signOpen}
  <SignatureCapture
    onClose={() => signOpen = false}
    onPlace={async (paths) => {
      signOpen = false;
      const currentPage = vstore.currentPage;
      if (paths.length > 0) {
        await addInkAnnotation(docId, currentPage, paths, [0, 0, 0], 2);
        tabs.markDirty(tab.id, true);
        await refreshAnnotations(currentPage);
      }
    }}
  />
{/if}

<style>
  .viewer { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  /* Toolbar */
  .toolbar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--bg-elev);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    min-height: 40px;
  }
  .toolbar-left { overflow: hidden; }
  .toolbar-center { display: flex; align-items: center; justify-content: center; }
  .toolbar-right { display: flex; align-items: center; gap: 3px; justify-content: flex-end; flex-wrap: nowrap; }
  .doc-title { font-size: 13px; color: var(--fg-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: block; }
  .page-nav { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--fg-muted); }
  .page-nav label { display: flex; align-items: center; gap: 4px; }
  .page-nav input {
    width: 44px; padding: 3px 6px; border: 1px solid var(--border);
    border-radius: var(--radius); background: var(--bg); color: var(--fg);
    text-align: center; font: inherit; font-size: 13px;
  }
  .toolbar-right button {
    background: transparent; border: 1px solid var(--border); border-radius: var(--radius);
    padding: 3px 7px; cursor: pointer; font-size: 13px; color: var(--fg-muted);
    line-height: 1; min-width: 26px; white-space: nowrap; flex-shrink: 0;
  }
  .toolbar-right button:hover { background: var(--bg-chrome); color: var(--fg); }
  .toolbar-right button:disabled { opacity: 0.4; cursor: default; }
  .toolbar-right button.active { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .toolbar-right .save-btn { border-color: var(--accent); color: var(--accent); }
  .zoom-pct { font-size: 13px; color: var(--fg-muted); min-width: 38px; text-align: center; }
  .sep { width: 1px; height: 16px; background: var(--border); margin: 0 2px; flex-shrink: 0; }
  .color-pick {
    width: 24px; height: 24px; border: 1px solid var(--border); border-radius: var(--radius);
    padding: 1px; cursor: pointer; background: none; flex-shrink: 0;
  }

  /* XFA warning */
  .xfa-banner {
    padding: 6px 16px; font-size: 12px; flex-shrink: 0;
    background: #fff3cd; color: #664d03; border-bottom: 1px solid #ffc107;
  }

  /* Find bar */
  .find-bar {
    display: flex; align-items: center; gap: 4px; padding: 5px 12px;
    background: var(--bg-elev); border-bottom: 1px solid var(--border); flex-shrink: 0;
  }
  .find-bar input[type="search"] {
    flex: 1; max-width: 280px; padding: 4px 8px; border: 1px solid var(--border);
    border-radius: var(--radius); background: var(--bg); color: var(--fg);
    font: inherit; font-size: 13px; outline: none;
  }
  .find-bar input[type="search"]:focus { border-color: var(--accent); }
  .find-count { font-size: 12px; color: var(--fg-muted); min-width: 72px; white-space: nowrap; }
  .find-bar button {
    background: transparent; border: 1px solid var(--border); border-radius: var(--radius);
    padding: 4px 8px; cursor: pointer; font-size: 13px; color: var(--fg-muted); line-height: 1;
  }
  .find-bar button:hover:not(:disabled) { background: var(--bg-chrome); color: var(--fg); }
  .find-bar button:disabled { opacity: 0.4; cursor: default; }

  /* Content layout */
  .content-row { flex: 1; display: flex; overflow: hidden; }
  .pages-area { flex: 1; overflow: auto; background: var(--bg); outline: none; }
  .pages-list {
    display: flex; flex-direction: column; align-items: center;
    gap: 16px; padding: 24px 24px 48px;
  }
  .page-entry { position: relative; display: flex; flex-direction: column; align-items: center; gap: 4px; }
  .page-label { font-size: 11px; color: var(--fg-muted); opacity: 0.6; user-select: none; }
  .center-msg {
    display: flex; align-items: center; justify-content: center;
    height: 100%; min-height: 200px; color: var(--fg-muted); font-size: 14px;
  }

  /* Sidebar */
  .sidebar {
    width: 260px; flex-shrink: 0; border-left: 1px solid var(--border);
    background: var(--bg-elev); display: flex; flex-direction: column; overflow: hidden;
  }
  .sidebar-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 10px 12px; border-bottom: 1px solid var(--border);
    font-size: 13px; font-weight: 500;
  }
  .sidebar-header button {
    background: none; border: none; cursor: pointer; font-size: 14px; color: var(--fg-muted); padding: 2px;
  }
  .sidebar-header button:hover { color: var(--fg); }
  .sidebar-list { flex: 1; overflow-y: auto; padding: 8px; }
  .sidebar-empty { color: var(--fg-muted); font-size: 13px; text-align: center; padding: 24px 0; }
  .sidebar-item {
    padding: 8px 10px; border-radius: var(--radius); cursor: pointer; margin-bottom: 4px;
    border: 1px solid var(--border); background: var(--bg);
  }
  .sidebar-item:hover { border-color: var(--accent); }
  .sidebar-kind { font-size: 11px; font-weight: 600; text-transform: uppercase; color: var(--accent); }
  .sidebar-page { float: right; font-size: 11px; color: var(--fg-muted); }
  .sidebar-text { font-size: 12px; margin: 4px 0 0; color: var(--fg); }
  .sidebar-author { font-size: 11px; color: var(--fg-muted); margin: 2px 0 0; }
</style>
