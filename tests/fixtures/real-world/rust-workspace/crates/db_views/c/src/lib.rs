//! View c.

use db_schema::posts_query;

/// Lists rows for view c.
pub fn list_c() -> &'static str {
    posts_query()
}
