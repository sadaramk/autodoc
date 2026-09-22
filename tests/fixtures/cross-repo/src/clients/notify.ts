// The host is named here and used as a template below, which is the ordinary
// shape: neither line contains the whole URL on its own.
const NOTIFY_URL = process.env.NOTIFY_URL ?? "http://notifications:9000";

/** Tells a service that lives in a different repository. */
export async function notifyShipped(orderId: string): Promise<void> {
  await fetch(`${NOTIFY_URL}/v1/notifications`, {
    method: "POST",
    body: JSON.stringify({ orderId }),
  });
}
