use diesel::Queryable;
use diesel::Insertable;

use super::schema::users;

#[derive(Queryable)]
pub struct User {
    pub id: i32,
    pub full_name: String,
    pub telegram_id: Option<String>,
    pub phone: Option<String>,
}

#[derive(Insertable)]
#[table_name="users"]
pub struct NewUser {
    pub full_name: String
}
