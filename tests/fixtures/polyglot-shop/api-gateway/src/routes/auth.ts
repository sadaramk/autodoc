import type { NextFunction, Request, Response } from "express";

/** Rejects requests without a customer session token. */
export function requireCustomer(req: Request, res: Response, next: NextFunction): void {
  if (!req.header("Authorization")) {
    res.status(401).json({ error: "sign in to check out" });
    return;
  }
  next();
}
