<script lang="ts">
  import { discardPrompt } from "../stores/discard.svelte";
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (discardPrompt.current && !dialog.open) dialog.showModal();
    else if (!discardPrompt.current && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} aria-labelledby="discard-title" aria-describedby="discard-message" role="alertdialog"
  oncancel={(event) => { event.preventDefault(); discardPrompt.answer(false); }}>
  <h2 id="discard-title">Discard unsaved changes?</h2>
  <p id="discard-message">{discardPrompt.current?.message ?? ""}</p>
  <div class="actions">
    <button onclick={() => discardPrompt.answer(false)}>Cancel</button>
    <button class="discard" onclick={() => discardPrompt.answer(true)}>Discard changes</button>
  </div>
</dialog>

<style>
  dialog { max-width: min(460px, calc(100vw - 48px)); padding: 24px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bg-elev); color: var(--fg); box-shadow: var(--shadow-lg); }
  dialog::backdrop { background: rgba(0, 0, 0, 0.4); }
  h2 { margin: 0 0 12px; font-size: 18px; }
  p { line-height: 1.6; overflow-wrap: anywhere; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 24px; }
  button { padding: 8px 14px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--control-bg); cursor: pointer; }
  button:hover { background: var(--control-hover); }
  .discard { color: var(--danger); }
</style>
