<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { errorText } from "../lib/format";
  import { store } from "../lib/store.svelte";

  let source = $state("");
  let started = $state(false);

  const p = $derived(started ? store.progress : null);
  const running = $derived(started && !p?.finished);
  const pct = $derived(p && p.total ? Math.round((p.done / p.total) * 100) : 0);

  async function pick() {
    const dir = await open({ directory: true, title: "Choose a folder to import" });
    if (typeof dir === "string") source = dir;
  }

  async function start() {
    store.progress = null;
    try {
      await api.startImport(source);
      started = true;
    } catch (e) {
      store.toast(errorText(e), true);
    }
  }

  function close() {
    if (running) return;
    started = false;
    store.importOpen = false;
  }
</script>

<svelte:window onkeydown={(e) => store.importOpen && e.key === "Escape" && close()} />

{#if store.importOpen}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Import photos">
      <h2>Import photos</h2>

      {#if !started}
        <p class="hint">
          Photos are <strong>copied</strong> into your library under <code>originals/YYYY/MM/DD</code>. The source
          folder is not changed. Files already in the library are skipped.
        </p>
        <div class="pick">
          <input readonly value={source} placeholder="No folder selected" onclick={pick} />
          <button onclick={pick}>Browse…</button>
        </div>
        <div class="buttons">
          <button onclick={close}>Cancel</button>
          <button class="primary" disabled={!source} onclick={start}>Import</button>
        </div>
      {:else}
        <div class="bar"><div style:width="{pct}%"></div></div>
        <div class="stats">
          <span>{p?.done ?? 0} / {p?.total ?? "…"}</span>
          <span class="ok">{p?.added ?? 0} added</span>
          <span>{p?.skipped ?? 0} already in library</span>
          {#if p?.failed}<span class="bad">{p.failed} failed</span>{/if}
        </div>
        {#if running}
          <p class="current" title={p?.current}>{p?.current ?? "Scanning folder…"}</p>
        {:else if p}
          <p><strong>{p.cancelled ? "Import cancelled." : "Import finished."}</strong></p>
          {#if p.errors.length}
            <details open={p.errors.length < 5}>
              <summary>{p.errors.length} error(s)</summary>
              <ul class="errors">
                {#each p.errors as err}<li><code>{err.path}</code><br />{err.error}</li>{/each}
              </ul>
            </details>
          {/if}
        {/if}
        <div class="buttons">
          {#if running}
            <button onclick={() => api.cancelImport()}>Stop</button>
          {:else}
            <button onclick={() => { started = false; source = ""; }}>Import more</button>
            <button class="primary" onclick={close}>Done</button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; z-index: 60; background: rgb(0 0 0 / 0.5); display: flex; align-items: center; justify-content: center; }
  .dialog { width: min(560px, calc(100vw - 32px)); max-height: 80vh; overflow-y: auto; padding: 20px 22px;
    background: var(--panel); border: 1px solid var(--border); border-radius: 12px; box-shadow: 0 20px 60px rgb(0 0 0 / 0.5); }
  h2 { margin: 0 0 12px; font-size: 18px; }
  .hint { color: var(--muted); font-size: 13px; line-height: 1.5; }
  .pick { display: flex; gap: 8px; }
  .pick input { flex: 1; cursor: pointer; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .bar { height: 8px; border-radius: 4px; background: var(--cell); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
  .stats { display: flex; gap: 16px; margin-top: 10px; font-size: 13px; }
  .ok { color: var(--ok); }
  .bad { color: var(--danger); }
  .current { font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .errors { font-size: 12px; max-height: 200px; overflow-y: auto; padding-left: 18px; }
  .errors li { margin-bottom: 6px; }
</style>
