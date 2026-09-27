import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Album, Bucket, Counts, Filter, ImportProgress, Photo, PhotoDetail, Sort, Tag } from "./types";

export const src = (path: string) => convertFileSrc(path);

export const api = {
  openLibrary: (path: string, create: boolean) => invoke<{ root: string }>("open_library", { path, create }),
  currentLibrary: () => invoke<{ root: string } | null>("current_library"),

  startImport: (src: string) => invoke<void>("start_import", { src }),
  cancelImport: () => invoke<void>("cancel_import"),
  onImportProgress: (cb: (p: ImportProgress) => void): Promise<UnlistenFn> =>
    listen<ImportProgress>("import://progress", (e) => cb(e.payload)),

  listPhotos: (filter: Filter, sort: Sort, offset: number, limit: number) =>
    invoke<Photo[]>("list_photos", { filter, sort, offset, limit }),
  dateBuckets: (filter: Filter, sort: Sort) => invoke<Bucket[]>("photo_date_buckets", { filter, sort }),
  getPhoto: (id: number) => invoke<PhotoDetail>("get_photo", { id }),
  counts: () => invoke<Counts>("counts"),

  setRating: (ids: number[], rating: number) => invoke<void>("set_rating", { ids, rating }),
  setFavorite: (ids: number[], favorite: boolean) => invoke<void>("set_favorite", { ids, favorite }),

  listTags: () => invoke<Tag[]>("list_tags"),
  addTags: (ids: number[], names: string[]) => invoke<void>("add_tags", { ids, names }),
  removeTag: (ids: number[], tagId: number) => invoke<void>("remove_tag", { ids, tagId }),
  listCameras: () => invoke<string[]>("list_cameras"),

  listAlbums: () => invoke<Album[]>("list_albums"),
  createAlbum: (name: string) => invoke<number>("create_album", { name }),
  renameAlbum: (id: number, name: string) => invoke<void>("rename_album", { id, name }),
  deleteAlbum: (id: number) => invoke<void>("delete_album", { id }),
  addToAlbum: (albumId: number, ids: number[]) => invoke<void>("add_to_album", { albumId, ids }),
  removeFromAlbum: (albumId: number, ids: number[]) => invoke<void>("remove_from_album", { albumId, ids }),
  setAlbumCover: (albumId: number, photoId: number) => invoke<void>("set_album_cover", { albumId, photoId }),

  findDuplicates: (threshold: number) => invoke<Photo[][]>("find_duplicates", { threshold }),
  trashPhotos: (ids: number[]) => invoke<void>("trash_photos", { ids }),
  restorePhotos: (ids: number[]) => invoke<void>("restore_photos", { ids }),
  emptyTrash: () => invoke<number>("empty_trash"),
};
