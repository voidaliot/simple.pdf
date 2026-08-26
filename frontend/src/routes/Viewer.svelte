<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from "svelte";
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
    resetAllFormFields,
    type TextSpan,
    type Annotation,
    type AnnRect,
    type FormField,
  } from "../lib/ipc";
  import Page, { type Highlight } from "../components/Page.svelte";
  import SignatureCapture from "../components/SignatureCapture.svelte";
  import Icon from "../components/Icon.svelte";

  interface Props { tab: Tab; }
  let { tab }: Props = $props();

  const docId = $derived(tab.docId ?? "");
  const vstore = untrack(() => createViewerStore(docId));

  let container: HTMLElement | undefined = $state();
  let renderSet = $state(new Set<number>());
  let viewportSet = $state(new Set<number>());
  let loadingPages = $state(false);
  let pagesError = $state("");
  let scrollRestored = false;
  let currentPageLock: number | undefined;
  let pendingContainerSize: { width: number; height: number } | undefined;
  let resizeAnchorFrame: number | undefined;
  let resizeAnchorRunning = false;

  // ── Text spans ────────────────────────────────────────────────────────────────
  let textSpansByPage = $state<(TextSpan[] | undefined)[]>([]);

  async function loadTextSpans(pageIndex: number) {
    if (textSpansByPage[pageIndex] !== undefined) return;
    textSpansByPage[pageIndex] = [];
    try { textSpansByPage[pageIndex] = await getPageTextSpans(docId, pageIndex); }
    catch { textSpansByPage[pageIndex] = []; }
  }

  $effect(() => { for (const idx of renderSet) loadTextSpans(idx); });

  // ── Forms ─────────────────────────────────────────────────────────────────────
  let formType = $state("none");
  let formFieldsByPage = $state<(FormField[] | undefined)[]>([]);
  let formWriteQueue: Promise<void> = Promise.resolve();
  let formWriteError = $state("");
  let formWriteSequence = 0;
  const failedFormWrites = new Map<string, { sequence: number; message: string }>();

  function describeError(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function syncFormWriteError() {
    let latest: { sequence: number; message: string } | undefined;
    for (const failure of failedFormWrites.values()) {
      if (!latest || failure.sequence > latest.sequence) latest = failure;
    }
    formWriteError = latest?.message ?? "";
  }

  function queueFormWrite(
    key: string,
    write: () => Promise<void>,
    supersedesAll = false,
  ): Promise<void> {
    // Dirty state must be synchronous: closing the tab immediately after an
    // edit must still offer the unsaved-changes guard.
    tabs.markDirty(tab.id, true);
    const sequence = ++formWriteSequence;
    formWriteQueue = formWriteQueue
      .catch(() => undefined)
      .then(async () => {
        try {
          await write();
          if (supersedesAll) {
            failedFormWrites.clear();
          } else {
            const previousFailure = failedFormWrites.get(key);
            if (previousFailure && previousFailure.sequence <= sequence) {
              failedFormWrites.delete(key);
            }
          }
          syncFormWriteError();
        } catch (error) {
          failedFormWrites.set(key, { sequence, message: describeError(error) });
          syncFormWriteError();
          throw error;
        }
      });
    // Keep the rejected promise available for Save while preventing an
    // unhandled-rejection report from an event handler.
    void formWriteQueue.catch((error) => console.error("form write failed", error));
    return formWriteQueue;
  }

  async function flushFormWrites(): Promise<boolean> {
    // A write can be queued while this function is awaiting an earlier tail.
    // Keep following the tail until no new edit was added during the wait.
    while (true) {
      const sequence = formWriteSequence;
      try { await formWriteQueue; }
      catch { /* The sticky failure map below is the source of truth. */ }
      if (sequence !== formWriteSequence) continue;
      syncFormWriteError();
      return failedFormWrites.size === 0;
    }
  }

  onMount(async () => {
    try { formType = await getFormType(docId); } catch { formType = "none"; }
  });

  async function loadFormFields(pageIndex: number) {
    if (formType === "none" || formFieldsByPage[pageIndex] !== undefined) return;
    formFieldsByPage[pageIndex] = [];
    try { formFieldsByPage[pageIndex] = await getFormFields(docId, pageIndex); }
    catch { formFieldsByPage[pageIndex] = []; }
  }

  $effect(() => { for (const idx of renderSet) loadFormFields(idx); });

  function handleFieldText(pageIndex: number, annotIndex: number, value: string) {
    queueFormWrite(
      `field:${pageIndex}:${annotIndex}`,
      () => setFieldTextValue(docId, pageIndex, annotIndex, value),
    );
  }

  function handleFieldChecked(pageIndex: number, annotIndex: number, checked: boolean) {
    queueFormWrite(
      `field:${pageIndex}:${annotIndex}`,
      () => setFieldChecked(docId, pageIndex, annotIndex, checked),
    );
  }

  async function handlePushButton(_pageIndex: number, actionType: string) {
    if (actionType === "submit") {
      alert("Form submission is not supported — please fill and save the PDF, then submit it using your browser or email client.");
      return;
    }
    if (actionType === "other") return; // print or unknown non-reset action

    // Reset action: clear all fields in the document (not just the current page),
    // matching PDF ResetForm semantics when no field-inclusion list is specified.
    try {
      await queueFormWrite(
        "document:reset",
        () => resetAllFormFields(docId),
        true,
      );
    } catch {
      return;
    }
    // Invalidate the form-fields cache for every page that was already loaded.
    formFieldsByPage = formFieldsByPage.map(() => undefined);
    for (const idx of renderSet) await loadFormFields(idx);
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
    for (const idx of renderSet) {
      if (annotsByPage[idx] === undefined) loadAnnotations(idx);
    }
  });

  // ── Annotation sidebar ────────────────────────────────────────────────────────
  let sidebarOpen = $state(false);

  const allAnnotations = $derived.by(() => {
    const list: { pageIndex: number; annot: Annotation }[] = [];
    annotsByPage.forEach((anns, pi) => {
      if (!anns) return;
      for (const a of anns) {
        if (a.kind !== "widget" && a.kind !== "link") list.push({ pageIndex: pi, annot: a });
      }
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
  let markupOpen = $state(false);
  let toolColor = $state<[number, number, number]>([255, 214, 0]);
  let inkWidth = $state(2);

  function chooseTool(tool: Exclude<AnnotTool, "none">) {
    activeTool = activeTool === tool ? "none" : tool;
    markupOpen = false;
  }

  async function handlePageClick(pageIndex: number, left: number, top: number) {
    if (activeTool !== "text") return;
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
  let findIndexing = $state(false);
  let findGeneration = 0;

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
    await tick();
    findInput?.focus();
    findInput?.select();

    const generation = ++findGeneration;
    findIndexing = true;
    for (let i = 0; i < vstore.pageSizes.length; i++) {
      if (!findOpen || generation !== findGeneration) break;
      await loadTextSpans(i);
      // Yield between pages so rendering and scrolling remain responsive.
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    }
    if (generation === findGeneration) findIndexing = false;
  }

  function closeFindBar() {
    findOpen = false;
    findQuery = "";
    findIndexing = false;
    findGeneration += 1;
  }

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
  async function loadPages() {
    loadingPages = true;
    pagesError = "";
    try {
      vstore.setPageSizes(await getPageSizes(docId));
    } catch (e: unknown) {
      pagesError = e instanceof Error ? e.message : String(e);
      console.error("failed to get page sizes", e);
    } finally {
      loadingPages = false;
    }
  }

  onMount(() => {
    void loadPages();
  });

  $effect(() => {
    if (!container) return;
    const root = container;
    const ro = new ResizeObserver(() => {
      // clientWidth/clientHeight describe the real scrollport and exclude any
      // classic scrollbar space. That is the width the fitted page must obey.
      const width = root.clientWidth;
      const height = root.clientHeight;
      if (vstore.pageSizes.length === 0) vstore.setContainerSize(width, height);
      else scheduleContainerResize(width, height);
    });
    ro.observe(root);
    if (vstore.pageSizes.length === 0) {
      vstore.setContainerSize(container.clientWidth, container.clientHeight);
    } else {
      scheduleContainerResize(container.clientWidth, container.clientHeight);
    }
    return () => {
      ro.disconnect();
      pendingContainerSize = undefined;
      if (resizeAnchorFrame !== undefined) cancelAnimationFrame(resizeAnchorFrame);
      resizeAnchorFrame = undefined;
    };
  });

  $effect(() => {
    const root = container;
    if (!root || scrollRestored || vstore.pageSizes.length === 0) return;
    scrollRestored = true;
    const frame = requestAnimationFrame(() => {
      root.scrollTo({ left: vstore.scrollLeft, top: vstore.scrollTop });
    });
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    const pages = vstore.pageSizes;
    const root = container;
    if (!pages.length || !root) return;

    const nearby = new Set<number>();
    const onscreen = new Set<number>();

    const updateCurrentPage = () => {
      viewportSet = new Set(onscreen);
      if (currentPageLock !== undefined) {
        vstore.setCurrentPage(currentPageLock);
        return;
      }
      const rootRect = root.getBoundingClientRect();
      const probeY = rootRect.top + Math.min(240, rootRect.height * 0.35);
      const candidates = onscreen.size > 0 ? onscreen : nearby;
      let bestIndex: number | undefined;
      let bestDistance = Number.POSITIVE_INFINITY;

      for (const idx of candidates) {
        const pageEl = root.querySelector<HTMLElement>(`[data-page-index="${idx}"]`);
        if (!pageEl) continue;
        const rect = pageEl.getBoundingClientRect();
        const distance = probeY < rect.top
          ? rect.top - probeY
          : probeY > rect.bottom
            ? probeY - rect.bottom
            : 0;
        if (distance < bestDistance) {
          bestDistance = distance;
          bestIndex = idx;
        }
      }

      if (bestIndex !== undefined) vstore.setCurrentPage(bestIndex);
    };

    const renderObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const idx = Number((entry.target as HTMLElement).dataset.pageIndex);
          if (entry.isIntersecting) nearby.add(idx);
          else nearby.delete(idx);
        }
        renderSet = new Set([...nearby, ...onscreen]);
      },
      // Keep roughly two screens ready so fast scrolling still feels like one
      // continuous document rather than a sequence of pages loading on demand.
      { root, rootMargin: `${Math.max(800, root.clientHeight * 1.5)}px 0px`, threshold: 0 }
    );

    const viewportObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const idx = Number((entry.target as HTMLElement).dataset.pageIndex);
          if (entry.isIntersecting) onscreen.add(idx);
          else onscreen.delete(idx);
        }
        renderSet = new Set([...nearby, ...onscreen]);
        updateCurrentPage();
      },
      { root, threshold: [0, 0.1, 0.25, 0.5, 0.75, 1] },
    );

    const frame = requestAnimationFrame(() => {
      root.querySelectorAll<HTMLElement>("[data-page-index]").forEach((el) => {
        renderObserver.observe(el);
        viewportObserver.observe(el);
      });
    });

    return () => {
      cancelAnimationFrame(frame);
      renderObserver.disconnect();
      viewportObserver.disconnect();
    };
  });

  // ── Keyboard ──────────────────────────────────────────────────────────────────
  async function onKeyDown(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    const isEditing = target instanceof HTMLInputElement
      || target instanceof HTMLTextAreaElement
      || target instanceof HTMLSelectElement
      || target?.isContentEditable;

    if (e.ctrlKey) {
      if (e.key === "f" || e.key === "F") { e.preventDefault(); openFindBar(); return; }
      if (e.key === "s" || e.key === "S") { e.preventDefault(); await handleSave(); return; }
      if (!isEditing && (e.key === "z" || e.key === "Z")) { e.preventDefault(); await handleUndo(); return; }
      if (e.key === "=" || e.key === "+") { e.preventDefault(); await zoomIn(); return; }
      if (e.key === "-")                  { e.preventDefault(); await zoomOut(); return; }
      if (e.key === "0")                  { e.preventDefault(); await fitWidth(); return; }
    }

    if (!isEditing && container) {
      if (e.key === "PageDown") {
        e.preventDefault();
        container.scrollBy({ top: Math.max(120, container.clientHeight - 64), behavior: "smooth" });
        return;
      }
      if (e.key === "PageUp") {
        e.preventDefault();
        container.scrollBy({ top: -Math.max(120, container.clientHeight - 64), behavior: "smooth" });
        return;
      }
      if (e.key === "Home") { e.preventDefault(); container.scrollTo({ top: 0, behavior: "smooth" }); return; }
      if (e.key === "End") { e.preventDefault(); container.scrollTo({ top: container.scrollHeight, behavior: "smooth" }); return; }
    }

    if (e.key === "Escape") {
      if (findOpen) { e.preventDefault(); closeFindBar(); }
      else if (markupOpen) { markupOpen = false; }
      else if (activeTool !== "none") { activeTool = "none"; }
    }
  }

  function onFindKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") { e.preventDefault(); e.shiftKey ? prevMatch() : nextMatch(); }
    else if (e.key === "Escape") { e.preventDefault(); closeFindBar(); }
  }

  type ZoomAnchorStrategy = "point" | "page";

  async function nextAnimationFrame() {
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }

  async function withContainerResizeAnchor(width: number, height: number) {
    const nextWidth = Math.max(1, width);
    const nextHeight = Math.max(1, height);
    if (
      Math.abs(vstore.containerWidth - nextWidth) < 0.5
      && Math.abs(vstore.containerHeight - nextHeight) < 0.5
    ) return;

    const root = container;
    const lockedPage = vstore.currentPage;
    const entry = root?.querySelector<HTMLElement>(`[data-page-index="${lockedPage}"]`);
    const before = entry?.getBoundingClientRect();
    const rootBefore = root?.getBoundingClientRect();
    const modeBefore = vstore.zoomMode;

    if (!root || !entry || !before || !rootBefore || modeBefore === "custom") {
      vstore.setContainerSize(nextWidth, nextHeight);
      return;
    }

    const visibleTop = Math.max(before.top, rootBefore.top);
    const visibleBottom = Math.min(before.bottom, rootBefore.bottom);
    const anchorY = visibleBottom > visibleTop
      ? (visibleTop + visibleBottom) / 2
      : rootBefore.top + rootBefore.height / 2;
    const relativeY = Math.max(0, Math.min(1, (anchorY - before.top) / Math.max(1, before.height)));
    const viewportY = Math.max(0, Math.min(1, (anchorY - rootBefore.top) / Math.max(1, rootBefore.height)));

    currentPageLock = lockedPage;
    try {
      vstore.setContainerSize(nextWidth, nextHeight);
      await tick();
      await nextAnimationFrame();

      if (container !== root) return;
      const updated = root.querySelector<HTMLElement>(`[data-page-index="${lockedPage}"]`);
      const after = updated?.getBoundingClientRect();
      const rootAfter = root.getBoundingClientRect();
      if (!after) return;

      const desiredY = modeBefore === "fit-page"
        ? rootAfter.top + rootAfter.height / 2
        : rootAfter.top + rootAfter.height * viewportY;
      const anchoredY = modeBefore === "fit-page"
        ? after.top + after.height / 2
        : after.top + after.height * relativeY;

      root.scrollTop += anchoredY - desiredY;
      // Fitted pages are centered by layout and must never retain a horizontal
      // offset from an earlier custom-zoom state.
      root.scrollLeft = 0;
      vstore.setCurrentPage(lockedPage);
      vstore.setScrollPosition(root.scrollLeft, root.scrollTop);
      await nextAnimationFrame();
    } finally {
      if (currentPageLock === lockedPage) {
        currentPageLock = undefined;
        vstore.setCurrentPage(lockedPage);
      }
    }
  }

  function scheduleContainerResize(width: number, height: number) {
    pendingContainerSize = { width, height };
    if (resizeAnchorRunning || resizeAnchorFrame !== undefined) return;

    resizeAnchorFrame = requestAnimationFrame(() => {
      resizeAnchorFrame = undefined;
      const requested = pendingContainerSize;
      pendingContainerSize = undefined;
      if (!requested) return;

      resizeAnchorRunning = true;
      const operation = zoomAnchorQueue
        .catch(() => undefined)
        .then(() => {
          const latest = pendingContainerSize ?? requested;
          pendingContainerSize = undefined;
          return withContainerResizeAnchor(latest.width, latest.height);
        });
      zoomAnchorQueue = operation;

      const finish = () => {
        resizeAnchorRunning = false;
        if (pendingContainerSize && resizeAnchorFrame === undefined) {
          scheduleContainerResize(pendingContainerSize.width, pendingContainerSize.height);
        }
      };
      void operation.then(finish, (error) => {
        console.error("failed to preserve the page position after resizing", error);
        finish();
      });
    });
  }

  async function withZoomAnchor(
    change: () => void,
    clientX?: number,
    clientY?: number,
    strategy: ZoomAnchorStrategy = "point",
  ) {
    const root = container;
    if (!root) { change(); return; }

    const rootRect = root.getBoundingClientRect();
    const anchorX = clientX ?? rootRect.left + rootRect.width / 2;
    const anchorY = clientY ?? rootRect.top + rootRect.height / 2;
    const pointed = document.elementFromPoint(anchorX, anchorY)?.closest<HTMLElement>("[data-page-index]");
    const currentEntry = root.querySelector<HTMLElement>(`[data-page-index="${vstore.currentPage}"]`);
    const entry = strategy === "page"
      ? currentEntry
      : pointed && root.contains(pointed)
        ? pointed
        : currentEntry;
    const pageIndex = entry?.dataset.pageIndex;
    const lockedPage = pageIndex === undefined ? undefined : Number(pageIndex);
    const before = entry?.getBoundingClientRect();
    const relativeX = before ? (anchorX - before.left) / Math.max(1, before.width) : 0.5;
    const relativeY = before ? (anchorY - before.top) / Math.max(1, before.height) : 0.5;

    if (strategy === "page" && Number.isInteger(lockedPage)) currentPageLock = lockedPage;
    try {
      change();
      await tick();
      await nextAnimationFrame();

      if (pageIndex === undefined || !before) return;
      const updated = root.querySelector<HTMLElement>(`[data-page-index="${pageIndex}"]`);
      const after = updated?.getBoundingClientRect();
      if (!after) return;

      if (strategy === "page") {
        // Fit Page is intentionally centered. Fit Width remains top-aligned so
        // the pages read as one continuous vertical stream.
        const updatedRootRect = root.getBoundingClientRect();
        const verticalInset = vstore.zoomMode === "fit-page"
          ? Math.max(16, (updatedRootRect.height - after.height) / 2)
          : 16;
        root.scrollTop += after.top - (updatedRootRect.top + verticalInset);
        if (vstore.zoomMode !== "custom" || root.scrollWidth <= root.clientWidth + 1) {
          root.scrollLeft = 0;
        } else {
          root.scrollLeft += after.left + after.width / 2
            - (updatedRootRect.left + updatedRootRect.width / 2);
        }
        if (lockedPage !== undefined) vstore.setCurrentPage(lockedPage);
        await nextAnimationFrame();
      } else {
        if (vstore.zoomMode !== "custom" || root.scrollWidth <= root.clientWidth + 1) {
          root.scrollLeft = 0;
        } else {
          root.scrollLeft += after.left + relativeX * after.width - anchorX;
        }
        root.scrollTop += after.top + relativeY * after.height - anchorY;
      }
    } finally {
      if (strategy === "page" && currentPageLock === lockedPage) {
        currentPageLock = undefined;
        if (lockedPage !== undefined) vstore.setCurrentPage(lockedPage);
      }
    }
  }

  let zoomAnchorQueue: Promise<void> = Promise.resolve();
  function queueZoomAnchor(
    change: () => void,
    clientX?: number,
    clientY?: number,
    strategy: ZoomAnchorStrategy = "point",
  ): Promise<void> {
    const operation = zoomAnchorQueue
      .catch(() => undefined)
      .then(() => withZoomAnchor(change, clientX, clientY, strategy));
    zoomAnchorQueue = operation;
    return operation;
  }

  async function zoomIn() { await queueZoomAnchor(() => vstore.zoomIn()); }
  async function zoomOut() { await queueZoomAnchor(() => vstore.zoomOut()); }
  async function actualSize() { await queueZoomAnchor(() => vstore.setZoom(1)); }
  async function fitWidth() { await queueZoomAnchor(() => vstore.setZoomMode("fit-width"), undefined, undefined, "page"); }
  async function fitPage() { await queueZoomAnchor(() => vstore.setZoomMode("fit-page"), undefined, undefined, "page"); }
  async function rotateLeft() { await queueZoomAnchor(() => vstore.rotateCcw()); }
  async function rotateRight() { await queueZoomAnchor(() => vstore.rotateCw()); }

  let wheelZoomTimer: ReturnType<typeof setTimeout> | undefined;
  let wheelZoomDirection: -1 | 1 = 1;
  let wheelAnchorX = 0;
  let wheelAnchorY = 0;
  onDestroy(() => {
    if (wheelZoomTimer !== undefined) clearTimeout(wheelZoomTimer);
  });

  function onWheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    wheelZoomDirection = e.deltaY < 0 ? -1 : 1;
    wheelAnchorX = e.clientX;
    wheelAnchorY = e.clientY;
    if (wheelZoomTimer !== undefined) clearTimeout(wheelZoomTimer);
    wheelZoomTimer = setTimeout(() => {
      wheelZoomTimer = undefined;
      const direction = wheelZoomDirection;
      void queueZoomAnchor(
        () => { if (direction < 0) vstore.zoomIn(); else vstore.zoomOut(); },
        wheelAnchorX,
        wheelAnchorY,
      );
    }, 45);
  }

  function onViewerScroll() {
    if (container) vstore.setScrollPosition(container.scrollLeft, container.scrollTop);
  }

  function onPageInput(e: Event) {
    const val = parseInt((e.target as HTMLInputElement).value, 10) - 1;
    if (val >= 0 && val < vstore.pageSizes.length) {
      container?.querySelector<HTMLElement>(`[data-page-index="${val}"]`)
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }

  async function handleSave() {
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && focused.closest(".viewer") && focused.matches("input, textarea, select")) {
      focused.blur();
      await tick();
    }
    if (!await flushFormWrites()) return;

    const flushedSequence = formWriteSequence;
    try {
      await saveDocument(docId);
      // An edit queued while the backend save was in flight was not
      // necessarily included. Keep the tab dirty and require another save.
      if (flushedSequence !== formWriteSequence || failedFormWrites.size > 0) {
        syncFormWriteError();
        return;
      }
      tabs.markDirty(tab.id, false);
    } catch (error) {
      formWriteError = describeError(error);
    }
  }

  async function handleUndo() {
    const pageIndex = await undoAnnotation(docId);
    if (pageIndex !== null) await refreshAnnotations(pageIndex);
  }

  const zoomPct = $derived(Math.round(vstore.effectiveZoom * 100));

  const TOOL_LABELS: Record<AnnotTool, string> = {
    none: "No tool", highlight: "Highlight", underline: "Underline",
    strikeout: "Strikethrough", text: "Sticky note", ink: "Freehand",
  };

  function toolIcon(tool: Exclude<AnnotTool, "none">): "highlight" | "underline" | "strikeout" | "note" | "ink" {
    return tool === "text" ? "note" : tool;
  }

  const noResults = $derived(findOpen && findQuery.trim().length > 0 && findMatches.length === 0);
</script>

<svelte:window onkeydown={onKeyDown} />

<section
  class="viewer"
  class:xfa-active={formType === "xfa_full" || formType === "xfa_foreground"}
  aria-label="PDF viewer"
>
  <!-- ── Toolbar ── -->
  <div class="toolbar">
    <div class="toolbar-left">
      {#if vstore.pageSizes.length > 0}
        <div class="control-group page-nav" aria-label="Page navigation">
          <span class="sr-only">Page</span>
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

    <div class="toolbar-center" aria-label="Zoom controls">
      <div class="control-group">
        <button onclick={zoomOut} title="Zoom out (Ctrl+-)" aria-label="Zoom out"><Icon name="minus" /></button>
        <button
          class="zoom-pct-btn"
          onclick={actualSize}
          title="Actual size (100%)"
          aria-label="Zoom level {zoomPct} percent; reset to actual size"
        >{zoomPct}%</button>
        <button onclick={zoomIn} title="Zoom in (Ctrl++)" aria-label="Zoom in"><Icon name="plus" /></button>
      </div>
      <div class="control-group view-presets">
        <button
          class:active={vstore.zoomMode === "fit-width"}
          onclick={fitWidth}
          title="Fit width (Ctrl+0)"
          aria-label="Fit width"
        ><Icon name="fit-width" /></button>
        <button
          class:active={vstore.zoomMode === "fit-page"}
          onclick={fitPage}
          title="Fit page"
          aria-label="Fit page"
        ><Icon name="fit-page" /></button>
      </div>
    </div>

    <div class="toolbar-right" aria-label="Document tools">
      <div class="control-group rotate-group">
        <button onclick={rotateLeft} title="Rotate left" aria-label="Rotate left"><Icon name="rotate-left" /></button>
        <button onclick={rotateRight} title="Rotate right" aria-label="Rotate right"><Icon name="rotate-right" /></button>
      </div>

      <span class="sep" aria-hidden="true"></span>

      <div class="markup-menu">
        <button
          class:active={activeTool !== "none" || markupOpen}
          onclick={() => { markupOpen = !markupOpen; }}
          title="Markup tools"
          aria-label="Markup tools"
          aria-expanded={markupOpen}
        >
          <Icon name={activeTool === "none" ? "highlight" : toolIcon(activeTool)} />
          <span class="button-label">Markup</span>
        </button>

        {#if markupOpen}
          <div class="markup-popover" role="toolbar" aria-label="Markup tools">
            <div class="markup-tools">
              {#each (["highlight","underline","strikeout","text","ink"] as const) as tool}
                <button
                  class:active={activeTool === tool}
                  onclick={() => chooseTool(tool)}
                  title={TOOL_LABELS[tool]}
                  aria-pressed={activeTool === tool}
                  aria-label={TOOL_LABELS[tool]}
                ><Icon name={toolIcon(tool)} /></button>
              {/each}
            </div>
            <label class="color-control">
              <span>Color</span>
              <input
                type="color"
                value="#{toolColor.map((c) => c.toString(16).padStart(2, "0")).join("")}"
                aria-label="Annotation color"
                oninput={(e) => {
                  const v = (e.target as HTMLInputElement).value.slice(1);
                  toolColor = [parseInt(v.slice(0,2),16), parseInt(v.slice(2,4),16), parseInt(v.slice(4,6),16)];
                }}
              />
            </label>
          </div>
        {/if}
      </div>

      <button
        class:active={findOpen}
        onclick={openFindBar}
        title="Find (Ctrl+F)"
        aria-label="Find in document"
      ><Icon name="search" /></button>
      <button
        class:active={sidebarOpen}
        onclick={() => { sidebarOpen = !sidebarOpen; }}
        title="Comments"
        aria-label="Toggle comments"
      ><Icon name="comments" /></button>
      <button onclick={() => { signOpen = true; }} title="Sign" aria-label="Sign document"><Icon name="signature" /></button>
      <button
        onclick={handleSave}
        title="Save (Ctrl+S)"
        class="save-btn"
        disabled={!tab.dirty}
        aria-label={tab.dirty ? "Save document" : "Document is saved"}
      ><Icon name="save" /></button>
    </div>
  </div>

  <!-- ── XFA warning ── -->
  {#if formType === "xfa_full" || formType === "xfa_foreground"}
    <div class="xfa-banner" role="alert">
      ⚠ This PDF uses XFA forms — not fully supported. Fields shown read-only.
    </div>
  {/if}

  {#if formWriteError}
    <div class="form-error-banner" role="alert">
      A form change could not be written. Save was stopped: {formWriteError}
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
          {:else if findIndexing}
            Searching…
          {:else}
            No results
          {/if}
        {:else if findIndexing}
          Indexing…
        {/if}
      </span>
      <button onclick={prevMatch} disabled={findMatches.length === 0} title="Previous (Shift+Enter)" aria-label="Previous match"><Icon name="chevron-up" /></button>
      <button onclick={nextMatch} disabled={findMatches.length === 0} title="Next (Enter)" aria-label="Next match"><Icon name="chevron-down" /></button>
      <button onclick={closeFindBar} title="Close (Escape)" aria-label="Close find bar"><Icon name="close" /></button>
    </div>
  {/if}

  <!-- ── Main content ── -->
  <div class="content-row">
    <!-- Pages scroll area -->
    <div
      class="pages-area"
      class:width-fitted={vstore.zoomMode !== "custom"}
      bind:this={container}
      onwheel={onWheel}
      onscroll={onViewerScroll}
      role="document"
      tabindex="-1"
    >
      {#if loadingPages}
        <div class="center-msg" aria-live="polite"><span class="loading-ring" aria-hidden="true"></span>Rendering document…</div>
      {:else if pagesError}
        <div class="center-msg error-state">
          <strong>Could not open this document</strong>
          <span>{pagesError}</span>
          <button type="button" onclick={loadPages}>Try again</button>
        </div>
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
                zoom={vstore.zoomForPage(i)}
                visible={renderSet.has(i)}
                priority={viewportSet.has(i)}
                rotation={vstore.rotation}
                textSpans={textSpansByPage[i]}
                highlights={pageMatches}
                activeHighlight={activeIdx}
                annotations={annotsByPage[i]}
                annotationsVersion={annotsVersionByPage[i] ?? 0}
                formFields={formFieldsByPage[i]}
                xfaReadOnly={formType === "xfa_full" || formType === "xfa_foreground"}
                activeTool={activeTool}
                onPageClick={(left, top) => handlePageClick(i, left, top)}
                onTextSelected={(rects) => handleTextSelection(i, rects)}
                onInkStroke={(paths) => handleInkStroke(i, paths)}
                onDeleteAnnotation={(idx) => handleDeleteAnnotation(i, idx)}
                onFieldText={(annotIdx, val) => handleFieldText(i, annotIdx, val)}
                onFieldChecked={(annotIdx, val) => handleFieldChecked(i, annotIdx, val)}
                onPushButton={(annotIdx) => {
                    const field = formFieldsByPage[i]?.find(f => f.index === annotIdx);
                    handlePushButton(i, field?.action_type ?? "reset");
                  }}
                inkColor={toolColor}
                {inkWidth}
              />
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
          <button onclick={() => { sidebarOpen = false; }} aria-label="Close sidebar"><Icon name="close" /></button>
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
  .viewer {
    position: relative; display: flex; flex-direction: column;
    height: 100%; overflow: hidden; background: var(--viewer-bg);
  }

  .sr-only {
    position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
    overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0;
  }

  /* ── Compact reader toolbar ── */
  .toolbar {
    z-index: 40; display: grid; grid-template-columns: minmax(90px, 1fr) auto minmax(260px, 1fr);
    align-items: center; gap: 10px; min-height: 48px; padding: 6px 10px;
    flex-shrink: 0; background: color-mix(in srgb, var(--bg-elev) 94%, transparent);
    border-bottom: 1px solid var(--border-subtle); box-shadow: var(--shadow-chrome);
  }

  .toolbar-left { display: flex; min-width: 0; justify-content: flex-start; }
  .toolbar-center { display: flex; align-items: center; justify-content: center; gap: 6px; }
  .toolbar-right { display: flex; align-items: center; justify-content: flex-end; gap: 4px; min-width: 0; }

  .control-group {
    display: inline-flex; align-items: center; gap: 2px; min-height: 34px; padding: 2px;
    border: 1px solid var(--border-subtle); border-radius: 9px; background: var(--control-bg);
  }

  .toolbar button,
  .find-bar button {
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    width: 30px; height: 30px; padding: 0; flex-shrink: 0;
    border: 0; border-radius: 7px; background: transparent; color: var(--fg-muted);
    cursor: pointer; line-height: 1; transition: background 100ms ease, color 100ms ease;
  }
  .toolbar button:hover:not(:disabled),
  .find-bar button:hover:not(:disabled) { background: var(--control-hover); color: var(--fg); }
  .toolbar button:disabled,
  .find-bar button:disabled { opacity: 0.32; cursor: default; }
  .toolbar button.active { background: var(--accent-soft); color: var(--accent-strong); }

  .page-nav { gap: 3px; padding-inline: 7px; font-size: 12px; color: var(--fg-muted); }
  .page-nav input {
    width: 36px; padding: 2px 1px; border: 0; outline: none; background: transparent;
    color: var(--fg); text-align: right; font: inherit; font-size: 12px; font-variant-numeric: tabular-nums;
    appearance: textfield;
  }
  .page-nav input::-webkit-inner-spin-button,
  .page-nav input::-webkit-outer-spin-button { appearance: none; margin: 0; }
  .page-nav input:focus { border-radius: 4px; box-shadow: inset 0 0 0 1px var(--accent); }
  .page-total { white-space: nowrap; font-variant-numeric: tabular-nums; }

  .zoom-pct-btn { width: 54px !important; font-size: 12px; font-variant-numeric: tabular-nums; color: var(--fg) !important; }
  .view-presets { margin-left: 2px; }
  .sep { width: 1px; height: 20px; margin: 0 3px; flex-shrink: 0; background: var(--border); }
  .save-btn:not(:disabled) { color: var(--accent-strong) !important; }

  .markup-menu { position: relative; display: flex; }
  .markup-menu > button { width: auto; min-width: 34px; padding: 0 8px; }
  .button-label { font-size: 12px; font-weight: 550; }
  .markup-popover {
    position: absolute; z-index: 90; top: calc(100% + 10px); right: 0;
    min-width: 220px; padding: 8px; border: 1px solid var(--border);
    border-radius: 12px; background: var(--bg-elev); box-shadow: var(--shadow-lg);
  }
  .markup-tools { display: flex; align-items: center; gap: 4px; }
  .markup-tools button { width: 36px; height: 36px; }
  .color-control {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    margin-top: 8px; padding: 7px 8px 2px; border-top: 1px solid var(--border-subtle);
    color: var(--fg-muted); font-size: 12px;
  }
  .color-control input {
    width: 28px; height: 24px; padding: 0; border: 1px solid var(--border);
    border-radius: 6px; background: transparent; cursor: pointer;
  }

  /* ── Notices and floating find ── */
  .xfa-banner {
    z-index: 30; padding: 7px 16px; flex-shrink: 0;
    border-bottom: 1px solid #e4b341; background: #fff4ce; color: #654b00; font-size: 12px;
  }
  :global(:root[data-theme="dark"]) .xfa-banner { background: #3b310e; color: #f2d77c; border-color: #67551b; }
  .form-error-banner {
    z-index: 30; padding: 7px 16px; flex-shrink: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--danger) 55%, var(--border));
    background: color-mix(in srgb, var(--danger) 10%, var(--bg-elev));
    color: var(--fg); font-size: 12px;
  }

  .find-bar {
    position: absolute; z-index: 70; top: 58px; right: 14px;
    display: flex; align-items: center; gap: 3px; width: min(430px, calc(100% - 28px));
    padding: 7px; border: 1px solid var(--border); border-radius: 11px;
    background: var(--bg-elev); box-shadow: var(--shadow-lg);
  }
  .viewer.xfa-active .find-bar { top: 88px; }
  .find-bar input[type="search"] {
    min-width: 80px; flex: 1; padding: 6px 8px; border: 0; border-radius: 6px;
    outline: none; background: transparent; color: var(--fg); font: inherit; font-size: 13px;
  }
  .find-bar input[type="search"]:focus { background: var(--control-bg); }
  .find-bar input.no-results { color: var(--danger); }
  .find-bar input[type="search"]::-webkit-search-cancel-button { display: none; }
  .find-count {
    min-width: 72px; color: var(--fg-muted); text-align: center;
    white-space: nowrap; font-size: 11px; font-variant-numeric: tabular-nums;
  }

  /* ── Document canvas ── */
  .content-row { min-height: 0; flex: 1; display: flex; overflow: hidden; }
  .pages-area {
    min-width: 0; flex: 1; overflow-x: auto; overflow-y: auto; outline: none; background: var(--viewer-bg);
    scrollbar-color: var(--scroll-thumb) transparent; scrollbar-width: thin;
    scrollbar-gutter: stable; overscroll-behavior: contain;
  }
  .pages-area.width-fitted { overflow-x: hidden; }
  .pages-list {
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    width: max-content; min-width: 100%; padding: 20px 0 64px;
  }
  .page-entry { position: relative; flex: 0 0 auto; }

  .center-msg {
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px;
    height: 100%; min-height: 240px; padding: 32px; color: var(--fg-muted); font-size: 13px; text-align: center;
  }
  .center-msg strong { color: var(--fg); font-size: 15px; font-weight: 600; }
  .center-msg span { max-width: 540px; overflow-wrap: anywhere; }
  .center-msg button {
    padding: 7px 13px; border: 1px solid var(--border); border-radius: 7px;
    background: var(--bg-elev); color: var(--fg); cursor: pointer;
  }
  .center-msg button:hover { border-color: var(--accent); }
  .loading-ring {
    width: 22px; height: 22px; border: 2px solid var(--border);
    border-top-color: var(--accent); border-radius: 50%; animation: reader-spin 700ms linear infinite;
  }
  @keyframes reader-spin { to { transform: rotate(360deg); } }

  /* ── Comments panel ── */
  .sidebar {
    width: min(300px, 36vw); flex-shrink: 0; display: flex; flex-direction: column; overflow: hidden;
    border-left: 1px solid var(--border-subtle); background: var(--bg-elev); box-shadow: -8px 0 24px rgba(0, 0, 0, 0.035);
  }
  .sidebar-header {
    display: flex; align-items: center; justify-content: space-between; min-height: 48px;
    padding: 8px 12px 8px 16px; border-bottom: 1px solid var(--border-subtle);
    font-size: 13px; font-weight: 600;
  }
  .sidebar-header button {
    display: inline-flex; align-items: center; justify-content: center; width: 30px; height: 30px;
    padding: 0; border: 0; border-radius: 7px; background: transparent; color: var(--fg-muted); cursor: pointer;
  }
  .sidebar-header button:hover { background: var(--control-hover); color: var(--fg); }
  .sidebar-list { flex: 1; overflow-y: auto; padding: 10px; }
  .sidebar-empty { padding: 32px 8px; color: var(--fg-muted); text-align: center; font-size: 12px; }
  .sidebar-item {
    margin-bottom: 7px; padding: 10px 11px; border: 1px solid var(--border-subtle);
    border-radius: 9px; background: var(--control-bg); cursor: pointer;
    transition: border-color 100ms ease, background 100ms ease;
  }
  .sidebar-item:hover { border-color: var(--border); background: var(--control-hover); }
  .sidebar-kind { color: var(--accent-strong); font-size: 10px; font-weight: 700; letter-spacing: .05em; text-transform: uppercase; }
  .sidebar-page { float: right; color: var(--fg-muted); font-size: 10px; }
  .sidebar-text { margin: 5px 0 0; color: var(--fg); font-size: 12px; line-height: 1.45; }
  .sidebar-author { margin: 3px 0 0; color: var(--fg-muted); font-size: 10px; }

  @media (max-width: 900px) {
    .toolbar { grid-template-columns: auto 1fr auto; gap: 6px; }
    .button-label { display: none; }
    .markup-menu > button { width: 30px; padding: 0; }
  }

  @media (max-width: 740px) {
    .toolbar { padding-inline: 6px; }
    .sep { display: none; }
    .sidebar { position: absolute; z-index: 60; inset: 48px 0 0 auto; width: min(300px, 86vw); }
  }
</style>
