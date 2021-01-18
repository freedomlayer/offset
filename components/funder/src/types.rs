use common::conn::{BoxFuture, BoxStream, ConnPairVec};
use common::ser_utils::ser_b64;

use proto::crypto::{DhPublicKey, PlainLock, PublicKey, Signature, Uid};

use proto::app_server::messages::RelayAddress;
use proto::funder::messages::{
    CancelSendFundsOp, ChannelerUpdateFriend, Currency, FriendMessage, FunderIncomingControl,
    FunderOutgoingControl, MoveToken, PendingTransaction, RequestSendFundsOp, ResponseSendFundsOp,
    TokenInfo, UnsignedResponseSendFundsOp,
};

use signature::signature_buff::create_response_signature_buffer;

use identity::IdentityClient;

use crypto::dh::DhStaticPrivateKey;

#[derive(Debug)]
struct RelayClientError;

trait RelayClient {
    fn connect(
        &mut self,
        relay: RelayAddress,
        port: DhPublicKey,
    ) -> BoxFuture<'_, Result<ConnPairVec, RelayClientError>>;

    fn listen(
        &mut self,
        relay: RelayAddress,
        port_private: DhStaticPrivateKey,
    ) -> BoxStream<'_, ConnPairVec>;
}

pub async fn create_response_send_funds<'a>(
    currency: &Currency,
    pending_transaction: &'a PendingTransaction,
    src_plain_lock: PlainLock,
    serial_num: u128,
    identity_client: &'a mut IdentityClient,
) -> ResponseSendFundsOp {
    let u_response_send_funds = UnsignedResponseSendFundsOp {
        request_id: pending_transaction.request_id.clone(),
        src_plain_lock: src_plain_lock.clone(),
        serial_num,
    };

    let signature_buff = create_response_signature_buffer(
        currency,
        u_response_send_funds.clone(),
        pending_transaction,
    );
    let signature = identity_client
        .request_signature(signature_buff)
        .await
        .unwrap();

    ResponseSendFundsOp {
        request_id: u_response_send_funds.request_id,
        src_plain_lock,
        serial_num,
        signature,
    }
}

pub fn create_cancel_send_funds(request_id: Uid) -> CancelSendFundsOp {
    CancelSendFundsOp { request_id }
}

// TODO:
// 1. Maybe take RequestSendFundOp by value, delegate cloning to external code.
// 2. Maybe PendingTransaction is just an alias for RequestSendFunds? Why do we have two
//    structs?
pub fn create_pending_transaction(request_send_funds: &RequestSendFundsOp) -> PendingTransaction {
    PendingTransaction {
        request_id: request_send_funds.request_id.clone(),
        currency: request_send_funds.currency.clone(),
        src_hashed_lock: request_send_funds.src_hashed_lock.clone(),
        route: request_send_funds.route.clone(),
        dest_payment: request_send_funds.dest_payment,
        invoice_hash: request_send_funds.invoice_hash.clone(),
        left_fees: request_send_funds.left_fees,
    }
}

/*
#[derive(Arbitrary, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum UnsignedFriendTcOp {
    EnableRequests,
    DisableRequests,
    SetRemoteMaxDebt(u128),
    RequestSendFunds(RequestSendFundsOp),
    ResponseSendFunds(ResponseSendFundsOp),
    UnsignedResponseSendFunds(UnsignedResponseSendFundsOp),
    CancelSendFunds(CancelSendFundsOp),
    CollectSendFunds(CollectSendFundsOp),
}
*/

#[derive(Arbitrary, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveTokenHashed {
    /// Hash of operations and local_relays
    #[serde(with = "ser_b64")]
    pub old_token: Signature,
    pub token_info: TokenInfo,
    #[serde(with = "ser_b64")]
    pub new_token: Signature,
}

/*
// TODO: Restore later:
pub fn create_unsigned_move_token<B>(
    currencies_operations: Vec<CurrencyOperations>,
    relays_diff: Vec<RelayAddress<B>>,
    currencies_diff: Vec<Currency>,
    token_info: &TokenInfo,
    old_token: Signature,
) -> UnsignedMoveToken<B> {
    UnsignedMoveToken {
        old_token,
        currencies_operations,
        relays_diff,
        currencies_diff,
        info_hash: hash_token_info(token_info),
    }
}
*/

/*
pub async fn create_move_token<A>(operations: Vec<FriendTcOp>,
                 opt_local_relays: Option<A>,
                 old_token: Signature,
                 inconsistency_counter: u64,
                 move_token_counter: u128,
                 balance: i128,
                 local_pending_debt: u128,
                 remote_pending_debt: u128,
                 rand_nonce: RandValue,
                 identity_client: IdentityClient) -> MoveToken<A>
where
    A: CanonicalSerialize,
{

    let mut move_token = MoveToken {
        operations,
        opt_local_relays,
        old_token,
        inconsistency_counter,
        move_token_counter,
        balance,
        local_pending_debt,
        remote_pending_debt,
        rand_nonce,
        new_token: Signature::zero(),
    };

    let sig_buffer = move_token_signature_buff(&move_token);
    move_token.new_token = identity_client.request_signature(sig_buffer).await.unwrap();
    move_token
}
*/

/// Create a hashed version of the MoveToken.
/// Hashed version contains the hash of the operations instead of the operations themselves,
/// hence it is usually shorter.
pub fn create_hashed(move_token: &MoveToken, token_info: &TokenInfo) -> MoveTokenHashed {
    MoveTokenHashed {
        old_token: move_token.old_token.clone(),
        token_info: token_info.clone(),
        new_token: move_token.new_token.clone(),
    }
}

#[derive(Debug, Clone)]
pub enum IncomingLivenessMessage {
    Online(PublicKey),
    Offline(PublicKey),
}

pub struct FriendInconsistencyError {
    pub reset_token: Signature,
    pub balance_for_reset: i128,
}

#[derive(Debug)]
pub enum ChannelerConfig<RA> {
    /// Set relay address for local node
    /// This is the address the Channeler will connect to
    /// and listen for new connections
    SetRelays(Vec<RA>),
    UpdateFriend(ChannelerUpdateFriend<RA>),
    RemoveFriend(PublicKey),
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum FunderIncomingComm {
    Liveness(IncomingLivenessMessage),
    Friend((PublicKey, FriendMessage)),
}

/// An incoming message to the Funder:
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum FunderIncoming<B> {
    Init,
    Control(FunderIncomingControl<B>),
    Comm(FunderIncomingComm),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum FunderOutgoing<B>
where
    B: Clone,
{
    Control(FunderOutgoingControl),
    Comm(FunderOutgoingComm<B>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum FunderOutgoingComm<B> {
    FriendMessage((PublicKey, FriendMessage)),
    ChannelerConfig(ChannelerConfig<RelayAddress<B>>),
}
