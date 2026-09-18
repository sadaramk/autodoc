//! View a.

use db_schema::posts_query;

/// Lists rows for view a.
pub fn list_a() -> &'static str {
    posts_query()
}
