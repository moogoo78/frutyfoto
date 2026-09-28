<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { MarkActionKind, MarkSlot } from "../lib/types";

  const KINDS: { value: MarkActionKind; text: string }[] = [
    { value: "copy", text: "Copy to folder" },
    { value: "album", text: "Add to album" },
    { value: "tag", text: "Add tag" },
  ];

  function save(slot: MarkSlot, patch: Partial<MarkSlot>) {
    const s = { ...slot, ...patch };
    // Switching kind drops a target that meant something else.
    if (patch.kind !== undefined && patch.kind !== slot.kind) s.target = "";
    store.mutate(() => api.setMarkAction(Number(s.mark), s.label, s.kind, s.target));
  }

  async function pickFolder(slot: MarkSlot) {
    const dir = await open({ directory: true, title: "Copy marked photos to…", defaultPath: slot.target || undefined });
    if (typeof dir === "string") save(slot, { target: dir });
  }

  function describe(slot: MarkSlot): string {
    switch (slot.kind) {
      case "trash": return "Move to the library trash (restorable)";
      case "copy": return slot.target ? `Copy to ${slot.target}` : "Choose a folder";
      case "album": {
        const a = store.albums.find((a) => String(a.id) === slot.target);
        return a ? `Add to album “${a.name}” (files move into its folder)` : "Choose an album";
      }
      case "tag": return slot.target ? `Tag with #${slot.target}` : "Enter a tag";
      default: return "Not set up";
    }
  }

  const ready = (slot: MarkSlot) => slot.kind === "trash" || (!!slot.kind && !!slot.target);
</script>

<section class="marks">
  <header>
    <h1>Marks</h1>
    <span class="count">{store.counts.marked} marked photo(s)</span>
  </header>

  <div class="body">
    <p class="hint">
      While browsing, press <kbd>X</kbd> to mark photos for deletion and <kbd>1</kbd>–<kbd>4</kbd> for your own actions
      (press again to unmark). Then run each mark here on all its photos at once. Photos lose their mark once processed.
    </p>

    {#each store.marks as slot (slot.mark)}
      <article class="slot">
        <span class="mark mark-{slot.mark} key">{slot.mark}</span>

        <div class="setup">
          {#if slot.mark === "x"}
            <strong>Delete</strong>
          {:else}
            <div class="fields">
              <input class="label" placeholder="Label (e.g. Backup)" value={slot.label}
                onchange={(e) => save(slot, { label: e.currentTarget.value })} />
              <select value={slot.kind ?? ""}
                onchange={(e) => save(slot, { kind: (e.currentTarget.value || null) as MarkActionKind | null })}>
                <option value="">No action</option>
                {#each KINDS as k}<option value={k.value}>{k.text}</option>{/each}
              </select>
              {#if slot.kind === "copy"}
                <button onclick={() => pickFolder(slot)}>{slot.target ? "Change folder…" : "Choose folder…"}</button>
              {:else if slot.kind === "album"}
                <select value={slot.target} onchange={(e) => save(slot, { target: e.currentTarget.value })}>
                  <option value="">Choose album…</option>
                  {#each store.albums as a (a.id)}<option value={String(a.id)}>{a.name}</option>{/each}
                </select>
              {:else if slot.kind === "tag"}
                <input placeholder="Tag" value={slot.target} onchange={(e) => save(slot, { target: e.currentTarget.value })} />
              {/if}
            </div>
          {/if}
          <div class="desc" title={slot.target}>{describe(slot)}</div>
          {#if store.runningMark === slot.mark && store.markProgress}
            {@const p = store.markProgress}
            <div class="bar"><div style:width="{(p.done / p.total) * 100}%"></div></div>
          {/if}
        </div>

        <span class="n">{slot.count}</span>
        <div class="actions">
          <button disabled={!slot.count} onclick={() => store.setView({ kind: "mark", mark: slot.mark })}>View</button>
          <button class:danger={slot.mark === "x"} class:primary={slot.mark !== "x"}
            disabled={!slot.count || !ready(slot) || !!store.runningMark}
            onclick={() => store.runMark(slot.mark)}>
            {store.runningMark === slot.mark ? "Running…" : "Run"}
          </button>
        </div>
      </article>
    {/each}
  </div>
</section>

<style>
  .marks { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .body { flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 10px; }
  .hint { color: var(--muted); font-size: 13px; line-height: 1.6; margin: 0 0 6px; }
  kbd { padding: 0 5px; border: 1px solid var(--border); border-radius: 4px; background: var(--cell); font-size: 12px; }
  .slot { display: flex; align-items: center; gap: 14px; padding: 12px; border: 1px solid var(--border);
    border-radius: 8px; background: var(--panel); }
  .key { min-width: 32px; height: 32px; font-size: 16px; }
  .setup { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .fields { display: flex; flex-wrap: wrap; gap: 8px; }
  .label { width: 160px; }
  .desc { font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .n { font-size: 18px; font-weight: 600; min-width: 40px; text-align: right; }
  .actions { display: flex; gap: 6px; }
  .bar { height: 6px; border-radius: 3px; background: var(--cell); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
</style>
