table! {
    users (id) {
        id -> Int4,
        full_name -> Varchar,
        telegram_id -> Nullable<Varchar>,
        phone -> Nullable<Varchar>,
    }
}
