/** A single line in the shopping cart. */
export interface CartItem {
  sku: string;
  name: string;
  quantity: number;
  priceCents: number;
}

/** Response returned by POST /checkout. */
export interface CheckoutResult {
  orderId: string;
  status: "placed" | "payment_failed";
  /** Promised delivery date shown on the confirmation page. */
  estimatedDelivery: string;
}
