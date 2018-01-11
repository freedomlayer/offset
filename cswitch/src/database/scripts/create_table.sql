BEGIN;
CREATE TABLE neighbor (
  neighbor_public_key BLOB    NOT NULL,
  remote_maximim_debt BIGINT  NOT NULL,
  maximum_channel     INT     NOT NULL,
  is_enable           BOOLEAN NOT NULL,

  PRIMARY KEY (neighbor_public_key)
);

CREATE TABLE neighbor_token_channel (
  neighbor_public_key     BLOB    NOT NULL,
  token_channel_index     INT     NOT NULL,

  move_token_type         TINYINT NOT NULL,
  move_token_transactions BLOB    NOT NULL,
  move_token_old_token    BLOB    NOT NULL,
  move_token_rand_nonce   BLOB    NOT NULL,

  remote_maximum_debt     BIGINT  NOT NULL,
  local_maximum_debt      BIGINT  NOT NULL,
  remote_pending_debt     BIGINT  NOT NULL,
  local_pending_debt      BIGINT  NOT NULL,
  balance                 BIGINT  NOT NULL,

  local_funds_rand_nonce  BLOB,
  remote_funds_rand_nonce BLOB,

  PRIMARY KEY (neighbor_public_key, token_channel_index),
  FOREIGN KEY (neighbor_public_key)
  REFERENCES neighbor (neighbor_public_key)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE neighbor_local_request (
  neighbor_public_key            BLOB     NOT NULL,
  token_channel_index            SMALLINT NOT NULL,

  request_id                     BLOB     NOT NULL,
  route                          BLOB     NOT NULL,
  request_type                   TINYINT  NOT NULL,
  request_content_hash           BLOB     NOT NULL,
  maximum_response_length        INT      NOT NULL,
  processing_fee_proposal        BIGINT   NOT NULL,
  half_credits_per_byte_proposal INT      NOT NULL,

  PRIMARY KEY (neighbor_public_key, token_channel_index),
  FOREIGN KEY (neighbor_public_key, token_channel_index)
  REFERENCES neighbor_token_channel (neighbor_public_key, token_channel_index)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE friend (
  friend_public_key   BLOB    NOT NULL,
  remote_maximum_debt BLOB    NOT NULL,
  is_enable           BOOLEAN NOT NULL,

  PRIMARY KEY (friend_public_key)
);

CREATE TABLE friend_token_channel (
  friend_public_key       BLOB    NOT NULL,
  move_token_type         BLOB    NOT NULL,
  move_token_transactions BLOB    NOT NULL,
  move_token_old_token    BLOB    NOT NULL,
  move_token_rand_nonce   BLOB    NOT NULL,
  remote_maximum_debt     BIGINT  NOT NULL,
  local_maximum_debt      BIGINT  NOT NULL,
  remote_pending_debt     BIGINT  NOT NULL,
  local_pending_debt      BIGINT  NOT NULL,
  balance                 BIGINT  NOT NULL,

  is_local_enable         BOOLEAN NOT NULL,
  is_remote_enable        BOOLEAN NOT NULL,

  PRIMARY KEY (friend_public_key),
  FOREIGN KEY (friend_public_key) REFERENCES friend (friend_public_key)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE friend_local_request (
  friend_public_key         BLOB NOT NULL,
  request_id                BLOB NOT NULL,
  route                     BLOB NOT NULL,
  mediator_payment_proposal BLOB NOT NULL,
  invoice_id                BLOB NOT NULL,
  destination_payment       BLOB NOT NULL,

  PRIMARY KEY (request_id),
  FOREIGN KEY (friend_public_key) REFERENCES friend (friend_public_key)
    ON DELETE CASCADE
    ON UPDATE CASCADE
);

CREATE TABLE indexing_provider (
  indexing_provider_id BLOB    NOT NULL,
  chain_link           BLOB    NOT NULL,
  last_route           BLOB    NOT NULL,
  is_enable            BOOLEAN NOT NULL,

  PRIMARY KEY (indexing_provider_id)
);
COMMIT;
