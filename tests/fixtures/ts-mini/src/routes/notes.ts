import type { FastifyInstance } from "fastify";
import type { NotesStore } from "../store/notes-store";
import type { NewNote } from "../types";

/** Registers CRUD routes for notes. */
export function registerNoteRoutes(app: FastifyInstance, store: NotesStore): void {
  app.get("/notes", async () => store.list());

  app.post<{ Body: NewNote }>("/notes", async (req, reply) => {
    const note = store.create(req.body);
    reply.code(201);
    return note;
  });

  app.delete<{ Params: { id: string } }>("/notes/:id", async (req, reply) => {
    store.remove(Number(req.params.id));
    reply.code(204);
  });
}
