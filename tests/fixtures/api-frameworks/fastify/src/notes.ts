import type { FastifyInstance } from "fastify";

/** A stored note. */
export interface Note {
  id: number;
  title: string;
  body: string;
}

export type NewNote = Omit<Note, "id">;

export interface NoteSearch {
  q?: string;
  limit?: number;
}

/** Note routes. */
export default async function notes(app: FastifyInstance): Promise<void> {
  app.get<{ Querystring: NoteSearch; Reply: Note[] }>("/notes", async () => []);

  app.post<{ Body: NewNote; Reply: Note }>("/notes", { preHandler: [app.authenticate] }, async (req, reply) => {
    reply.code(201);
    return { id: 1, ...req.body };
  });
}
