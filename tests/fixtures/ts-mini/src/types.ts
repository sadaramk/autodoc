/** A persisted note. */
export interface Note {
  id: number;
  title: string;
  body: string;
  createdAt: string;
}

/** Payload accepted by POST /notes. */
export type NewNote = Pick<Note, "title" | "body">;
