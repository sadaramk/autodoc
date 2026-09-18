import Fastify from "fastify";
import notes from "./notes";

const app = Fastify();
app.register(notes, { prefix: "/v1" });
app.listen({ port: 8080 });
