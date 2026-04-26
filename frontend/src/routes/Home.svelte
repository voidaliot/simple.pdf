<script lang="ts">
  import { recents, type RecentEntry } from "../stores/recents.svelte";
  import { pickAndOpen, pickFolderAndOpen, openFromUrl, openPath } from "../lib/open";
  import { revealInExplorer } from "../lib/ipc";

  let filter = $state("");
  let pasteUrlOpen = $state(false);
  let pasteUrlValue = $state("");
  let pasteUrlError = $state("");
  let pasteUrlLoading = $state(false);

  const sorted = $derived([
    ...recents.entries.filter((e) => e.pinned),
    ...recents.entries.filter((e) => !e.pinned),
  ]);

  const filtered = $derived(
    filter.trim()
      ? sorted.filter(
          (e) =>
            e.title.toLowerCase().includes(filter.toLowerCase()) ||
            e.path.toLowerCase().includes(filter.toLowerCase())
        )
      : sorted
  );

  let contextMenu = $state<{ x: number; y: number; entry: RecentEntry } | null>(null);

  function onContextMenu(e: MouseEvent, entry: RecentEntry) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, entry };
  }

  function closeContext() {
    contextMenu = null;
  }

  async function openPasteUrl() {
    if (!pasteUrlValue.trim()) return;
    pasteUrlLoading = true;
    pasteUrlError = "";
    try {
      await openFromUrl(pasteUrlValue.trim());
      pasteUrlOpen = false;
      pasteUrlValue = "";
    } catch (err: unknown) {
      pasteUrlError = err instanceof Error ? err.message : String(err);
    } finally {
      pasteUrlLoading = false;
    }
  }

  function onPasteUrlKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") openPasteUrl();
    if (e.key === "Escape") { pasteUrlOpen = false; pasteUrlValue = ""; }
  }
</script>

<svelte:window onclick={closeContext} />

<section class="home">
  <header>
    <h1>simple<span class="dot">.</span>pdf</h1>
    <p class="tagline">Fast, small, modern PDF reader.</p>
  </header>

  <div class="actions">
    <button class="primary" onclick={pickAndOpen}>Open file…</button>
    <button onclick={pickFolderAndOpen}>Open folder</button>
    <button onclick={() => { pasteUrlOpen = true; }}>Paste URL</button>
  </div>

  <!-- Paste URL dialog -->
  {#if pasteUrlOpen}
    <div class="url-dialog" role="dialog" aria-label="Open from URL">
      <input
        type="url"
        placeholder="https://example.com/document.pdf"
        bind:value={pasteUrlValue}
        onkeydown={onPasteUrlKeydown}
        aria-label="PDF URL"
      />
      {#if pasteUrlError}
        <p class="url-error">{pasteUrlError}</p>
      {/if}
      <div class="url-actions">
        <button class="primary" onclick={openPasteUrl} disabled={pasteUrlLoading}>
          {pasteUrlLoading ? "Downloading…" : "Open"}
        </button>
        <button onclick={() => { pasteUrlOpen = false; pasteUrlValue = ""; }}>Cancel</button>
      </div>
    </div>
  {/if}

  <div class="filter">
    <input
      type="search"
      placeholder="Search recent files"
      bind:value={filter}
      aria-label="Search recent files"
    />
  </div>

  <section class="recents" aria-label="Recent files">
    {#if recents.entries.length === 0}
      <div class="empty">
        <p>No recent files yet.</p>
        <p class="hint">Open a PDF to get started.</p>
      </div>
    {:else if filtered.length === 0}
      <div class="empty">
        <p>No matches for "{filter}".</p>
      </div>
    {:else}
      <div class="grid">
        {#each filtered as entry (entry.path)}
          <div
            class="card"
            class:pinned={entry.pinned}
            role="button"
            tabindex="0"
            title={entry.path}
            onclick={() => openPath(entry.path).catch(console.error)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ")
                openPath(entry.path).catch(console.error);
            }}
            oncontextmenu={(e) => onContextMenu(e, entry)}
          >
            <div class="thumb" aria-hidden="true">
              {#if entry.thumbnail}
                <img src={entry.thumbnail} alt="" class="thumb-img" />
              {/if}
              {#if entry.pinned}<span class="pin-badge">pinned</span>{/if}
            </div>
            <div class="meta">
              <h3>{entry.title}</h3>
              <p class="path-label">{entry.path}</p>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</section>

<!-- Context menu -->
{#if contextMenu}
  <div
    class="context-menu"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    role="menu"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <button role="menuitem"
      onclick={() => { openPath(contextMenu!.entry.path).catch(console.error); closeContext(); }}>
      Open
    </button>
    <button role="menuitem"
      onclick={() => { recents.togglePin(contextMenu!.entry.path); closeContext(); }}>
      {contextMenu.entry.pinned ? "Unpin" : "Pin to top"}
    </button>
    <button role="menuitem"
      onclick={() => {
        revealInExplorer(contextMenu!.entry.path).catch(console.error);
        closeContext();
      }}>
      Reveal in Explorer
    </button>
    <button role="menuitem"
      onclick={() => {
        navigator.clipboard.writeText(contextMenu!.entry.path).catch(console.error);
        closeContext();
      }}>
      Copy path
    </button>
    <div class="sep" aria-hidden="true"></div>
    <button role="menuitem" class="danger"
      onclick={() => { recents.remove(contextMenu!.entry.path); closeContext(); }}>
      Remove from recents
    </button>
  </div>
{/if}

<style>
  .home {
    height: 100%;
    overflow: auto;
    padding: 48px 64px;
    max-width: 1200px;
    margin: 0 auto;
  }
  header { margin-bottom: 32px; }
  h1 {
    font-size: 48px;
    font-weight: 300;
    margin: 0;
    letter-spacing: -0.02em;
  }
  .dot { color: var(--accent); }
  .tagline { color: var(--fg-muted); margin: 8px 0 0; font-size: 15px; }
  .actions { display: flex; gap: 8px; margin-bottom: 16px; flex-wrap: wrap; }
  .actions button {
    padding: 8px 16px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .actions button:hover:not(:disabled) { border-color: var(--accent); }
  .actions button:disabled { opacity: 0.5; cursor: not-allowed; }
  .actions .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }

  /* Paste-URL inline dialog */
  .url-dialog {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 16px;
    margin-bottom: 16px;
    max-width: 480px;
  }
  .url-dialog input {
    width: 100%;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    margin-bottom: 8px;
    box-sizing: border-box;
  }
  .url-dialog input:focus { outline: 2px solid var(--accent); border-color: transparent; }
  .url-error { color: var(--danger); font-size: 12px; margin: 0 0 8px; }
  .url-actions { display: flex; gap: 8px; }
  .url-actions button {
    padding: 6px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    cursor: pointer;
    font: inherit;
    font-size: 13px;
  }
  .url-actions .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
  .url-actions button:disabled { opacity: 0.5; cursor: not-allowed; }

  .filter { margin-bottom: 24px; }
  .filter input {
    width: 100%;
    max-width: 420px;
    padding: 8px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--fg);
  }
  .recents .empty { padding: 64px 0; color: var(--fg-muted); text-align: center; }
  .recents .empty p { margin: 4px 0; }
  .recents .empty .hint { font-size: 13px; opacity: 0.7; }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 16px;
  }
  .card {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    cursor: pointer;
    transition: border-color 120ms ease, transform 120ms ease;
    user-select: none;
  }
  .card:hover { border-color: var(--accent); transform: translateY(-1px); }
  .card:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .card.pinned { border-color: var(--accent); }

  .thumb {
    position: relative;
    aspect-ratio: 3 / 4;
    background: linear-gradient(180deg, var(--bg-chrome), var(--border));
    overflow: hidden;
  }
  .thumb-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .pin-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--accent);
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: 3px;
    padding: 2px 5px;
  }
  .meta { padding: 10px 12px; }
  .meta h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path-label {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .context-menu {
    position: fixed;
    z-index: 200;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: 4px 0;
    min-width: 180px;
  }
  .context-menu button {
    display: block;
    width: 100%;
    padding: 8px 16px;
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
    font-size: 13px;
    color: var(--fg);
  }
  .context-menu button:hover { background: var(--bg-chrome); }
  .context-menu button.danger { color: var(--danger); }
  .context-menu .sep { height: 1px; background: var(--border); margin: 4px 0; }
</style>
