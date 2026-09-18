import type { NextFunction, Request, Response } from "express";

/** Allows only callers holding `role`. */
export function requireRole(role: string) {
  return (req: Request, res: Response, next: NextFunction) => {
    if (req.header("X-Role") !== role) {
      res.status(403).json({ error: "forbidden" });
      return;
    }
    next();
  };
}
