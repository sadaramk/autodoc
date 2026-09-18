-- Catalog and orders.

CREATE TABLE products (
    sku         TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    price_cents INTEGER NOT NULL CHECK (price_cents >= 0),
    active      BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE orders (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    total_cents     INTEGER NOT NULL CHECK (total_cents > 0),
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'paid', 'shipped', 'cancelled')),
    items           JSONB NOT NULL,
    tracking_number TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE order_items (
    order_id    UUID NOT NULL REFERENCES orders (id) ON DELETE CASCADE,
    sku         TEXT NOT NULL,
    quantity    INTEGER NOT NULL CHECK (quantity > 0),
    price_cents INTEGER NOT NULL,
    PRIMARY KEY (order_id, sku),
    FOREIGN KEY (sku) REFERENCES products (sku)
);
