BEGIN;

--------------- Table Definitions ---------------

CREATE TABLE neighbor (
  neighbor_public_key    BLOB    NOT NULL,
  wanted_remote_max_debt BIGINT  NOT NULL,
  wanted_max_channel     INT     NOT NULL,
  status                 TINYINT NOT NULL,

  PRIMARY KEY (neighbor_public_key)
);

CREATE TABLE neighbor_token_channel (
  id                      BIGINT  NOT NULL,
  neighbor_public_key     BLOB    NOT NULL,
  token_channel_index     INT     NOT NULL,

  move_token_type         TINYINT NOT NULL,
  move_token_transactions BLOB    NOT NULL,
  move_token_old_token    BLOB    NOT NULL,
  move_token_rand_nonce   BLOB    NOT NULL,

  remote_max_debt         BIGINT  NOT NULL,
  local_max_debt          BIGINT  NOT NULL,
  remote_pending_debt     BIGINT  NOT NULL,
  local_pending_debt      BIGINT  NOT NULL,
  balance                 BIGINT  NOT NULL,

  local_invoice_id        BLOB,
  remote_invoice_id       BLOB,

  PRIMARY KEY (id),
  UNIQUE (neighbor_public_key, token_channel_index),
  FOREIGN KEY (neighbor_public_key)
  REFERENCES neighbor (neighbor_public_key)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE neighbor_request (
  request_id                  BLOB    NOT NULL,
  from_neighbor_token_channel BIGINT,
  to_neighbor_token_channel   BIGINT,
  route                       BLOB    NOT NULL,
  request_type                TINYINT NOT NULL,
  request_content_hash        BLOB    NOT NULL,
  max_response_length         INT     NOT NULL,
  processing_fee_proposal     BIGINT  NOT NULL,
  credits_per_byte_proposal   INT     NOT NULL,

  PRIMARY KEY (request_id),
  FOREIGN KEY (from_neighbor_token_channel, to_neighbor_token_channel)
  REFERENCES neighbor_token_channel (id, id)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE friend (
  friend_public_key      BLOB    NOT NULL,
  wanted_remote_max_debt BLOB    NOT NULL,
  status                 TINYINT NOT NULL,

  PRIMARY KEY (friend_public_key)
);

CREATE TABLE friend_token_channel (
  friend_public_key       BLOB    NOT NULL,
  move_token_type         TINYINT NOT NULL,
  move_token_transactions BLOB    NOT NULL,
  move_token_old_token    BLOB    NOT NULL,
  move_token_rand_nonce   BLOB    NOT NULL,
  remote_max_debt         BIGINT  NOT NULL,
  local_max_debt          BIGINT  NOT NULL,
  remote_pending_debt     BIGINT  NOT NULL,
  local_pending_debt      BIGINT  NOT NULL,
  balance                 BIGINT  NOT NULL,

  local_state             TINYINT NOT NULL,
  remote_state            TINYINT NOT NULL,

  PRIMARY KEY (friend_public_key),
  FOREIGN KEY (friend_public_key) REFERENCES friend (friend_public_key)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE friend_request (
  request_id                BLOB   NOT NULL,
  friend_public_key         BLOB   NOT NULL,
  from_friend_token_channel BIGINT,
  to_friend_token_channel   BIGINT,
  route                     BLOB   NOT NULL,
  mediator_payment_proposal BIGINT NOT NULL,
  invoice_id                BLOB   NOT NULL,
  destination_payment       BLOB   NOT NULL,

  PRIMARY KEY (request_id),
  FOREIGN KEY (friend_public_key) REFERENCES friend (friend_public_key)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE indexing_provider (
  indexing_provider_id BLOB    NOT NULL,
  chain_link           BLOB    NOT NULL,
  last_route           BLOB    NOT NULL,
  status               TINYINT NOT NULL,

  PRIMARY KEY (indexing_provider_id)
);

-------------- Trigger Definitions --------------

CREATE TRIGGER neighbor_request_insert_validation
  BEFORE INSERT
  ON neighbor_request
  FOR EACH ROW
  WHEN new.to_neighbor_token_channel ISNULL
       AND new.from_neighbor_token_channel ISNULL
BEGIN
  SELECT RAISE(ABORT, 'invalid insertion: both null');
END;

CREATE TRIGGER neighbor_request_update_validation
  AFTER UPDATE
  ON neighbor_request
  FOR EACH ROW
BEGIN
  -- FIXME: Delete invalid records or rollback,
  -- or use the same behavior as INSERT validation
  DELETE FROM neighbor_request
  WHERE from_neighbor_token_channel ISNULL
        AND to_neighbor_token_channel ISNULL;
END;

CREATE TRIGGER friend_request_insert_validation
  BEFORE INSERT
  ON friend_request
  FOR EACH ROW
  WHEN new.to_friend_token_channel ISNULL
       AND new.from_friend_token_channel ISNULL
BEGIN
  SELECT RAISE(ABORT, 'invalid insertion: both null');
END;

CREATE TRIGGER friend_request_update_validation
  AFTER UPDATE
  ON friend_request
  FOR EACH ROW
BEGIN
  -- FIXME: Delete invalid records or rollback,
  -- or use the same behavior as INSERT validation
  DELETE FROM friend_request
  WHERE from_friend_token_channel ISNULL
        AND to_friend_token_channel ISNULL;
END;

COMMIT;
