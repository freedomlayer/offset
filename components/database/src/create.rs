use rusqlite::{self, params, Connection};

/// Create a new Offset database
fn create_database(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    // TODO: A better way to do this?
    tx.execute("PRAGMA foreign_keys = ON;", params![])?;

    // This table serves as an "archive" table, containing active and non active applications.
    // Some applications can not be removed from the database, because they might be still used in
    // the events table (As part of an invoice or payment event)
    tx.execute(
        "CREATE TABLE applications (
             app_public_key  BLOB NOT NULL PRIMARY KEY,
             name            TEXT NOT NULL UNIQUE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_applications ON applications(app_public_key, name);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE active_applications (
             app_public_key             BLOB NOT NULL PRIMARY KEY,
             last_online                INTEGER NOT NULL,
             is_enabled                 BOOL NOT NULL,

             perm_info_docs             BOOL NOT NULL,
             perm_info_friends          BOOL NOT NULL,
             perm_routes                BOOL NOT NULL,
             perm_buy                   BOOL NOT NULL,
             perm_sell                  BOOL NOT NULL,
             perm_config_card           BOOL NOT NULL,
             perm_config_access         BOOL NOT NULL,

             FOREIGN KEY(app_public_key) 
                REFERENCES applications(app_public_key)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // Single row is enforced in this table according to https://stackoverflow.com/a/33104119
    tx.execute(
        "CREATE TABLE funder(
             id                       INTEGER PRIMARY KEY CHECK (id = 0), -- enforce single row
             version                  INTEGER NOT NULL,  -- Database version
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
             PRIMARY KEY(friend_public_key, currency),
             FOREIGN KEY(friend_public_key) 
                REFERENCES friends(friend_public_key)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_currency_configs ON currency_configs(friend_public_key, currency);",
        params![],
    )?;

    // TODO: Add last incoming/outgoing move token?
    // Maybe we should add to friends table directly?
    tx.execute(
        "CREATE TABLE consistent_channels(
             friend_public_key          BLOB NOT NULL PRIMARY KEY,
             is_consistent              BOOL NOT NULL 
                                        CHECK (is_consistent = true)
                                        DEFAULT true,
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
             is_consistent                  BOOL NOT NULL 
                                            CHECK (is_consistent = false)
                                            DEFAULT false,
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
             in_fees                  BLOB NOT NULL,
             out_fees                 BLOB NOT NULL,
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
        "CREATE TABLE local_open_transactions(
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
        "CREATE UNIQUE INDEX idx_local_open_transactions ON local_open_transactions(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE remote_open_transactions(
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
        "CREATE UNIQUE INDEX idx_remote_open_transactions ON remote_open_transactions(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_user_requests(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             generation               BLOB NOT NULL UNIQUE,
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
        "CREATE UNIQUE INDEX idx_pending_user_requests_request_id ON pending_user_requests(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_user_requests_generation ON pending_user_requests(generation);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_requests(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             generation               BLOB NOT NULL UNIQUE,
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
        "CREATE UNIQUE INDEX idx_pending_requests_request_id ON pending_requests(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_requests_generation ON pending_requests(generation);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_backwards(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             generation               BLOB NOT NULL UNIQUE,
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
        "CREATE UNIQUE INDEX idx_pending_backwards_request_id ON pending_backwards(request_id);",
        params![],
    )?;

    tx.execute(
        "CREATE UNIQUE INDEX idx_pending_backwards_generation ON pending_backwards(generation);",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE pending_backwards_responses(
             friend_public_key        BLOB NOT NULL,
             currency                 TEXT NOT NULL,
             request_id               BLOB NOT NULL PRIMARY KEY,
             backwards_type           TEXT NOT NULL 
                                      CHECK (backwards_type = 'R')
                                      DEFAULT 'R',
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
             backwards_type           TEXT NOT NULL 
                                      CHECK (backwards_type = 'C')
                                      DEFAULT 'C',
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

    // Documents table, allowing total order on all items payments and invoices
    tx.execute(
        "CREATE TABLE events(
             counter      BLOB NOT NULL PRIMARY KEY,
             time         INTEGER NOT NULL,
             amount       BLOB NOT NULL,
             in_fees      BLOB NOT NULL,
             out_fees     BLOB NOT NULL,
             event_type   TEXT CHECK (event_type IN ('P', 'I', 'R')) NOT NULL
             -- Event type: P: Payment, I: Invoice, R: Friend Removal
            );",
        params![],
    )?;

    // TODO:
    // - Add a new table for pending requests?
    // - Add indices
    // More work needed here:
    tx.execute(
        "CREATE TABLE payment_events(
             counter             BLOB NOT NULL,
             event_type          TEXT CHECK (event_type = 'P') 
                                 DEFAULT 'P' 
                                 NOT NULL,
             payment_id          BLOB NOT NULL PRIMARY KEY,
             currency            TEXT NOT NULL,
             total_dest_payment  BLOB NOT NULL,
             dest_payment        BLOB NOT NULL,
             FOREIGN KEY(counter, event_type) 
                REFERENCES events(counter, event_type)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    // TODO:
    // - Add fields for total amount paid?
    // - Add indices
    // More work needed here:
    tx.execute(
        "CREATE TABLE invoice_events (
             counter         BLOB NOT NULL,
             event_type      TEXT CHECK (event_type = 'I') 
                             DEFAULT 'I'
                             NOT NULL,
             invoice_id      BLOB NOT NULL PRIMARY KEY,
             currency        TEXT NOT NULL,
             amount          BLOB NOT NULL,
             description     TEXT NOT NULL,
             status          BLOB NOT NULL,
             FOREIGN KEY(counter, event_type) 
                REFERENCES events(counter, event_type)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.execute(
        "CREATE TABLE friend_removal_events (
             counter         BLOB NOT NULL,
             event_type      TEXT CHECK (event_type = 'R') 
                             DEFAULT 'R'
                             NOT NULL,
             FOREIGN KEY(counter, event_type) 
                REFERENCES events(counter, event_type)
                ON DELETE CASCADE
            );",
        params![],
    )?;

    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_db() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_database(&mut conn).unwrap();
    }
}
