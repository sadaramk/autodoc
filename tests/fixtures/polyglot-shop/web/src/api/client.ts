import type { CartItem, CheckoutResult } from "./types";

const API_URL = import.meta.env.VITE_API_URL as string;

/**
 * Submits the cart to the api-gateway checkout endpoint.
 * The gateway charges the card and returns the placed order.
 */
export async function submitCheckout(items: CartItem[], paymentToken: string): Promise<CheckoutResult> {
  const res = await fetch(`${API_URL}/checkout`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ items, paymentToken }),
  });
  if (!res.ok) {
    throw new Error(`checkout failed: ${res.status}`);
  }
  return (await res.json()) as CheckoutResult;
}

/** Loads the product catalog (served from the gateway's Redis cache). */
export async function fetchCatalog(): Promise<CartItem[]> {
  const res = await fetch(`${API_URL}/catalog`);
  return (await res.json()) as CartItem[];
}
