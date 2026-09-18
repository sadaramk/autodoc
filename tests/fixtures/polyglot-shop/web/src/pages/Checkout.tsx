import { useEffect, useState } from "react";
import { fetchCatalog, submitCheckout } from "../api/client";
import type { CartItem, CheckoutResult } from "../api/types";

/** Checkout page: lists the catalog and places an order for the whole cart. */
export function Checkout() {
  const [items, setItems] = useState<CartItem[]>([]);
  const [result, setResult] = useState<CheckoutResult | null>(null);

  useEffect(() => {
    fetchCatalog().then(setItems);
  }, []);

  async function placeOrder() {
    setResult(await submitCheckout(items, "tok_visa"));
  }

  return (
    <main>
      <h1>Checkout</h1>
      <ul>
        {items.map((item) => (
          <li key={item.sku}>
            {item.name} × {item.quantity}
          </li>
        ))}
      </ul>
      <button onClick={placeOrder}>Place order</button>
      {result && <p>Order {result.orderId}: {result.status}</p>}
    </main>
  );
}
