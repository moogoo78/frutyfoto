<script lang="ts">
  import { ask, open } from "@tauri-apps/plugin-dialog";
  import { api, src } from "../lib/api";
  import { basename } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import type { View } from "../lib/types";

  let newAlbum = $state<string | null>(null);
  let renaming = $state<number | null>(null);
  let renameValue = $state("");
  let dropTarget = $state<number | null>(null);

  const is = (v: View) => JSON.stringify(v) === JSON.stringify(store.view);

  async function createAlbum() {
    const name = newAlbum?.trim();
    newAlbum = null;
    if (!name) return;
    let id = 0;
    await store.mutate(async () => (id = await api.createAlbum(name)));
    if (id) store.setView({ kind: "album", id });
  }

  async function finishRename(id: number) {
    const name = renameValue.trim();
    renaming = null;
    if (name) store.mutate(() => api.renameAlbum(id, name));
  }

  async function deleteAlbum(id: number, name: string) {
    if (await ask(`Delete album "${name}"? Photos stay in the library.`, { title: "Delete album", kind: "warning" })) {
      store.mutate(() => api.deleteAlbum(id));
    }
  }

  function drop(e: DragEvent, albumId: number) {
    dropTarget = null;
    const raw = e.dataTransfer?.getData("application/x-foto-ids");
    if (!raw) return;
    e.preventDefault();
    const ids = JSON.parse(raw) as number[];
    store.mutate(() => api.addToAlbum(albumId, ids), `Added ${ids.length} photo(s)`);
  }

  async function switchLibrary() {
    const dir = await open({ directory: true, title: "Open frutyfoto library" });
    if (typeof dir === "string") store.openLibrary(dir, false);
  }

  const focus = (el: HTMLInputElement) => el.focus();
</script>

<nav class="sidebar">
  <div class="brand">
    <span class="logo">◒</span> frutyfoto
  </div>
  <button class="primary import" onclick={() => store.openImport()}>＋ Import photos</button>

  <div class="section">
    <button class="item" class:active={is({ kind: "library" })} onclick={() => store.setView({ kind: "library" })}>
      <span>All photos</span><small>{store.counts.all}</small>
    </button>
    <button class="item" class:active={is({ kind: "favorites" })} onclick={() => store.setView({ kind: "favorites" })}>
      <span>Favorites</span><small>{store.counts.favorites}</small>
    </button>
    <button class="item" class:active={store.view.kind === "imports" || store.view.kind === "import"}
      onclick={() => store.setView({ kind: "imports" })}>
      <span>Imports</span><small>{store.imports.length || ""}</small>
    </button>
    <button class="item" class:active={store.view.kind === "marks" || store.view.kind === "mark"}
      onclick={() => store.setView({ kind: "marks" })}>
      <span>Marks</span><small>{store.counts.marked || ""}</small>
    </button>
    <button class="item" class:active={is({ kind: "duplicates" })} onclick={() => store.setView({ kind: "duplicates" })}>
      <span>Duplicates</span>
    </button>
    <button class="item" class:active={is({ kind: "trash" })} onclick={() => store.setView({ kind: "trash" })}>
      <span>Trash</span><small>{store.counts.trash || ""}</small>
    </button>
  </div>

  <div class="section">
    <div class="heading">
      Albums <button class="icon" title="New album" onclick={() => (newAlbum = "")}>＋</button>
    </div>
    {#if newAlbum != null}
      <input
        class="inline"
        placeholder="Album name"
        bind:value={newAlbum}
        use:focus
        onblur={createAlbum}
        onkeydown={(e) => {
          if (e.key === "Enter") createAlbum();
          if (e.key === "Escape") newAlbum = null;
        }} />
    {/if}
    {#each store.albums as a (a.id)}
      {#if renaming === a.id}
        <input
          class="inline"
          bind:value={renameValue}
          use:focus
          onblur={() => finishRename(a.id)}
          onkeydown={(e) => {
            if (e.key === "Enter") finishRename(a.id);
            if (e.key === "Escape") renaming = null;
          }} />
      {:else}
        <div
          class="item album"
          class:active={is({ kind: "album", id: a.id })}
          class:drop={dropTarget === a.id}
          role="button"
          tabindex="0"
          onclick={() => store.setView({ kind: "album", id: a.id })}
          onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "album", id: a.id })}
          ondblclick={() => {
            renaming = a.id;
            renameValue = a.name;
          }}
          ondragover={(e) => {
            e.preventDefault();
            dropTarget = a.id;
          }}
          ondragleave={() => (dropTarget = null)}
          ondrop={(e) => drop(e, a.id)}>
          {#if a.cover}<img src={src(a.cover)} alt="" />{:else}<span class="nocover"></span>{/if}
          <span class="name" title="Double-click to rename">{a.name}</span>
          <small>{a.count}</small>
          <button class="icon del" title="Delete album" onclick={(e) => {
            e.stopPropagation();
            deleteAlbum(a.id, a.name);
          }}>×</button>
        </div>
      {/if}
    {:else}
      {#if newAlbum == null}<p class="hint">Drag photos onto an album to add them.</p>{/if}
    {/each}
  </div>

  <div class="section tags">
    <div class="heading">Tags</div>
    {#each store.tags as t (t.id)}
      <button class="item" class:active={is({ kind: "tag", id: t.id })} onclick={() => store.setView({ kind: "tag", id: t.id })}>
        <span>#{t.name}</span><small>{t.count}</small>
      </button>
    {:else}
      <p class="hint">No tags yet. Select photos and press T.</p>
    {/each}
  </div>

  <div class="footer" title={store.root}>
    <span>📁 {basename(store.root ?? "")}</span>
    <button class="link" onclick={switchLibrary}>Switch</button>
  </div>
</nav>

<style>
  .sidebar { width: 230px; flex-shrink: 0; display: flex; flex-direction: column; background: var(--panel);
    border-right: 1px solid var(--border); overflow-y: auto; }
  .brand { padding: 16px 16px 10px; font-weight: 700; font-size: 16px; }
  .logo { color: var(--accent); }
  .import { margin: 0 12px 8px; }
  .section { padding: 8px 8px; border-top: 1px solid var(--border); }
  .heading { display: flex; justify-content: space-between; align-items: center; padding: 4px 8px;
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 8px; border: none; border-radius: 6px;
    background: none; color: inherit; text-align: left; font-size: 13px; cursor: pointer; }
  .item > span:first-child, .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item:hover { background: var(--hover); }
  .item.active { background: var(--accent-bg); color: var(--accent-fg); }
  .item.drop { outline: 2px dashed var(--accent); }
  small { color: var(--muted); font-size: 11px; }
  .album img, .nocover { width: 24px; height: 24px; border-radius: 4px; object-fit: cover; background: var(--cell); flex-shrink: 0; }
  .icon { background: none; border: none; color: var(--muted); padding: 0 4px; font-size: 14px; cursor: pointer; }
  .del { visibility: hidden; }
  .album:hover .del { visibility: visible; }
  .inline { width: 100%; margin: 2px 0; }
  .hint { color: var(--muted); font-size: 12px; padding: 0 8px; margin: 4px 0; }
  .tags { flex: 1; }
  .footer { display: flex; align-items: center; gap: 6px; padding: 10px 14px; border-top: 1px solid var(--border);
    font-size: 12px; color: var(--muted); }
  .footer span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
