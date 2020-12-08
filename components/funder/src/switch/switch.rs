use derive_more::From;
use std::collections::{HashMap, HashSet};

use common::async_rpc::OpError;

use proto::app_server::messages::RelayAddress;
use proto::crypto::PublicKey;
use proto::funder::messages::{Currency, MoveToken, RelaysUpdate, RequestSendFundsOp};

use crate::switch::types::SwitchDbClient;

#[derive(Debug, From)]
pub enum SwitchError {
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

pub async fn send_request(
    _switch_db_client: &mut impl SwitchDbClient,
    _request: RequestSendFundsOp,
) -> Result<(PublicKey, MoveToken), SwitchError> {
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
) -> Result<MoveToken, SwitchError> {
    // TODO
    todo!();
}

pub async fn remove_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<MoveToken, SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn set_remote_max_debt(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
    _remote_max_debt: u128,
) -> Result<(), SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn open_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<(), SwitchError> {
    // TODO
    todo!();
}

// TODO: Do we need to send an update to index client somehow?
pub async fn close_currency(
    _switch_db_client: &mut impl SwitchDbClient,
    _friend_public_key: PublicKey,
    _currency: Currency,
) -> Result<(), SwitchError> {
    // TODO
    todo!();
}

/*
// TODO
type FriendListen = (PublicKey, RelayAddress);

#[derive(Debug)]
pub struct UpdateLocalRelaysOutput {
    // Optional outgoing "relays update" message:
    opt_relays_update: Option<RelaysUpdate>,
    add_friend_listens: HashSet<FriendListen>,
    remove_friend_listens: HashSet<FriendListen>,
}

pub async fn update_local_relays(
    _switch_db_client: &mut impl SwitchDbClient,
    _local_relays: HashMap<PublicKey, RelayAddress>,
) -> Result<UpdateLocalRelaysOutput, SwitchError> {
    // TODO
    todo!();
}
*/
