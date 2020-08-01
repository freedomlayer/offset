use rusqlite::{self, params, Connection};

#[allow(unused)]
fn create_database(conn: &Connection) -> rusqlite::Result<()> {
    // A single row table. TODO: How to enforce a single row?
    // See also: https://stackoverflow.com/questions/2300356/using-a-single-row-configuration-table-in-sql-server-database-bad-idea
    conn.execute(
        "CREATE TABLE funder(
             local_public_key         BLOB NOT NULL PRIMARY KEY


            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE friends(
             friend_public_key         BLOB NOT NULL PRIMARY KEY


            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE currencies(
             public_key         BLOB NOT NULL PRIMARY KEY


            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE payments(
             uid         BLOB NOT NULL PRIMARY KEY


            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE invoices (
             uid         BLOB NOT NULL PRIMARY KEY


            );",
        params![],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{self, params};

    #[derive(Debug)]
    struct Person {
        id: i32,
        name: String,
        data: Option<Vec<u8>>,
    }

    fn inner_db_check() -> rusqlite::Result<()> {
        let conn = Connection::open_in_memory()?;

        conn.execute(
            "CREATE TABLE person (
                  id              INTEGER PRIMARY KEY,
                  name            TEXT NOT NULL,
                  data            BLOB
                  )",
            params![],
        )?;
        let me = Person {
            id: 0,
            name: "Steven".to_string(),
            data: None,
        };
        conn.execute(
            "INSERT INTO person (name, data) VALUES (?1, ?2)",
            params![me.name, me.data],
        )?;

        let mut stmt = conn.prepare("SELECT id, name, data FROM person")?;
        let person_iter = stmt.query_map(params![], |row| {
            Ok(Person {
                id: row.get(0)?,
                name: row.get(1)?,
                data: row.get(2)?,
            })
        })?;

        for person in person_iter {
            println!("Found person {:?}", person.unwrap());
        }
        Ok(())
    }

    #[test]
    fn test_db_check() {
        inner_db_check().unwrap();
    }
}
