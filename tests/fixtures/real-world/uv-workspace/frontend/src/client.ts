/** Base URL of the backend API. */
export const BASE = import.meta.env.VITE_API_URL;

/** An item as the backend returns it. */
export interface Item {
  id: number;
  title: string;
  ownerEmail: string;
  status: "draft" | "published";
}

/** Loads one item for the detail view. */
export async function getItem(id: number): Promise<Item> {
  const res = await fetch(`${BASE}/api/v1/items/${id}`);
  return (await res.json()) as Item;
}

/** Root component placeholder. */
export function App(): null {
  return null;
}
