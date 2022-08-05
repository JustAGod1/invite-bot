use std::sync::{Arc, Mutex};
use sqlite::{Connection, Cursor, OpenFlags, Row, State, Value};

pub struct User {
    pub full_name: String,
    pub telegram_id: Option<String>,
}

pub struct DBConn {
   conn: Mutex<Connection>
}

unsafe impl Send for DBConn {}
unsafe impl Sync for DBConn {}


impl DBConn {

    pub fn open() -> Result<DBConn, String> {
        sqlite::open("db.sqlite")
            .map(|a| DBConn {
            conn: Mutex::new(a)
        })
        .map_err(|a| a.to_string())
    }

    fn build_from_cursor(cursor: &Row) -> Result<User, String> {
        Ok(User {
            full_name: cursor.try_get::<String, _>(0).map_err(|a| a.to_string())?,
            telegram_id: cursor.try_get::<Option<String>, _>(1).map_err(|a| a.to_string())?,
        })
    }

    pub fn find_by_full_name(&self, name: &str) -> Result<Option<User>, String> {
        let conn = self.conn.lock()
            .map_err(|a| a.to_string())?;
        let mut cursor = conn
            .prepare("SELECT full_name, telegram_id FROM users WHERE full_name = ? LIMIT 1")
            .map_err(|a| a.to_string())?
            .into_cursor()
            .bind(&[Value::String(name.to_string())])
            .map_err(|a| a.to_string())?;

        if let Some(Ok(row)) = cursor.next() {
            let user = Self::build_from_cursor(&row)?;

            return Ok(Some(user));
        }

        Ok(None)
    }

    pub fn find_by_telegram_id(&self, id: u64) -> Result<Option<User>, String> {
        let conn = self.conn.lock()
            .map_err(|a| a.to_string())?;
        let mut cursor = conn
            .prepare("SELECT full_name, telegram_id FROM users WHERE telegram_id = ? LIMIT 1")
            .map_err(|a| a.to_string())?
            .into_cursor()
            .bind(&[Value::String(id.to_string())])
            .map_err(|a| a.to_string())?;

        if let Some(Ok(row)) = cursor.next() {
            let user = Self::build_from_cursor(&row)?;

            return Ok(Some(user));
        }

        Ok(None)
    }


    pub fn query_all_names(&self) -> Result<Vec<String>, String> {
        let conn = self.conn.lock()
            .map_err(|a| a.to_string())?;
        let mut cursor = conn
            .prepare("SELECT full_name FROM users")
            .map_err(|a| a.to_string())?
            .into_cursor()
            .bind(&[])
            .map_err(|a| a.to_string())?;

        let mut users = Vec::new();
        while let Some(Ok(row)) = cursor.next() {
            users.push(row.try_get(0).map_err(|a| a.to_string())?);
        }

        Ok(users)
    }


    pub fn insert_telegram_data(&self, fullname: String, telegram_id: u64) -> Result<(), String>{
        let conn = self.conn.lock()
            .map_err(|a| a.to_string())?;

        let mut statement = conn.prepare("UPDATE users SET telegram_id = ? WHERE full_name = ? AND telegram_id IS NULL")
            .map_err(|a| a.to_string())?
            .bind(1, telegram_id.to_string().as_str()).map_err(|a| a.to_string())?
            .bind(2, fullname.to_string().as_str()).map_err(|a| a.to_string())?;


        loop {
            match statement.next() {
                Ok(v) => {
                    if matches!(v, State::Done) {
                        break
                    }
                }
                Err(a) => {
                    return Err(a.to_string());
                }
            }
        }

        Ok(())
    }

    pub fn delete_telegram_id(&self, fullname: &str) -> Result<(), String> {
        let conn = self.conn.lock()
            .map_err(|a| a.to_string())?;

        let mut statement = conn.prepare("UPDATE users SET telegram_id = NULL WHERE full_name = ?")
            .map_err(|a| a.to_string())?
            .bind(1, fullname.to_string().as_str()).map_err(|a| a.to_string())?;

        loop {
            match statement.next() {
                Ok(v) => {
                    if matches!(v, State::Done) {
                        break
                    }
                }
                Err(a) => {
                    return Err(a.to_string());
                }
            }
        }

        Ok(())
    }


}