import { Router } from "express";
import { z } from "zod";
import { chargeOrder } from "../clients/payments";
import { insertOrder } from "../db";
import { publishOrderPlaced } from "../events";
import { requireCustomer } from "./auth";

export const checkoutRouter = Router();

/** Body of POST /checkout, validated before anything is persisted. */
export const checkoutSchema = z.object({
  /** Cart lines to buy. */
  items: z
    .array(
      z.object({
        sku: z.string().min(1),
        quantity: z.number().int().positive().max(99),
      }),
    )
    .min(1),
  /** Card token issued by the payment form. */
  paymentToken: z.string().min(1),
  couponCode: z.string().max(32).optional(),
});

export type CheckoutRequest = z.infer<typeof checkoutSchema>;

/**
 * POST /checkout — the critical transaction path.
 * Persists the order, charges it through the payments service,
 * and announces `order.placed` for fulfillment.
 */
checkoutRouter.post("/", requireCustomer, async (req, res) => {
  const parsed = checkoutSchema.safeParse(req.body);
  if (!parsed.success) {
    res.status(400).json({ error: "invalid checkout request" });
    return;
  }
  const { items, paymentToken } = parsed.data;
  const order = await insertOrder(items);
  const charge = await chargeOrder(order.id, order.totalCents, paymentToken);
  if (charge.status !== "succeeded") {
    res.status(402).json({ error: "payment declined", orderId: order.id });
    return;
  }
  await publishOrderPlaced(order);
  res.status(201).json({ orderId: order.id, status: "placed" });
});
