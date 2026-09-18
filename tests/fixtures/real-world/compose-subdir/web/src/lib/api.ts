import { listAlbums } from "@shop/sdk";

const API = import.meta.env.PUBLIC_API_URL;

/** Loads albums from the server. */
export async function loadAlbums(): Promise<string[]> {
  return listAlbums(API);
}
