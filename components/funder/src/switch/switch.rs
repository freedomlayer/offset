use derive_more::From;
use std::collections::{HashMap, HashSet};

use common::async_rpc::OpError;

use proto::app_server::messages::RelayAddress;
use proto::crypto::PublicKey;
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendMessage, MoveToken, RelaysUpdate, RequestSendFundsOp,
    ResponseSendFundsOp,
};
use proto::index_server::messages::IndexMutation;

use crate::switch::types::{SwitchDbClient, SwitchState};
use crate::token_channel::{TcDbClient, TcStatus};

#[derive(Debug, From)]
pub enum SwitchError {
    FriendAlreadyOnline,
    GenerationOverflow,
    OpError(OpError),
}

/*
#[derive(Debug)]
pub struct SwitchOutput {
    opt_move_tokens: HashMap<PublicKey, MoveToken>,
    add_connections: Vec<(u128, PublicKey)>,
    remove_connections: Vec<(u128, PublicKey)>,
}
*/

// TODO: Make this structure more ergonomic to use:
#[derive(Debug)]
pub struct SwitchOutput {
    pub friends_messages: HashMap<PublicKey, FriendMessage>,
    pub index_mutations: Vec<IndexMutation>,
    pub updated_remote_relays: Vec<PublicKey>,
    pub incoming_requests: Vec<RequestSendFundsOp>,
    pub incoming_responses: Vec<ResponseSendFundsOp>,
    pub incoming_cancels: Vec<CancelSendFundsOp>,
}

pub async fn set_friend_online(
    switch_db_client: &mut impl SwitchDbClient,
    switch_state: &mut SwitchState,
    friend_public_key: PublicKey,
) -> Result<SwitchOutput, SwitchError> {
    if switch_state.liveness.is_online(&friend_public_key) {
        // The friend is already marked as online!
        return Err(SwitchError::FriendAlreadyOnline);
    }

    if let Some(max_generation) = switch_db_client
        .get_max_sent_relays_generation(friend_public_key.clone())
        .await?
    {
        let generation = max_generation
            .checked_add(1)
            .ok_or(SwitchError::GenerationOverflow)?;

        let local_relays = switch_db_client.get_local_relays().await?;
        let mut relays = Vec::new();
        for local_relay in local_relays {
            relays.push(RelayAddress {
                public_key: local_relay.public_key,
                address: local_relay.address,
                // TODO: We need to randomly generate relay ports here
                // Should we use u128, or a different type? How will it be json serialized?
                port: todo!(),
            })
        }

        let relays_update = RelaysUpdate {
            generation,
            relays: relays.clone(),
        };

        switch_db_client
            .update_sent_relays(friend_public_key, generation, relays)
            .await?;

        // Actually send relays here:
        todo!();
    }

    // TODO: Possibly resend relays updates
    // - If there are unpacked changes to relays:
    // -
    todo!();

    // TODO: Attempt to reconstruct and send last outgoing message, if exists.
    match switch_db_client
        .tc_db_client(friend_public_key.clone())
        .get_tc_status()
        .await?
    {
        TcStatus::ConsistentIn(_) => {
            // Create an outgoing move token if we have something to send.
            todo!();
        }
        TcStatus::ConsistentOut(move_token_out, _opt_move_token_hashed_in) => {
            // Resend outgoing move token.
            // Resend with "request token back = true" if we have more things to send.
            todo!();
        }
        TcStatus::Inconsistent(..) => {
            // Resend reset terms
            todo!();
        }
    }

    // TODO: Maybe change liveness to work directly, without the mutations concept?
    // Maybe we don't need atomicity for the liveness struct?
    switch_state.liveness.set_online(friend_public_key.clone());
}

pub async fn set_friend_offline(
    _switch_db_client: &mut impl SwitchDbClient,
    _switch_state: &mut SwitchState,
    _friend_public_key: PublicKey,
) -> Result<SwitchOutput, SwitchError> {
    todo!();
}

pub async fn send_request(
    _switch_db_client: &mut impl SwitchDbClient,
    _request: RequestSendFundsOp,
) -> Result<SwitchOutput, SwitchError> {
    // TODO:
    // - Add request to relevant user pending requests queue (According to friend on route)
    // - For the relevant friend: If token is present:
    //      - Compose a friend move token message
    // - If the token is not present:
    //      - Compose a request token message.
    todo!();
}

pub async fn add_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn remove_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn set_remote_max_debt(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
    _remote_max_debt: u128,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn open_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn close_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn update_local_relays(
    _switch_db_client: &mut impl SwitchDbClient,
    _local_relays: HashMap<PublicKey, RelayAddress>,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}

pub async fn incoming_friend_message(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _friend_message: FriendMessage,
) -> Result<SwitchOutput, SwitchError> {
    // TODO
    todo!();
}
