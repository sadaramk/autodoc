import express, { Router } from "express";
import { usersRouter } from "./routes/users";

const app = express();
const api = Router();

api.use("/users", usersRouter());
app.use(express.json());
app.use("/api", api);
app.get("/healthz", (_req, res) => res.send("ok"));

app.listen(3000);
