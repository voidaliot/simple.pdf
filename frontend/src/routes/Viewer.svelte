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
    resetFormFields,
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

  // ── Text spans ────────────────────────────────────────────────────────────────
  let textSpansByPage = $state<(TextSpan[] | undefined)[]>([]);

  async function loadTextSpans(pageIndex: number) {
    if (textSpansByPage[pageIndex] !== undefined) return;
    textSpansByPage[pageIndex] = [];
    try { textSpansByPage[pageIndex] = await getPageTextSpans(docId, pageIndex); }
    catch { textSpansByPage[pageIndex] = []; }
  }

  $effect(() => { for (const idx of visibleSet) loadTextSpans(idx); });

  // ── Forms ─────────────────────────────────────────────────────────────────────
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

  async function handlePushButton(pageIndex: number) {
    await resetFormFields(docId, pageIndex).catch(console.error);
    formFieldsByPage[pageIndex] = undefined;
    await loadFormFields(pageIndex);
    tabs.markDirty(tab.id, true);
  }

  // ── Annotations ───────────────────────────────────────────────────────────────
  let annotsByPage = $state<(Annotation[] | undefined)[]>([]);
  let annotsVersionByPage = $state<number[]>([]);

  function bumpAnnotsVersion(pageIndex: number) {
    annotsVersionByPage[pageIndex] = (annotsVersionByPage[pageIndex] ?? 0) + 1;
  }

  async function loadAnnotations(pageIndex: number) {
    try { annotsByPage[pageIndex] = await getPageAnnotations(docId, pageIndex); }
    catch { annotsByPage[pageIndex] = []; }
  }

  async function refreshAnnotations(pageIndex: number) {
    annotsByPage[pageIndex] = await getPageAnnotations(docId, pageIndex).catch(() => []);
    bumpAnnotsVersion(pageIndex);
  }

  $effect(() => {
    for (const idx of visibleSet) {
      if (annotsByPage[idx] === undefined) loadAnnotations(idx);
    }
  });

  // ── Annotation sidebar ────────────────────────────────────────────────────────
  let sidebarOpen = $state(false);

  const allAnnotations = $derived.by(() => {
    const list: { pageIndex: number; annot: Annotation }[] = [];
    annotsByPage.forEach((anns, pi) => {
      if (!anns) return;
      for (const a of anns) if (a.kind !== "widget") list.push({ pageIndex: pi, annot: a });
    });
    return list;
  });

  function scrollToPage(pageIndex: number) {
    container?.querySelector<HTMLElement>(`[data-page-index="${pageIndex}"]`)
      ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }

  // ── Annotation tools ──────────────────────────────────────────────────────────
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

  async function handleTextSelection(pageIndex: number, selRects: AnnRect[]) {
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

  // ── Signing ───────────────────────────────────────────────────────────────────
  let signOpen = $state(false);

  // ── Find-in-page ──────────────────────────────────────────────────────────────
  interface FindMatch {
    pageIndex: number;
    left: number; top: number; width: number; height: number;
  }

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
    for (const m of findMatches) {
      let arr = map.get(m.pageIndex);
      if (!arr) { arr = []; map.set(m.pageIndex, arr); }
      arr.push(m);
    }
    return map;
  });

  // Index of the active match within its page's highlight list
  const activeByPage = $derived.by(() => {
    const cur = findMatches[findCurrentMatch];
    if (!cur) return new Map<number, number>();
    let idxInPage = 0;
    for (const m of findMatches.slice(0, findCurrentMatch + 1)) {
      if (m.pageIndex === cur.pageIndex) idxInPage++;
    }
    return new Map([[cur.pageIndex, idxInPage - 1]]);
  });

  $effect(() => { findQuery; findCurrentMatch = 0; });

  async function openFindBar() {
    findOpen = true;
    // Pre-load all text spans so search works immediately
    for (let i = 0; i < vstore.pageSizes.length; i++) loadTextSpans(i);
    await tick();
    findInput?.focus();
    findInput?.select();
  }

  function closeFindBar() { findOpen = false; findQuery = ""; }

  function navigateToMatch(idx: number) {
    const m = findMatches[idx];
    if (m) scrollToPage(m.pageIndex);
  }

  function nextMatch() {
    if (!findMatches.length) return;
    findCurrentMatch = (findCurrentMatch + 1) % findMatches.length;
    navigateToMatch(findCurrentMatch);
  }

  function prevMatch() {
    if (!findMatches.length) return;
    findCurrentMatch = (findCurrentMatch - 1 + findMatches.length) % findMatches.length;
    navigateToMatch(findCurrentMatch);
  }

  // Scroll to current match when it changes
  $effect(() => {
    const idx = findCurrentMatch;
    if (findMatches.length > 0 && findOpen) navigateToMatch(idx);
  });

  // ── Page loading ──────────────────────────────────────────────────────────────
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
        const directlyVisible = new Set<number>();
        for (const entry of entries) {
          const idx = Number((entry.target as HTMLElement).dataset.pageIndex);
          if (entry.isIntersecting) directlyVisible.add(idx);
        }
        if (directlyVisible.size === 0) return;

        const next = new Set(visibleSet);
        for (const idx of directlyVisible) next.add(idx);
        visibleSet = next;
        vstore.setCurrentPage(Math.min(...directlyVisible));

        // Prefetch ±1 neighbours after a tick (avoids piling up concurrent renders)
        setTimeout(() => {
          const prefetch = new Set(visibleSet);
          for (const idx of directlyVisible) {
            if (idx > 0) prefetch.add(idx - 1);
            if (idx < pages.length - 1) prefetch.add(idx + 1);
          }
          visibleSet = prefetch;
        }, 0);
      },
      { root: container, rootMargin: "1200px 0px", threshold: 0.01 }
    );

    requestAnimationFrame(() => {
      container?.querySelectorAll<HTMLElement>("[data-page-index]")
        .forEach((el) => obs.observe(el));
    });

    return () => obs.disconnect();
  });

  // ── Keyboard ──────────────────────────────────────────────────────────────────
  async function onKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey) {
      if (e.key === "f" || e.key === "F") { e.preventDefault(); openFindBar(); return; }
      if (e.key === "s" || e.key === "S") { e.preventDefault(); await handleSave(); return; }
      if (e.key === "z" || e.key === "Z") { e.preventDefault(); await handleUndo(); return; }
      if (e.key === "=" || e.key === "+") { e.preventDefault(); vstore.zoomIn(); return; }
      if (e.key === "-")                  { e.preventDefault(); vstore.zoomOut(); return; }
      if (e.key === "0")                  { e.preventDefault(); vstore.setZoomMode("fit-width"); return; }
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
    if (e.deltaY < 0) vstore.zoomIn(); else vstore.zoomOut();
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
    none: "No tool", highlight: "Highlight", underline: "Underline",
    strikeout: "Strikethrough", text: "Sticky note", ink: "Freehand",
  };

  const noResults = $derived(findOpen && findQuery.trim().length > 0 && findMatches.length === 0);
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
        <div class="page-nav">
          <span class="page-label-text">Page</span>
          <input
            type="number"
            min="1"
            max={vstore.pageSizes.length}
            value={vstore.currentPage + 1}
            aria-label="Current page"
            onchange={onPageInput}
            onclick={(e) => (e.target as HTMLInputElement).select()}
            onfocus={(e) => (e.target as HTMLInputElement).select()}
          />
          <span class="page-total">/ {vstore.pageSizes.length}</span>
        </div>
      {/if}
    </div>

    <div class="toolbar-right">
      <!-- Zoom controls with snap levels -->
      <button onclick={() => vstore.zoomOut()} title="Zoom out (Ctrl+−)" aria-label="Zoom out">−</button>
      <button
        class="zoom-pct-btn"
        onclick={() => vstore.setZoomMode("fit-width")}
        title="Click for fit-width; also Ctrl+0"
        aria-label="Zoom level — click to fit width"
      >{zoomPct}%</button>
      <button onclick={() => vstore.zoomIn()} title="Zoom in (Ctrl++)" aria-label="Zoom in">+</button>
      <button
        class:active={vstore.zoomMode === "fit-width"}
        onclick={() => vstore.setZoomMode("fit-width")}
        title="Fit width (Ctrl+0)"
        aria-label="Fit width"
      >⟺</button>
      <button
        class:active={vstore.zoomMode === "fit-page"}
        onclick={() => vstore.setZoomMode("fit-page")}
        title="Fit page"
        aria-label="Fit page"
      >□</button>

      <span class="sep" aria-hidden="true"></span>

      <!-- Rotate -->
      <button onclick={() => vstore.rotateCcw()} title="Rotate left" aria-label="Rotate left">↺</button>
      <button onclick={() => vstore.rotateCw()} title="Rotate right" aria-label="Rotate right">↻</button>

      <span class="sep" aria-hidden="true"></span>

      <!-- Annotation tools -->
      {#each (["highlight","underline","strikeout","text","ink"] as const) as t}
        <button
          class:active={activeTool === t}
          onclick={() => { activeTool = activeTool === t ? "none" : t; }}
          title={TOOL_LABELS[t]}
          aria-pressed={activeTool === t}
          aria-label={TOOL_LABELS[t]}
        >{TOOL_ICONS[t]}</button>
      {/each}
      <input
        type="color"
        class="color-pick"
        value="#{toolColor.map((c) => c.toString(16).padStart(2, "0")).join("")}"
        title="Annotation color"
        aria-label="Annotation color"
        oninput={(e) => {
          const v = (e.target as HTMLInputElement).value.slice(1);
          toolColor = [parseInt(v.slice(0,2),16), parseInt(v.slice(2,4),16), parseInt(v.slice(4,6),16)];
        }}
      />

      <span class="sep" aria-hidden="true"></span>

      <!-- Find, sidebar, sign, save -->
      <button class:active={findOpen} onclick={openFindBar} title="Find (Ctrl+F)" aria-label="Find in document">🔍</button>
      <button class:active={sidebarOpen} onclick={() => { sidebarOpen = !sidebarOpen; }} title="Comments sidebar" aria-label="Toggle comments">💬</button>
      <button onclick={() => { signOpen = true; }} title="Sign document" aria-label="Sign document">✍️</button>
      {#if tab.dirty}
        <button onclick={handleSave} title="Save (Ctrl+S)" class="save-btn" aria-label="Save document">💾</button>
      {/if}
    </div>
  </div>

  <!-- ── XFA warning ── -->
  {#if formType === "xfa_full" || formType === "xfa_foreground"}
    <div class="xfa-banner" role="alert">
      ⚠ This PDF uses XFA forms — not fully supported. Fields shown read-only.
    </div>
  {/if}

  <!-- ── Find bar ── -->
  {#if findOpen}
    <div class="find-bar" role="search" aria-label="Find in document">
      <input
        type="search"
        placeholder="Find in document…"
        bind:value={findQuery}
        bind:this={findInput}
        class:no-results={noResults}
        onkeydown={onFindKeyDown}
        aria-label="Search query"
        autocomplete="off"
        spellcheck="false"
      />
      <span class="find-count" aria-live="polite" aria-atomic="true">
        {#if findQuery.trim()}
          {#if findMatches.length > 0}
            {findCurrentMatch + 1} / {findMatches.length}
          {:else}
            No results
          {/if}
        {/if}
      </span>
      <button onclick={prevMatch} disabled={findMatches.length === 0} title="Previous (Shift+Enter)" aria-label="Previous match">↑</button>
      <button onclick={nextMatch} disabled={findMatches.length === 0} title="Next (Enter)" aria-label="Next match">↓</button>
      <button onclick={closeFindBar} title="Close (Escape)" aria-label="Close find bar">✕</button>
    </div>
  {/if}

  <!-- ── Main content ── -->
  <div class="content-row">
    <!-- Pages scroll area -->
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
            {@const pageMatches = pageHighlights.get(i)}
            {@const activeIdx = activeByPage.get(i) ?? -1}
            <div class="page-entry" data-page-index={i}>
              <Page
                {docId}
                pageIndex={i}
                width={size.width}
                height={size.height}
                zoom={vstore.effectiveZoom}
                visible={visibleSet.has(i)}
                rotation={vstore.rotation}
                textSpans={textSpansByPage[i]}
                highlights={pageMatches}
                activeHighlight={activeIdx}
                annotations={annotsByPage[i]}
                annotationsVersion={annotsVersionByPage[i] ?? 0}
                formFields={formFieldsByPage[i]}
                xfaReadOnly={formType === "xfa_full" || formType === "xfa_foreground"}
                activeTool={activeTool}
                onPageClick={(e, el, cw, ch) => handlePageClick(e, i, el, cw, ch)}
                onTextSelected={(rects) => handleTextSelection(i, rects)}
                onInkStroke={(paths) => handleInkStroke(i, paths)}
                onDeleteAnnotation={(idx) => handleDeleteAnnotation(i, idx)}
                onFieldText={(annotIdx, val) => handleFieldText(i, annotIdx, val)}
                onFieldChecked={(annotIdx, val) => handleFieldChecked(i, annotIdx, val)}
                onPushButton={() => handlePushButton(i)}
                inkColor={toolColor}
                {inkWidth}
              />
              <span class="page-label" aria-hidden="true">{i + 1}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Annotations sidebar -->
    {#if sidebarOpen}
      <aside class="sidebar" aria-label="Annotations">
        <div class="sidebar-header">
          <span>Comments</span>
          <button onclick={() => { sidebarOpen = false; }} aria-label="Close sidebar">✕</button>
        </div>
        <div class="sidebar-list">
          {#if allAnnotations.length === 0}
            <p class="sidebar-empty">No annotations yet.</p>
          {:else}
            {#each allAnnotations as { pageIndex, annot }}
              <div
                class="sidebar-item"
                role="button"
                tabindex="0"
                onclick={() => scrollToPage(pageIndex)}
                onkeydown={(e) => { if (e.key === "Enter") scrollToPage(pageIndex); }}
              >
                <span class="sidebar-kind">{annot.kind}</span>
                <span class="sidebar-page">p.{pageIndex + 1}</span>
                {#if annot.contents}<p class="sidebar-text">{annot.contents}</p>{/if}
                {#if annot.author}<p class="sidebar-author">{annot.author}</p>{/if}
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
    onClose={() => { signOpen = false; }}
    onPlace={async (paths) => {
      signOpen = false;
      const currentPage = vstore.currentPage;
      if (paths.length === 0) return;
      const sigAspect = 480 / 200;
      const targetW = 0.30;
      const targetH = targetW / sigAspect;
      const targetLeft = 0.65;
      const targetTop = 0.85 - targetH;
      const placedPaths = paths.map((path) =>
        path.map<[number, number]>(([nx, ny]) => [
          targetLeft + nx * targetW,
          targetTop + ny * targetH,
        ]),
      );
      await addInkAnnotation(docId, currentPage, placedPaths, [0, 0, 0], 2);
      tabs.markDirty(tab.id, true);
      await refreshAnnotations(currentPage);
    }}
  />
{/if}

<style>
  .viewer { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

  /* ── Toolbar ── */
  .toolbar {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    background: var(--bg-elev);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
    min-height: 40px;
  }

  .toolbar-left { overflow: hidden; }
  .toolbar-center { display: flex; align-items: center; justify-content: center; }
  .toolbar-right {
    display: flex; align-items: center; gap: 3px;
    justify-content: flex-end; flex-wrap: nowrap;
  }

  .doc-title {
    font-size: 13px; color: var(--fg-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: block;
  }

  /* Page nav */
  .page-nav { display: flex; align-items: center; gap: 5px; font-size: 13px; }
  .page-label-text { color: var(--fg-muted); }
  .page-nav input {
    width: 44px; padding: 3px 6px;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg); color: var(--fg);
    text-align: center; font: inherit; font-size: 13px; outline: none;
    cursor: text;
  }
  .page-nav input:focus { border-color: var(--accent); }
  .page-total { color: var(--fg-muted); }

  /* Toolbar buttons */
  .toolbar-right button {
    background: transparent; border: 1px solid var(--border); border-radius: var(--radius);
    padding: 3px 7px; cursor: pointer; font-size: 13px; color: var(--fg-muted);
    line-height: 1; min-width: 26px; white-space: nowrap; flex-shrink: 0;
    transition: background 80ms, color 80ms;
  }
  .toolbar-right button:hover { background: var(--bg-chrome); color: var(--fg); }
  .toolbar-right button:disabled { opacity: 0.4; cursor: default; }
  .toolbar-right button.active { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .toolbar-right .save-btn { border-color: var(--accent); color: var(--accent); }

  /* Zoom % button — doubles as fit-width shortcut */
  .zoom-pct-btn {
    min-width: 48px; text-align: center; font-variant-numeric: tabular-nums;
    font-size: 13px;
  }

  .sep { width: 1px; height: 16px; background: var(--border); margin: 0 2px; flex-shrink: 0; }

  .color-pick {
    width: 24px; height: 24px; border: 1px solid var(--border); border-radius: var(--radius);
    padding: 1px; cursor: pointer; background: none; flex-shrink: 0;
  }

  /* ── XFA banner ── */
  .xfa-banner {
    padding: 6px 16px; font-size: 12px; flex-shrink: 0;
    background: #fff3cd; color: #664d03; border-bottom: 1px solid #ffc107;
  }

  /* ── Find bar ── */
  .find-bar {
    display: flex; align-items: center; gap: 4px;
    padding: 5px 12px; background: var(--bg-elev);
    border-bottom: 1px solid var(--border); flex-shrink: 0;
  }

  .find-bar input[type="search"] {
    flex: 1; max-width: 280px; padding: 4px 8px;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg); color: var(--fg);
    font: inherit; font-size: 13px; outline: none;
    transition: border-color 80ms;
  }
  .find-bar input[type="search"]:focus { border-color: var(--accent); }
  .find-bar input.no-results { border-color: var(--danger); color: var(--danger); }
  .find-bar input[type="search"]::-webkit-search-cancel-button { display: none; }

  .find-count {
    font-size: 12px; color: var(--fg-muted);
    min-width: 72px; white-space: nowrap; text-align: center;
  }

  .find-bar button {
    background: transparent; border: 1px solid var(--border); border-radius: var(--radius);
    padding: 4px 8px; cursor: pointer; font-size: 13px; color: var(--fg-muted); line-height: 1;
  }
  .find-bar button:hover:not(:disabled) { background: var(--bg-chrome); color: var(--fg); }
  .find-bar button:disabled { opacity: 0.4; cursor: default; }

  /* ── Content layout ── */
  .content-row { flex: 1; display: flex; overflow: hidden; }
  .pages-area { flex: 1; overflow: auto; background: var(--bg); outline: none; }

  .pages-list {
    display: flex; flex-direction: column; align-items: center;
    gap: 8px;            /* 8px gap between pages */
    padding: 16px 16px 48px;
  }

  .page-entry {
    position: relative; display: flex;
    flex-direction: column; align-items: center; gap: 4px;
  }

  .page-label { font-size: 11px; color: var(--fg-muted); opacity: 0.55; user-select: none; }

  .center-msg {
    display: flex; align-items: center; justify-content: center;
    height: 100%; min-height: 200px; color: var(--fg-muted); font-size: 14px;
  }

  /* ── Sidebar ── */
  .sidebar {
    width: 256px; flex-shrink: 0; border-left: 1px solid var(--border);
    background: var(--bg-elev); display: flex; flex-direction: column; overflow: hidden;
  }

  .sidebar-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 10px 12px; border-bottom: 1px solid var(--border);
    font-size: 13px; font-weight: 500;
  }

  .sidebar-header button {
    background: none; border: none; cursor: pointer;
    font-size: 14px; color: var(--fg-muted); padding: 2px;
  }
  .sidebar-header button:hover { color: var(--fg); }

  .sidebar-list { flex: 1; overflow-y: auto; padding: 8px; }
  .sidebar-empty { color: var(--fg-muted); font-size: 13px; text-align: center; padding: 24px 0; }

  .sidebar-item {
    padding: 8px 10px; border-radius: var(--radius); cursor: pointer;
    margin-bottom: 4px; border: 1px solid var(--border); background: var(--bg);
    transition: border-color 80ms;
  }
  .sidebar-item:hover { border-color: var(--accent); }

  .sidebar-kind {
    font-size: 11px; font-weight: 600; text-transform: uppercase; color: var(--accent);
  }
  .sidebar-page { float: right; font-size: 11px; color: var(--fg-muted); }
  .sidebar-text { font-size: 12px; margin: 4px 0 0; color: var(--fg); }
  .sidebar-author { font-size: 11px; color: var(--fg-muted); margin: 2px 0 0; }
</style>
