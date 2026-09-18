mod models;
mod schema;

use diesel::prelude::*;

use crate::models::TaskState;
use crate::schema::tasks;

/// Completes a task that is in progress.
fn finish_task(conn: &mut PgConnection, task_id: i32) -> QueryResult<usize> {
    diesel::update(tasks::table.find(task_id))
        .filter(tasks::state.eq(TaskState::Doing))
        .set(tasks::state.eq(TaskState::Done))
        .execute(conn)
}

fn main() {
    let mut conn = PgConnection::establish("postgres://localhost/tasks").unwrap();
    let _ = finish_task(&mut conn, 1);
}
