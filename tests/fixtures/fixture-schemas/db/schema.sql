-- The application's own schema.
CREATE TABLE catalogue_items (
    id UUID PRIMARY KEY,
    sku TEXT NOT NULL,
    title TEXT NOT NULL
);
