const PAYMENTS_URL = process.env.PAYMENTS_URL ?? "http://payments:8080";

/** Result of a charge attempt reported by the payments service. */
export interface ChargeResult {
  chargeId: string;
  status: "succeeded" | "failed";
}

/** Synchronously charges an order via the Go payments service. */
export async function chargeOrder(orderId: string, amountCents: number, token: string): Promise<ChargeResult> {
  const res = await fetch(`${PAYMENTS_URL}/charges`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ orderId, amountCents, token }),
  });
  if (!res.ok) {
    return { chargeId: "", status: "failed" };
  }
  return (await res.json()) as ChargeResult;
}
