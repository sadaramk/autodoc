-- Payment attempts recorded by the payments service.

CREATE TABLE payments (
    id           BIGSERIAL PRIMARY KEY,
    order_id     UUID NOT NULL,
    charge_id    TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('succeeded', 'failed')),
    amount_cents BIGINT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE payments
    ADD CONSTRAINT payments_order_fk FOREIGN KEY (order_id) REFERENCES orders (id),
    ADD CONSTRAINT payments_charge_unique UNIQUE (charge_id);
