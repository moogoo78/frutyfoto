<script lang="ts">
  import { api, src } from "../lib/api";
  import { errorText, fmtBytes, fmtDateTime, originLabel } from "../lib/format";
  import type { PhotoList } from "../lib/photoList.svelte";
  import Stars from "../lib/Stars.svelte";
  import { store } from "../lib/store.svelte";
  import SourceInput from "../lib/SourceInput.svelte";
  import TagInput from "../lib/TagInput.svelte";
  import type { Mark, Photo, PhotoDetail } from "../lib/types";

  let {
    list,
    index = $bindable(),
    onclose,
  }: { list: PhotoList; index: number; onclose: () => void } = $props();

  let photo = $state<Photo | undefined>();
  let detail = $state<PhotoDetail | null>(null);
  let showInfo = $state(true);
  let tagInput: TagInput | undefined = $state();

  // Load the photo at `index` (re-run when data changes via store.version / list pages).
  $effect(() => {
    const i = index;
    void store.version;
    if (i >= list.total && list.total > 0) index = list.total - 1;
    if (list.total === 0) {
      onclose();
      return;
    }
    const cached = list.get(i);
    if (cached) photo = cached;
    else list.fetch(i).then((p) => index === i && (photo = p));
  });

  $effect(() => {
    const id = photo?.id;
    void store.version;
    if (id == null) return;
    api.getPhoto(id).then(
      (d) => photo?.id === id && (detail = d),
      (e) => store.toast(errorText(e), true),
    );
  });

  const step = (d: number) => (index = Math.min(list.total - 1, Math.max(0, index + d)));

  function onkeydown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || !photo) return;
    const ids = [photo.id];
    if (e.key === "ArrowRight" || e.key === " ") step(1);
    else if (e.key === "ArrowLeft") step(-1);
    else if (e.key === "Escape") onclose();
    else if (e.key === "i") showInfo = !showInfo;
    else if (e.shiftKey && /^Digit[0-5]$/.test(e.code)) store.mutate(() => api.setRating(ids, Number(e.code.slice(5))));
    else if (e.key === "x" || /^[1-4]$/.test(e.key)) store.toggleMark(ids, e.key as Mark, () => photo?.mark);
    else if (e.key === "f") store.mutate(() => api.setFavorite(ids, !photo!.favorite));
    else if (e.key === "t") tagInput?.focus();
    else if (e.key === "Delete") trash();
    else return;
    e.preventDefault();
  }

  function trash() {
    if (!photo) return;
    const ids = [photo.id];
    if (photo.trashedAt) store.mutate(() => api.restorePhotos(ids), "Restored");
    else store.mutate(() => api.trashPhotos(ids), "Moved to trash");
  }
</script>

<svelte:window {onkeydown} />

<div class="viewer">
  <div class="stage">
    {#if photo}
      <img src={src(photo.display)} alt={photo.origName} />
    {/if}
    <button class="nav prev" disabled={index <= 0} onclick={() => step(-1)} aria-label="Previous">‹</button>
    <button class="nav next" disabled={index >= list.total - 1} onclick={() => step(1)} aria-label="Next">›</button>
    <div class="top">
      <span class="pos">{index + 1} / {list.total}</span>
      {#if photo?.mark}<span class="mark mark-{photo.mark}" title={store.markName(photo.mark)}>{photo.mark} · {store.markName(photo.mark)}</span>{/if}
      <span class="spacer"></span>
      <button onclick={() => (showInfo = !showInfo)} title="Toggle info (i)">Info</button>
      <button onclick={onclose} title="Close (Esc)">✕</button>
    </div>
  </div>

  {#if showInfo && photo}
    {@const p = photo}
    <aside class="info">
      <h2 title={p.relPath}>{p.origName}</h2>
      <div class="row">
        <Stars value={p.rating} onchange={(n) => store.mutate(() => api.setRating([p.id], n))} />
        <button class="fav" class:on={p.favorite} onclick={() => store.mutate(() => api.setFavorite([p.id], !p.favorite))}
          title="Favorite (f)">♥</button>
      </div>

      <dl>
        <dt>Taken</dt><dd>{fmtDateTime(p.takenAt)}</dd>
        <dt>Size</dt><dd>{p.width} × {p.height} · {fmtBytes(p.fileSize)}</dd>
        {#if p.cameraModel}<dt>Camera</dt><dd>{[p.cameraMake, p.cameraModel].filter(Boolean).join(" ")}</dd>{/if}
        {#if p.lens}<dt>Lens</dt><dd>{p.lens}</dd>{/if}
        {#if p.fNumber || p.exposure || p.iso || p.focalLen}
          <dt>Exposure</dt>
          <dd>
            {[p.fNumber && `ƒ/${p.fNumber.toFixed(1)}`, p.exposure, p.iso && `ISO ${p.iso}`, p.focalLen && `${Math.round(p.focalLen)} mm`]
              .filter(Boolean).join(" · ")}
          </dd>
        {/if}
        {#if p.gpsLat != null && p.gpsLon != null}
          <dt>Location</dt><dd>{p.gpsLat.toFixed(5)}, {p.gpsLon.toFixed(5)}</dd>
        {/if}
        <dt>Source</dt>
        <dd>
          {originLabel(p) || "Unknown"}
          {#if p.source}<button class="link" onclick={() => store.mutate(() => api.setSource([p.id], ""))}>reset</button>{/if}
        </dd>
        <dt>Imported</dt><dd>{fmtDateTime(p.importedAt)}</dd>
        {#if p.sourcePath}<dt>Source</dt><dd class="path">{p.sourcePath}</dd>{/if}
        <dt>File</dt><dd class="path">{p.relPath}</dd>
      </dl>

      <h3>Tags</h3>
      <div class="chips">
        {#each detail?.tags ?? [] as t (t.id)}
          <span class="chip">{t.name}<button onclick={() => store.mutate(() => api.removeTag([p.id], t.id))} aria-label="Remove tag">×</button></span>
        {/each}
      </div>
      <TagInput bind:this={tagInput} onadd={(names) => store.mutate(() => api.addTags([p.id], names))} />

      <h3>Source</h3>
      <SourceInput onset={(s) => store.mutate(() => api.setSource([p.id], s))} />

      <h3>Albums</h3>
      <div class="chips">
        {#each detail?.albums ?? [] as a (a.id)}
          <span class="chip">{a.name}<button onclick={() => store.mutate(() => api.removeFromAlbum(a.id, [p.id]))} aria-label="Remove from album">×</button></span>
        {/each}
      </div>
      {#if store.albums.length}
        <select
          onchange={(e) => {
            const id = Number(e.currentTarget.value);
            e.currentTarget.value = "";
            if (id) store.mutate(() => api.addToAlbum(id, [p.id]));
          }}>
          <option value="">Add to album…</option>
          {#each store.albums as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
        </select>
      {/if}

      <div class="actions">
        <button class:danger={!p.trashedAt} onclick={trash}>{p.trashedAt ? "Restore" : "Move to trash"}</button>
      </div>
    </aside>
  {/if}
</div>

<style>
  .viewer { position: fixed; inset: 0; z-index: 50; display: flex; background: #0b0c0e; }
  .stage { position: relative; flex: 1; display: flex; align-items: center; justify-content: center; min-width: 0; }
  .stage img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .top { position: absolute; top: 0; left: 0; right: 0; display: flex; gap: 8px; padding: 10px 12px;
    background: linear-gradient(rgb(0 0 0 / 0.6), transparent); }
  .pos { color: #ccc; align-self: center; font-size: 13px; }
  .top .mark { align-self: center; height: 24px; text-transform: none; }
  .spacer { flex: 1; }
  .nav { position: absolute; top: 50%; transform: translateY(-50%); font-size: 40px; width: 52px; height: 80px;
    background: rgb(0 0 0 / 0.3); border: none; color: #fff; border-radius: 8px; }
  .nav:disabled { opacity: 0.15; }
  .prev { left: 10px; }
  .next { right: 10px; }
  .info { width: 320px; flex-shrink: 0; overflow-y: auto; padding: 16px; background: var(--panel);
    border-left: 1px solid var(--border); }
  h2 { font-size: 15px; margin: 0 0 8px; word-break: break-all; }
  h3 { font-size: 12px; text-transform: uppercase; color: var(--muted); margin: 18px 0 6px; letter-spacing: 0.05em; }
  .row { display: flex; align-items: center; gap: 12px; }
  .fav { background: none; border: none; font-size: 20px; color: var(--muted); padding: 0; }
  .fav.on { color: #ff6b81; }
  dl { display: grid; grid-template-columns: 76px 1fr; gap: 6px 8px; font-size: 13px; margin: 14px 0 0; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .path { word-break: break-all; font-size: 12px; color: var(--muted); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .actions { margin-top: 24px; }
</style>
