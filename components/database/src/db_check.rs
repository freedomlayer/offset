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
             friend_public_key        BLOB NOT NULL PRIMARY KEY,
             sent_local_relays        BLOB NOT NULL,
             name                     TEXT NOT NULL
            );",
        params![],
    )?;

    // TODO: Fix primary key?
    conn.execute(
        "CREATE TABLE local_active_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key) 
                REFERENCES friends(friend_public_key)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // TODO: Fix primary key?
    conn.execute(
        "CREATE TABLE remote_active_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key) 
                REFERENCES friends(friend_public_key)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE active_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES local_active_currencies(friend_public_key, currency)
                ON DELETE RESTRICT
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES remote_active_currencies(friend_public_key, currency)
                ON DELETE RESTRICT
            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE local_pending_transactions(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES active_currencies(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE remote_pending_transactions(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES active_currencies(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE currency_configs(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             currency                   TEXT NOT NULL,
             rate                       BLOB NOT NULL,
             remote_max_debt            BLOB NOT NULL,
             is_open                    BOOL NOT NULL,
             is_enabled                 BOOL NOT NULL,
             is_consistent              BOOL NOT NULL,
             is_token_channel_incoming  BOOL
            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE pending_user_requests(
             friend_public_key        BLOB NOT NULL PRIMARY KEY
            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE pending_requests(
             friend_public_key        BLOB NOT NULL,
             currency                 BLOB NOT NULL PRIMARY KEY
            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE pending_responses(
             friend_public_key        BLOB NOT NULL PRIMARY KEY
            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE friend_relays(
             friend_public_key        BLOB NOT NULL,
             relay_public_key         BLOB NOT NULL,
             address                  TEXT
            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE relays(
             relay_public_key         BLOB NOT NULL PRIMARY KEY,
             address                  TEXT,
             name                     TEXT
            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE index_servers(
             index_public_key         BLOB NOT NULL PRIMARY KEY,
             address                  TEXT,
             name                     TEXT
            );",
        params![],
    )?;

    conn.execute(
        "CREATE TABLE currencies(
             friend_public_key         BLOB NOT NULL PRIMARY KEY

            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE payments(
             counter         BLOB NOT NULL PRIMARY KEY

            );",
        params![],
    )?;

    // TODO:
    conn.execute(
        "CREATE TABLE invoices (
             counter         BLOB NOT NULL PRIMARY KEY

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

        create_database(&conn).unwrap();

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
