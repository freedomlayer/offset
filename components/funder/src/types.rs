use crypto::identity::{PublicKey, Signature};
use crypto::crypto_rand::RandValue;
use crypto::hash::HashResult;

use common::canonical_serialize::CanonicalSerialize;

use proto::funder::messages::{RequestSendFunds, MoveToken, FriendMessage,
    FriendTcOp, PendingRequest, FunderIncomingControl, 
    FunderOutgoingControl};

use proto::funder::signature_buff::{operations_hash, local_address_hash,
    friend_move_token_signature_buff};

use identity::IdentityClient;



/// Keep information from a RequestSendFunds message.
/// This information will be used later to deal with a corresponding {Response,Failure}SendFunds messages,
/// as those messages do not repeat the information sent in the request.
pub fn create_pending_request(request_send_funds: &RequestSendFunds) -> PendingRequest {
    PendingRequest {
        request_id: request_send_funds.request_id,
        route: request_send_funds.route.clone(),
        dest_payment: request_send_funds.dest_payment,
        invoice_id: request_send_funds.invoice_id.clone(),
    }
}


#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveTokenHashed {
    pub operations_hash: HashResult,
    pub local_address_hash: HashResult,
    pub old_token: Signature,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: Signature,
}


pub async fn create_friend_move_token<A>(operations: Vec<FriendTcOp>,
                 opt_local_address: Option<A>,
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

    let mut friend_move_token = MoveToken {
        operations,
        opt_local_address,
        old_token,
        inconsistency_counter,
        move_token_counter,
        balance,
        local_pending_debt,
        remote_pending_debt,
        rand_nonce,
        new_token: Signature::zero(),
    };

    let sig_buffer = friend_move_token_signature_buff(&friend_move_token);
    friend_move_token.new_token = await!(identity_client.request_signature(sig_buffer)).unwrap();
    friend_move_token
}


/// Create a hashed version of the MoveToken.
/// Hashed version contains the hash of the operations instead of the operations themselves,
/// hence it is usually shorter.
pub fn create_hashed<A>(friend_move_token: &MoveToken<A>) -> MoveTokenHashed {
    MoveTokenHashed {
        operations_hash: operations_hash(friend_move_token),
        local_address_hash: local_address_hash(friend_move_token),
        old_token: friend_move_token.old_token.clone(),
        inconsistency_counter: friend_move_token.inconsistency_counter,
        move_token_counter: friend_move_token.move_token_counter,
        balance: friend_move_token.balance,
        local_pending_debt: friend_move_token.local_pending_debt,
        remote_pending_debt: friend_move_token.remote_pending_debt,
        rand_nonce: friend_move_token.rand_nonce.clone(),
        new_token: friend_move_token.new_token.clone(),
    }
}




#[derive(Debug, Clone)]
pub enum IncomingLivenessMessage {
    Online(PublicKey),
    Offline(PublicKey),
}


#[allow(unused)]
pub struct FriendInconsistencyError {
    pub reset_token: Signature,
    pub balance_for_reset: i128,
}



#[derive(Debug)]
pub enum ChannelerConfig<A> {
    /// Set relay address for local node
    /// This is the address the Channeler will connect to 
    /// and listen for new connections
    SetAddress(Option<A>),
    AddFriend((PublicKey, A)),
    RemoveFriend(PublicKey),
}

#[derive(Debug, Clone)]
pub enum FunderIncomingComm<A> {
    Liveness(IncomingLivenessMessage),
    Friend((PublicKey, FriendMessage<A>)),
}

/// An incoming message to the Funder:
#[derive(Clone, Debug)]
pub enum FunderIncoming<A> {
    Init,
    Control(FunderIncomingControl<A>),
    Comm(FunderIncomingComm<A>),
}

#[allow(unused)]
#[derive(Debug)]
pub enum FunderOutgoing<A: Clone> {
    Control(FunderOutgoingControl<A>),
    Comm(FunderOutgoingComm<A>),
}

#[derive(Debug)]
pub enum FunderOutgoingComm<A> {
    FriendMessage((PublicKey, FriendMessage<A>)),
    ChannelerConfig(ChannelerConfig<A>),
}
