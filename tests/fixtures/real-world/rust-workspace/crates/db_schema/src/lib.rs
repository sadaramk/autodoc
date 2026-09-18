//! Database schema and queries.

use diesel::prelude::*;

/// Query listing posts.
pub fn posts_query() -> &'static str {
    "SELECT id, title FROM post"
}
