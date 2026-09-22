import express from "express";

const app = express();

/** What a caller has to send to raise a notification. */
interface NotificationRequest {
  orderId: string;
  channel?: string;
}

interface Notification {
  id: string;
  orderId: string;
  status: string;
}

// The operation the gateway in the neighbouring repository calls. Read on its
// own, this is an ordinary service; read together with the gateway, it is the
// far side of an edge that crosses a repository boundary.
app.post("/v1/notifications", (req, res) => {
  const body = req.body as NotificationRequest;
  const notification: Notification = { id: "n-1", orderId: body.orderId, status: "queued" };
  res.status(202).json(notification);
});

app.get("/v1/notifications/:id", (req, res) => {
  res.json({ id: req.params.id, orderId: "o-1", status: "sent" });
});

app.listen(9000);
