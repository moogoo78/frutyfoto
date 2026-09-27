<script lang="ts">
  import { onMount } from "svelte";
  import { api, src } from "../lib/api";
  import { errorText, fmtBytes, fmtDateTime } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import type { Photo } from "../lib/types";

  let threshold = $state(6);
  let groups = $state.raw<Photo[][] | null>(null);
  let keep = $state<Record<number, number>>({});
  let scanning = $state(false);

  async function scan() {
    scanning = true;
    try {
      const found = await api.findDuplicates(threshold);
      groups = found;
      // Default: keep the largest (first) photo of each group.
      keep = Object.fromEntries(found.map((g, i) => [i, g[0].id]));
    } catch (e) {
      store.toast(errorText(e), true);
    }
    scanning = false;
  }

  const losers = (i: number) => groups![i].filter((p) => p.id !== keep[i]).map((p) => p.id);

  async function trashGroup(i: number) {
    await store.mutate(() => api.trashPhotos(losers(i)), "Moved extras to trash");
    scan();
  }

  async function trashAll() {
    const ids = groups!.flatMap((_, i) => losers(i));
    await store.mutate(() => api.trashPhotos(ids), `Moved ${ids.length} photo(s) to trash`);
    scan();
  }

  onMount(scan);
</script>

<section class="dupes">
  <header>
    <h1>Duplicates</h1>
    <label title="Maximum number of differing bits between perceptual hashes">
      Similarity
      <input type="range" min="0" max="16" bind:value={threshold} onchange={scan} />
      <span class="val">{threshold === 0 ? "identical look" : threshold <= 6 ? "very similar" : threshold <= 10 ? "similar" : "loose"} ({threshold})</span>
    </label>
    <span class="grow"></span>
    {#if groups?.length}
      <button class="danger" onclick={trashAll}>Trash all extras</button>
    {/if}
  </header>

  <div class="body">
    {#if scanning && !groups}
      <p class="empty">Scanning…</p>
    {:else if groups && groups.length === 0}
      <p class="empty">No similar photos found. Exact duplicates are already skipped during import.</p>
    {:else if groups}
      <p class="hint">{groups.length} group(s). Pick the photo to keep in each group; the others go to trash.</p>
      {#each groups as group, i (group.map((p) => p.id).join())}
        <div class="group">
          <div class="cards">
            {#each group as p (p.id)}
              <label class="card" class:keep={keep[i] === p.id}>
                <img src={src(p.thumb)} alt={p.origName} />
                <div class="meta">
                  <input type="radio" name="keep-{i}" checked={keep[i] === p.id} onchange={() => (keep[i] = p.id)} />
                  <strong title={p.relPath}>{p.origName}</strong>
                  <span>{p.width}×{p.height} · {fmtBytes(p.fileSize)}</span>
                  <span>{fmtDateTime(p.takenAt)}</span>
                  {#if p.rating || p.favorite}<span>{p.favorite ? "♥ " : ""}{"★".repeat(p.rating)}</span>{/if}
                </div>
              </label>
            {/each}
          </div>
          <button onclick={() => trashGroup(i)}>Keep selected, trash {group.length - 1}</button>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .dupes { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 16px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  label { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--muted); }
  .val { min-width: 120px; }
  .grow { flex: 1; }
  .body { flex: 1; overflow-y: auto; padding: 16px; }
  .empty, .hint { color: var(--muted); }
  .group { margin-bottom: 20px; padding: 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--panel); }
  .cards { display: flex; gap: 12px; overflow-x: auto; margin-bottom: 10px; }
  .card { flex-direction: column; align-items: stretch; width: 200px; flex-shrink: 0; padding: 6px; border-radius: 8px;
    border: 2px solid transparent; color: var(--fg); cursor: pointer; }
  .card.keep { border-color: var(--accent); }
  .card:not(.keep) img { opacity: 0.6; }
  .card img { width: 100%; height: 150px; object-fit: cover; border-radius: 4px; background: var(--cell); }
  .meta { display: grid; grid-template-columns: auto 1fr; gap: 2px 6px; font-size: 12px; }
  .meta input { grid-row: span 4; margin-top: 2px; }
  .meta strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta span { color: var(--muted); }
</style>
