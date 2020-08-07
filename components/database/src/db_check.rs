use rusqlite::{self, params, Connection};

const TC_CONSISTENT_IN: &str = &"C-IN";
const TC_CONSISTENT_OUT: &str = &"C-OUT";
const TC_INCONSISTENT: &str = &"I";

#[allow(unused)]
fn create_database(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    // TODO: A better way to do this?
    tx.execute("PRAGMA foreign_keys = ON;", params![])?;

    // Single row is enforced in this table according to https://stackoverflow.com/a/33104119
    tx.execute(
        "CREATE TABLE funder(
             id                       INTEGER PRIMARY KEY CHECK (id = 0), -- enforce single row
             local_public_key         BLOB NOT NULL
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE token_channel_statuses(
            status                      TEXT PRIMARY KEY
        );",
        params![],
    )?;

    // Initialize token_channel_status table:
    for value in &[TC_CONSISTENT_IN, TC_CONSISTENT_OUT, TC_INCONSISTENT] {
        tx.execute(
            "INSERT INTO token_channel_statuses VALUES (?1);",
            params![value],
        )?;
    }

    // TODO: Add index on primary key?
    tx.execute(
        "CREATE TABLE friends(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             sent_local_relays          BLOB NOT NULL,
             -- Last local relays we have sent to the friend
             remote_relays              BLOB NOT NULL,
             -- Friend's relays
             name                       TEXT NOT NULL,
             -- Friend's name
             is_enabled                 BOOL NOT NULL,
             -- Do we allow connectivity with this friend?
             is_consistent              BOOL NOT NULL,
             -- Is the channel with this friend consistent?
             token_channel_status       TEXT NOT NULL,
             -- Current status of the token channel
             FOREIGN KEY(token_channel_status)
                REFERENCES token_channel_statuses(status)
            );",
        params![],
    )?;

    // TODO: Make sure that consistent and inconsistent channels tables are disjoint, but still
    // refer to the friends table. Add constraints to make sure that when an item is removed from
    // the consistent_channels table, active currencies will also be affected.

    tx.execute(
        "CREATE TABLE consistent_channels(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             is_incoming                BOOL NOT NULL,
             FOREIGN KEY(friend_public_key)
                REFERENCES friends(friend_public_key)
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE inconsistent_channels(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             is_incoming                BOOL NOT NULL,
             FOREIGN KEY(friend_public_key)
                REFERENCES friends(friend_public_key)
            );",
        params![],
    )?;

    // Should only exist if relevant token channel is alive
    tx.execute(
        "CREATE TABLE local_active_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key) 
                REFERENCES friends(friend_public_key)
                ON DELETE CASCADE,
             PRIMARY KEY(friend_public_key, currency)
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE remote_active_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key) 
                REFERENCES friends(friend_public_key)
                ON DELETE CASCADE,
             PRIMARY KEY(friend_public_key, currency)
            );",
        params![],
    )?;

    // TODO:
    tx.execute(
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
    tx.execute(
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
    tx.execute(
        "CREATE TABLE remote_pending_transactions(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES active_currencies(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE currency_configs(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             currency                   TEXT NOT NULL,
             rate                       BLOB NOT NULL,
             remote_max_debt            BLOB NOT NULL,
             is_open                    BOOL NOT NULL
            );",
        params![],
    )?;

    // TODO:
    tx.execute(
        "CREATE TABLE pending_user_requests(
             friend_public_key        BLOB NOT NULL PRIMARY KEY
            );",
        params![],
    )?;

    // TODO:
    tx.execute(
        "CREATE TABLE pending_requests(
             friend_public_key        BLOB NOT NULL,
             currency                 BLOB NOT NULL PRIMARY KEY
            );",
        params![],
    )?;

    // TODO:
    tx.execute(
        "CREATE TABLE pending_responses(
             friend_public_key        BLOB NOT NULL PRIMARY KEY
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE friend_relays(
             friend_public_key        BLOB NOT NULL,
             relay_public_key         BLOB NOT NULL,
             address                  TEXT
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE relays(
             relay_public_key         BLOB NOT NULL PRIMARY KEY,
             address                  TEXT,
             name                     TEXT
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE index_servers(
             index_public_key         BLOB NOT NULL PRIMARY KEY,
             address                  TEXT,
             name                     TEXT
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE currencies(
             friend_public_key         BLOB NOT NULL PRIMARY KEY

            );",
        params![],
    )?;

    // TODO:
    tx.execute(
        "CREATE TABLE payments(
             counter         BLOB NOT NULL PRIMARY KEY

            );",
        params![],
    )?;

    // TODO:
    tx.execute(
        "CREATE TABLE invoices (
             counter         BLOB NOT NULL PRIMARY KEY

            );",
        params![],
    )?;

    tx.commit()
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
        let mut conn = Connection::open_in_memory()?;

        create_database(&mut conn).unwrap();

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
