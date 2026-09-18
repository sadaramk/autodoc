/** Fetches album names from a server base URL. */
export async function listAlbums(base: string): Promise<string[]> {
  const res = await fetch(`${base}/albums`);
  return (await res.json()) as string[];
}
