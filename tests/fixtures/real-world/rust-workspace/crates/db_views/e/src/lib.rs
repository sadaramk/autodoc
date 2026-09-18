//! View e.

use db_schema::posts_query;

/// Lists rows for view e.
pub fn list_e() -> &'static str {
    posts_query()
}
