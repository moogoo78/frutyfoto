<script lang="ts">
  import { api, src } from "../lib/api";
  import { basename, errorText, fmtDateTime } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import type { ImportRecord } from "../lib/types";

  let errors = $state<Record<number, { path: string; error: string }[]>>({});

  function status(i: ImportRecord): { text: string; kind: string } | null {
    const p = store.progress;
    if (!i.finishedAt) {
      return p && p.importId === i.id && !p.finished ? { text: "Running", kind: "run" } : { text: "Interrupted", kind: "bad" };
    }
    if (i.cancelled) return { text: "Stopped", kind: "warn" };
    if (i.failed) return { text: "Finished with errors", kind: "warn" };
    return null;
  }

  function duration(i: ImportRecord): string {
    if (!i.finishedAt) return "";
    const s = Math.round((new Date(i.finishedAt).getTime() - new Date(i.startedAt).getTime()) / 1000);
    if (!(s >= 0)) return "";
    return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
  }

  async function toggleErrors(id: number) {
    if (errors[id]) {
      delete errors[id];
      return;
    }
    try {
      errors[id] = await api.importErrors(id);
    } catch (e) {
      store.toast(errorText(e), true);
    }
  }
</script>

<section class="history">
  <header>
    <h1>Imports</h1>
    <span class="count">{store.imports.length} import{store.imports.length === 1 ? "" : "s"}</span>
  </header>

  <div class="body">
    {#each store.imports as imp (imp.id)}
      {@const st = status(imp)}
      <article class="card">
        <button class="cover" disabled={!imp.photoCount} title="View photos from this import"
          onclick={() => store.setView({ kind: "import", id: imp.id })}>
          {#if imp.cover}<img src={src(imp.cover)} alt="" />{:else}<span class="none">No photos</span>{/if}
        </button>

        <div class="info">
          <div class="title">
            <strong title={imp.sourceDir}>{basename(imp.sourceDir)}</strong>
            {#if st}<span class="badge {st.kind}">{st.text}</span>{/if}
          </div>
          <div class="path" title={imp.sourceDir}>{imp.sourceDir}</div>
          <div class="when">
            {fmtDateTime(imp.startedAt)}{#if duration(imp)} · took {duration(imp)}{/if}
          </div>
          <div class="stats">
            <span class="ok">{imp.added} added</span>
            <span>{imp.skippedDupes} already in library</span>
            {#if imp.failed}
              <button class="link bad" onclick={() => toggleErrors(imp.id)}>
                {imp.failed} failed {errors[imp.id] ? "▴" : "▾"}
              </button>
            {/if}
            {#if imp.photoCount !== imp.added}
              <span class="muted">{imp.photoCount} still in library</span>
            {/if}
          </div>
          {#if errors[imp.id]}
            <ul class="errors">
              {#each errors[imp.id] as err}<li><code>{err.path}</code> — {err.error}</li>{/each}
            </ul>
          {/if}
        </div>

        <div class="actions">
          <button disabled={!imp.photoCount} onclick={() => store.setView({ kind: "import", id: imp.id })}>View photos</button>
          <button onclick={() => store.openImport(imp.sourceDir)} title="Import this folder again; only new files are copied">
            Import again
          </button>
        </div>
      </article>
    {:else}
      <div class="empty">
        <p>No imports yet.</p>
        <button class="primary" onclick={() => store.openImport()}>Import photos</button>
      </div>
    {/each}
  </div>
</section>

<style>
  .history { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .body { flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 12px; }
  .card { display: flex; gap: 14px; align-items: flex-start; padding: 12px; border: 1px solid var(--border);
    border-radius: 8px; background: var(--panel); }
  .cover { width: 96px; height: 96px; flex-shrink: 0; padding: 0; overflow: hidden; border-radius: 6px; background: var(--cell); }
  .cover img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .none { font-size: 11px; color: var(--muted); }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .title { display: flex; align-items: center; gap: 8px; }
  .title strong { font-size: 15px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .path, .when { font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .stats { display: flex; flex-wrap: wrap; gap: 4px 14px; font-size: 13px; margin-top: 2px; }
  .ok { color: var(--ok); }
  .bad { color: var(--danger); }
  .muted { color: var(--muted); }
  .badge { font-size: 11px; padding: 1px 8px; border-radius: 999px; background: var(--cell); }
  .badge.run { color: var(--accent-fg); background: var(--accent-bg); }
  .badge.warn { color: var(--star); }
  .badge.bad { color: var(--danger); }
  .errors { font-size: 12px; margin: 6px 0 0; padding-left: 18px; max-height: 180px; overflow-y: auto; }
  .errors li { margin-bottom: 4px; word-break: break-all; }
  .actions { display: flex; flex-direction: column; gap: 6px; flex-shrink: 0; }
  .empty { margin: auto; display: flex; flex-direction: column; align-items: center; gap: 8px; color: var(--muted); }
</style>
