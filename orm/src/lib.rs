#[macro_use]
pub extern crate diesel;

pub mod schema;
pub mod models;

use diesel::prelude::*;
use diesel::pg::PgConnection;
use dotenv::dotenv;
use std::env;

pub fn establish_connection() -> Result<PgConnection, String> {
    dotenv().map_err(|e| format!("{}", e))?;

    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL must be set".to_string())?;
    PgConnection::establish(&database_url)
        .map_err(|e| format!("Error connecting to {}: {}", database_url, e))
}

// tests
#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_establish() {
        establish_connection().unwrap();
    }

    #[test]
    fn test_insert() {
        let value = models::NewUser {
            full_name: "John Doe".to_string(),
        };
        let conn = establish_connection().unwrap();
        diesel::insert_into(schema::users::table)
            .values(&value)
            .execute(&conn)
            .unwrap();
    }
}