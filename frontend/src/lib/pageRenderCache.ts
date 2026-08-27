import { renderPagePixels } from "./ipc";

export interface PageFrame {
  width: number;
  height: number;
  data: Uint8ClampedArray<ArrayBuffer>;
}

interface RenderRequest {
  docId: string;
  pageIndex: number;
  scale: number;
  version: number;
  /** 0 = in the viewport, 1 = speculative prefetch. */
  priority: 0 | 1;
}

interface CachedFrame {
  frame: PageFrame;
  bytes: number;
}

interface RenderJob {
  key: string;
  request: RenderRequest;
  priority: 0 | 1;
  sequence: number;
  started: boolean;
  promise: Promise<PageFrame>;
  resolve: (frame: PageFrame) => void;
  reject: (error: unknown) => void;
  consumers: Map<symbol, 0 | 1>;
}

export interface PageFrameRequest {
  promise: Promise<PageFrame>;
  /** Removes this consumer and drops the job if PDFium has not started it. */
  cancel: () => void;
}

const MIB = 1024 * 1024;
const CACHE_BUDGET_BYTES = (() => {
  // WebView2 exposes this on some Windows versions. Keep a conservative
  // fallback so the renderer never assumes a high-memory machine.
  const memoryGb = typeof navigator === "undefined"
    ? 4
    : (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 4;
  if (memoryGb <= 4) return 48 * MIB;
  if (memoryGb <= 8) return 64 * MIB;
  return 96 * MIB;
})();

const cache = new Map<string, CachedFrame>();
const jobs = new Map<string, RenderJob>();
const closedDocuments = new Set<string>();
const closedCleanupTimers = new Map<string, ReturnType<typeof setTimeout>>();
let cacheBytes = 0;
let nextSequence = 0;
let processing = false;

function renderKey(request: Omit<RenderRequest, "priority">): string {
  return `${request.docId}:${request.pageIndex}:${request.scale.toFixed(5)}:${request.version}`;
}

function readCache(key: string): PageFrame | undefined {
  const cached = cache.get(key);
  if (!cached) return undefined;
  // Map insertion order doubles as the LRU list.
  cache.delete(key);
  cache.set(key, cached);
  return cached.frame;
}

function trimCacheTo(byteLimit: number) {
  while (cacheBytes > byteLimit && cache.size > 0) {
    const oldestKey = cache.keys().next().value as string | undefined;
    if (!oldestKey) break;
    const oldest = cache.get(oldestKey);
    cache.delete(oldestKey);
    cacheBytes -= oldest?.bytes ?? 0;
  }
}

function writeCache(key: string, frame: PageFrame) {
  const previous = cache.get(key);
  if (previous) cacheBytes -= previous.bytes;
  cache.delete(key);
  const bytes = frame.data.byteLength;
  cache.set(key, { frame, bytes });
  cacheBytes += bytes;
  trimCacheTo(CACHE_BUDGET_BYTES);
}

function nextJob(): RenderJob | undefined {
  let selected: RenderJob | undefined;
  for (const job of jobs.values()) {
    if (job.started) continue;
    if (
      !selected
      || job.priority < selected.priority
      || (job.priority === selected.priority && job.sequence < selected.sequence)
    ) selected = job;
  }
  return selected;
}

function hasDocumentJobs(docId: string): boolean {
  for (const job of jobs.values()) {
    if (job.request.docId === docId) return true;
  }
  return false;
}

function scheduleClosedDocumentCleanup(docId: string) {
  if (!closedDocuments.has(docId) || hasDocumentJobs(docId) || closedCleanupTimers.has(docId)) return;
  const timer = setTimeout(() => {
    closedCleanupTimers.delete(docId);
    if (!hasDocumentJobs(docId)) closedDocuments.delete(docId);
  }, 1_000);
  closedCleanupTimers.set(docId, timer);
}

async function processQueue() {
  if (processing) return;
  processing = true;
  try {
    while (true) {
      const job = nextJob();
      if (!job) break;
      job.started = true;

      if (closedDocuments.has(job.request.docId)) {
        jobs.delete(job.key);
        job.reject(new Error("Document was closed before the page could render"));
        scheduleClosedDocumentCleanup(job.request.docId);
        continue;
      }

      try {
        // PDFium serializes access to a document. One frontend render at a time
        // avoids flooding its lock with stale prefetch work and lets newly
        // visible pages move ahead of jobs that have not started yet.
        const frame = await renderPagePixels(
          job.request.docId,
          job.request.pageIndex,
          job.request.scale,
        );
        if (job.consumers.size > 0 && !closedDocuments.has(job.request.docId)) {
          writeCache(job.key, frame);
        }
        jobs.delete(job.key);
        job.resolve(frame);
        scheduleClosedDocumentCleanup(job.request.docId);
      } catch (error) {
        jobs.delete(job.key);
        job.reject(error);
        scheduleClosedDocumentCleanup(job.request.docId);
      }
    }
  } finally {
    processing = false;
    // A request can be queued between the last nextJob() and this finally.
    if (nextJob()) queueMicrotask(() => void processQueue());
  }
}

/**
 * Return a retained page frame or enqueue one render. Duplicate requests share
 * the same promise, and an on-screen request promotes pending prefetch work.
 */
export function requestPageFrame(request: RenderRequest): PageFrameRequest {
  const key = renderKey(request);
  const cached = readCache(key);
  if (cached) return { promise: Promise.resolve(cached), cancel: () => undefined };

  const consumer = Symbol(key);

  const existing = jobs.get(key);
  if (existing) {
    existing.consumers.set(consumer, request.priority);
    existing.priority = Math.min(...existing.consumers.values()) as 0 | 1;
    return { promise: existing.promise, cancel: () => cancelConsumer(key, consumer) };
  }

  let resolve!: (frame: PageFrame) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<PageFrame>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  const job: RenderJob = {
    key,
    request,
    priority: request.priority,
    sequence: nextSequence++,
    started: false,
    promise,
    resolve,
    reject,
    consumers: new Map([[consumer, request.priority]]),
  };
  jobs.set(key, job);
  queueMicrotask(() => void processQueue());
  return { promise, cancel: () => cancelConsumer(key, consumer) };
}

function cancelConsumer(key: string, consumer: symbol) {
  const job = jobs.get(key);
  if (!job || !job.consumers.delete(consumer)) return;
  if (job.consumers.size > 0) {
    job.priority = Math.min(...job.consumers.values()) as 0 | 1;
    return;
  }
  if (job.started) return; // Tauri invoke cannot be aborted once dispatched.
  jobs.delete(key);
  job.reject(new Error("Page render was cancelled before it started"));
}

/** Release cached frames and skip queued work once a document tab is closed. */
export function clearDocumentFrames(docId: string): void {
  closedDocuments.add(docId);
  for (const [key, cached] of cache) {
    if (!key.startsWith(`${docId}:`)) continue;
    cache.delete(key);
    cacheBytes -= cached.bytes;
  }
  for (const [key, job] of jobs) {
    if (job.request.docId !== docId || job.started) continue;
    jobs.delete(key);
    job.reject(new Error("Document was closed before the page could render"));
  }
  scheduleClosedDocumentCleanup(docId);
}

// Minimized/hidden windows do not need the full hot-page set. This gives
// WebView2 room to return committed pages to Windows under ordinary app use.
if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) trimCacheTo(Math.min(CACHE_BUDGET_BYTES, 32 * MIB));
  });
}
