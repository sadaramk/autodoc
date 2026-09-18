import React from "react";
import { createRoot } from "react-dom/client";
import { Checkout } from "./pages/Checkout";

/** Mounts the storefront into the #root element. */
function bootstrap(): void {
  const el = document.getElementById("root");
  if (!el) throw new Error("missing #root element");
  createRoot(el).render(
    <React.StrictMode>
      <Checkout />
    </React.StrictMode>,
  );
}

bootstrap();
