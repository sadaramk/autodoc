-- A fixture's schema. It belongs to the sample repository this project's tests
-- run against, not to this project, and documenting it as this project's data
-- model is what nunki did to its own published book.
CREATE TABLE invoices (
    id UUID PRIMARY KEY,
    order_id UUID NOT NULL,
    total_cents INTEGER NOT NULL
);

CREATE TABLE orders (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL
);
