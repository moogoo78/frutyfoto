<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { store } from "../lib/store.svelte";

  async function choose(create: boolean) {
    const dir = await open({
      directory: true,
      title: create ? "Choose an empty folder for the new library" : "Open an existing frutyfoto library",
    });
    if (typeof dir === "string") store.openLibrary(dir, create);
  }
</script>

<div class="welcome">
  <div class="card">
    <div class="logo">◒</div>
    <h1>frutyfoto</h1>
    <p>Keep all your photos in one organised library, sorted by date, with tags, albums, ratings and duplicate detection.</p>
    <div class="buttons">
      <button class="primary" onclick={() => choose(true)}>Create new library</button>
      <button onclick={() => choose(false)}>Open existing library</button>
    </div>
    <p class="small">A library is a folder. Imported photos are copied into <code>originals/</code> inside it.</p>
  </div>
</div>

<style>
  .welcome { height: 100vh; display: flex; align-items: center; justify-content: center; }
  .card { max-width: 440px; text-align: center; padding: 32px; }
  .logo { font-size: 56px; color: var(--accent); }
  h1 { margin: 8px 0; }
  p { color: var(--muted); line-height: 1.5; }
  .buttons { display: flex; flex-direction: column; gap: 10px; margin: 24px 0; }
  .buttons button { padding: 10px; font-size: 15px; }
  .small { font-size: 12px; }
</style>
