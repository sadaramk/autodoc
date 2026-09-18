// @generated automatically by Diesel CLI.

diesel::table! {
    projects (id) {
        id -> Int4,
        name -> Text,
        archived_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    tasks (id) {
        id -> Int4,
        project_id -> Int4,
        title -> Varchar,
        state -> Varchar,
    }
}

diesel::joinable!(tasks -> projects (project_id));
