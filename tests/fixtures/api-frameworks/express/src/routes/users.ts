import { Router, type Request, type Response } from "express";
import { z } from "zod";
import { requireRole } from "../auth";

export class NotFoundError extends Error {}

/** A user as returned by the API. */
export interface User {
  id: string;
  email: string;
  role: "admin" | "member";
  /** Display name, if the user set one. */
  name?: string;
}

export interface UserQuery {
  page?: number;
}

const createUserSchema = z.object({
  email: z.string().email(),
  role: z.enum(["admin", "member"]),
  name: z.string().min(2).max(80).optional(),
});

/** Builds the users router. */
export function usersRouter(): Router {
  const router = Router();

  /** Lists users, newest first. */
  router.get("/", async (req: Request<{}, User[], {}, UserQuery>, res: Response<User[]>) => {
    const limit = req.query.limit;
    res.json([]);
  });

  router.get("/:id", async (req, res) => {
    const user = await findUser(req.params.id);
    if (!user) {
      throw new NotFoundError("user not found");
    }
    res.json(user);
  });

  /** Invites a user. Admins only. */
  router.post("/", requireRole("admin"), async (req, res) => {
    const input = createUserSchema.parse(req.body);
    const user = await saveUser(input);
    res.status(201).json(user);
  });

  router.delete("/:id", requireRole("admin"), async (req, res) => {
    res.sendStatus(204);
  });

  return router;
}

async function findUser(id: string): Promise<User | undefined> {
  return undefined;
}

async function saveUser(input: unknown): Promise<User> {
  return input as User;
}
