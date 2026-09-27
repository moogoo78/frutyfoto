export interface Photo {
  id: number;
  hash: string;
  relPath: string;
  origName: string;
  sourcePath: string | null;
  takenAt: string;
  importedAt: string;
  width: number | null;
  height: number | null;
  fileSize: number;
  mime: string | null;
  cameraMake: string | null;
  cameraModel: string | null;
  lens: string | null;
  iso: number | null;
  fNumber: number | null;
  exposure: string | null;
  focalLen: number | null;
  gpsLat: number | null;
  gpsLon: number | null;
  rating: number;
  favorite: boolean;
  trashedAt: string | null;
  path: string;
  /** What the viewer shows: the original, or a JPEG preview for HEIC. */
  display: string;
  thumb: string;
}

export interface Tag {
  id: number;
  name: string;
  count: number;
}

export interface Album {
  id: number;
  name: string;
  createdAt: string;
  count: number;
  cover: string | null;
}

export interface PhotoDetail {
  photo: Photo;
  tags: Tag[];
  albums: Album[];
}

export interface Bucket {
  day: string;
  count: number;
}

export interface Counts {
  all: number;
  favorites: number;
  trash: number;
}

export interface Filter {
  dateFrom?: string;
  dateTo?: string;
  tagIds: number[];
  albumId?: number;
  minRating?: number;
  favorite?: boolean;
  camera?: string;
  text?: string;
  trashed: boolean;
}

export type Sort = "takenDesc" | "takenAsc" | "importedDesc" | "ratingDesc";

export interface ImportProgress {
  importId: number;
  total: number;
  done: number;
  added: number;
  skipped: number;
  failed: number;
  current: string;
  finished: boolean;
  cancelled: boolean;
  errors: { path: string; error: string }[];
}

/** What the main area is showing. */
export type View =
  | { kind: "library" }
  | { kind: "favorites" }
  | { kind: "album"; id: number }
  | { kind: "tag"; id: number }
  | { kind: "duplicates" }
  | { kind: "trash" };
