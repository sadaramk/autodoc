import Fastify from "fastify";
import { registerNoteRoutes } from "./routes/notes";
import { NotesStore } from "./store/notes-store";

/** Starts the notes API on PORT (default 8000). */
async function start(): Promise<void> {
  const app = Fastify({ logger: true });
  const store = new NotesStore(process.env.NOTES_DB ?? "notes.db");
  registerNoteRoutes(app, store);
  await app.listen({ port: Number(process.env.PORT ?? 8000), host: "0.0.0.0" });
}

start();
