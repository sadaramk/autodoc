//! View b.

use db_schema::posts_query;

/// Lists rows for view b.
pub fn list_b() -> &'static str {
    posts_query()
}
