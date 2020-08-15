use rusqlite::{self, params, Connection};

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
        "CREATE TABLE friends(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             sent_local_relays          BLOB NOT NULL,
             -- Last local relays we have sent to the friend
             remote_relays              BLOB NOT NULL,
             -- Friend's relays
             name                       TEXT NOT NULL UNIQUE,
             -- Friend's name
             is_enabled                 BOOL NOT NULL,
             -- Do we allow connectivity with this friend?
             is_consistent              BOOL NOT NULL
             -- Is the channel with this friend consistent?
            );",
        params![],
    )?;

    // friend public key index:
    tx.execute(
        "CREATE UNIQUE INDEX idx_friends_friend_public_key ON friends(friend_public_key);",
        params![],
    )?;

    // public name index:
    tx.execute(
        "CREATE UNIQUE INDEX idx_friends_name ON friends(name);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE currency_configs(
             friend_public_key          BLOB NOT NULL,
             currency                   TEXT NOT NULL,
             rate                       BLOB NOT NULL,
             remote_max_debt            BLOB NOT NULL,
             is_open                    BOOL NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES friends(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_currency_configs ON currency_configs(friend_public_key, currency);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE consistent_channels(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             is_consistent              BOOL NOT NULL CHECK (is_consistent = true),
             is_incoming                BOOL NOT NULL,
             FOREIGN KEY(friend_public_key, is_consistent) 
                REFERENCES friends(friend_public_key, is_consistent)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // friend public key index:
    tx.execute(
        "CREATE UNIQUE INDEX idx_consistent_channels ON consistent_channels(friend_public_key);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE inconsistent_channels(
             friend_public_key              BLOB NOT NULL PRIMARY KEY,
             is_consistent                  BOOL NOT NULL CHECK (is_consistent = false),
             opt_last_incoming_move_token   BLOB,
             local_reset_terms              BLOB NOT NULL,
             opt_remote_reset_terms         BLOB,
             FOREIGN KEY(friend_public_key, is_consistent) 
                REFERENCES friends(friend_public_key, is_consistent)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // friend public key index:
    tx.execute(
        "CREATE UNIQUE INDEX idx_inconsistent_channels ON inconsistent_channels(friend_public_key);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE local_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key) 
                REFERENCES consistent_channels(friend_public_key)
                ON DELETE CASCADE,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES currency_configs(friend_public_key, currency)
                ON DELETE CASCADE,
             PRIMARY KEY(friend_public_key, currency)
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_local_currencies ON local_currencies(friend_public_key, currency);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE remote_currencies(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             FOREIGN KEY(friend_public_key) 
                REFERENCES consistent_channels(friend_public_key)
                ON DELETE CASCADE,
             PRIMARY KEY(friend_public_key, currency)
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_remote_currencies ON remote_currencies(friend_public_key, currency);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE mutual_credits(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             balance                  BLOB NOT NULL,
             local_pending_debt       BLOB NOT NULL,
             remote_pending_debt      BLOB NOT NULL,
             PRIMARY KEY(friend_public_key, currency)
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES local_currencies(friend_public_key, currency)
                ON DELETE CASCADE
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES remote_currencies(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_mutual_credits ON mutual_credits(friend_public_key, currency);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE local_pending_transactions(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             src_hashed_lock          BLOB NOT NULL,
             route                    BLOB NOT NULL,
             dest_payment             BLOB NOT NULL,
             total_dest_payment       BLOB NOT NULL,
             invoice_hash             BLOB NOT NULL,
             hmac                     BLOB NOT NULL,
             left_fees                BLOB NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES mutual_credits(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_local_pending_transactions ON local_pending_transactions(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE remote_pending_transactions(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             src_hashed_lock          BLOB NOT NULL,
             route                    BLOB NOT NULL,
             dest_payment             BLOB NOT NULL,
             total_dest_payment       BLOB NOT NULL,
             invoice_hash             BLOB NOT NULL,
             hmac                     BLOB NOT NULL,
             left_fees                BLOB NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES mutual_credits(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_remote_pending_transactions ON remote_pending_transactions(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_user_requests(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             src_hashed_lock          BLOB NOT NULL,
             route                    BLOB NOT NULL,
             dest_payment             BLOB NOT NULL,
             total_dest_payment       BLOB NOT NULL,
             invoice_hash             BLOB NOT NULL,
             hmac                     BLOB NOT NULL,
             left_fees                BLOB NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES mutual_credits(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_user_requests ON pending_user_requests(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_requests(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             src_hashed_lock          BLOB NOT NULL,
             route                    BLOB NOT NULL,
             dest_payment             BLOB NOT NULL,
             total_dest_payment       BLOB NOT NULL,
             invoice_hash             BLOB NOT NULL,
             hmac                     BLOB NOT NULL,
             left_fees                BLOB NOT NULL,
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES mutual_credits(friend_public_key, currency)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_requests ON pending_requests(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_backwards(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             backwards_type           TEXT CHECK (backwards_type IN ('R', 'C')) NOT NULL,
             -- R: Response, C: Cancel
             FOREIGN KEY(friend_public_key, currency) 
                REFERENCES mutual_credits(friend_public_key, currency)
                ON DELETE CASCADE,
             FOREIGN KEY(request_id) 
                REFERENCES pending_requests(request_id)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_backwards ON pending_backwards(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_backwards_responses(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             backwards_type           TEXT NOT NULL CHECK (backwards_type = 'R'),
             src_hashed_lock          BLOB NOT NULL,
             serial_num               BLOB NOT NULL,
             signature                BLOB NOT NULL,
             FOREIGN KEY(friend_public_key, currency, request_id, backwards_type) 
                REFERENCES pending_backwards(friend_public_key, currency, request_id, backwards_type)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_backwards_responses ON pending_backwards_responses(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_backwards_cancels(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             backwards_type           TEXT NOT NULL CHECK (backwards_type = 'C'),
             FOREIGN KEY(friend_public_key, currency, request_id, backwards_type) 
                REFERENCES pending_backwards(friend_public_key, currency, request_id, backwards_type)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_backwards_cancels ON pending_backwards_cancels(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE friend_relays(
             friend_public_key        BLOB NOT NULL,
             relay_public_key         BLOB NOT NULL,
             address                  TEXT,
             PRIMARY KEY(friend_public_key, relay_public_key),
             FOREIGN KEY(friend_public_key)
                REFERENCES friends(friend_public_key)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_friend_relays ON friend_relays(friend_public_key, relay_public_key);",
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
        "CREATE UNIQUE INDEX idx_relays ON relays(relay_public_key);",
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
        "CREATE UNIQUE INDEX idx_index_servers ON index_servers(index_public_key);",
        params![],
    )?;

    // TODO:
    // - Reconsider primary key
    // - Add a new table for pending requests?
    // - Add indices
    tx.execute(
        "CREATE TABLE payments(
             counter         BLOB NOT NULL PRIMARY KEY,
             payment_id      BLOB NOT NULL,
             amount          BLOB NOT NULL
            );",
        params![],
    )?;

    // TODO:
    // - Reconsider primary key
    // - Add fields for total amount paid?
    // - Add indices
    tx.execute(
        "CREATE TABLE invoices (
             counter         BLOB NOT NULL PRIMARY KEY,
             invoice_id      BLOB NOT NULL,
             currency        TEXT NOT NULL,
             amount          BLOB NOT NULL,
             desription      TEXT NOT NULL,
             status          BLOB NOT NULL
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
