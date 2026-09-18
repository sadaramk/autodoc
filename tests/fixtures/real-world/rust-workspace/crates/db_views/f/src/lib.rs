//! View f.

use db_schema::posts_query;

/// Lists rows for view f.
pub fn list_f() -> &'static str {
    posts_query()
}
