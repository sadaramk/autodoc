//! View d.

use db_schema::posts_query;

/// Lists rows for view d.
pub fn list_d() -> &'static str {
    posts_query()
}
