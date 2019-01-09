use crypto::crypto_rand::RandValue;
use crypto::hash::HashResult;

use common::canonical_serialize::CanonicalSerialize;

use proto::funder::messages::{RequestSendFunds, ResponseSendFunds, FailureSendFunds, 
    MoveToken, FriendMessage, FriendTcOp, PendingRequest, FunderIncomingControl, 
    FunderOutgoingControl, TPublicKey, TSignature};

use proto::funder::signature_buff::{prefix_hash,
    move_token_signature_buff, create_response_signature_buffer,
    create_failure_signature_buffer};

use identity::IdentityClient;



/*
pub type UnsignedFailureSendFunds = FailureSendFunds<()>;
pub type UnsignedResponseSendFunds = ResponseSendFunds<()>;
pub type UnsignedMoveToken<A> = MoveToken<A,()>;
*/

/*

pub async fn sign_move_token<'a,A>(unsigned_move_token: UnsignedMoveToken<A>,
                         identity_client: &'a mut IdentityClient) -> MoveToken<A> 
where
    A: CanonicalSerialize + 'a,
{

    let signature_buff = move_token_signature_buff(&unsigned_move_token);
    let new_token = await!(identity_client.request_signature(signature_buff))
            .unwrap();

    MoveToken {
        operations: unsigned_move_token.operations,
        opt_local_address: unsigned_move_token.opt_local_address,
        old_token: unsigned_move_token.old_token,
        inconsistency_counter: unsigned_move_token.inconsistency_counter,
        move_token_counter: unsigned_move_token.move_token_counter,
        balance: unsigned_move_token.balance,
        local_pending_debt: unsigned_move_token.local_pending_debt, 
        remote_pending_debt: unsigned_move_token.local_pending_debt,
        rand_nonce: unsigned_move_token.rand_nonce, 
        new_token,
    }
}
*/


pub async fn create_response_send_funds<'a>(pending_request: &'a PendingRequest,
                                            rand_nonce: RandValue,
                                            identity_client: &'a mut IdentityClient) -> ResponseSendFunds 
{

    let u_response_send_funds = ResponseSendFunds {
        request_id: pending_request.request_id,
        rand_nonce: rand_nonce,
        signature: (),
    };

    let signature_buff = create_response_signature_buffer(&u_response_send_funds, pending_request);
    let signature = await!(identity_client.request_signature(signature_buff))
        .unwrap();

    ResponseSendFunds {
        request_id: u_response_send_funds.request_id,
        rand_nonce: u_response_send_funds.rand_nonce,
        signature,
    }

}

pub async fn create_failure_send_funds<'a,P>(pending_request: &'a PendingRequest<P>,
                                         local_public_key: &'a TPublicKey<P>,
                                         rand_nonce: RandValue,
                                         identity_client: &'a mut IdentityClient) -> FailureSendFunds<P,RS> {

    let u_failure_send_funds = FailureSendFunds {
        request_id: pending_request.request_id,
        reporting_public_key: local_public_key.clone(),
        rand_nonce: rand_nonce,
        signature: (),
    };

    let signature_buff = create_failure_signature_buffer(&u_failure_send_funds, pending_request);
    let signature = await!(identity_client.request_signature(signature_buff))
        .unwrap();

    FailureSendFunds {
        request_id: u_failure_send_funds.request_id,
        reporting_public_key: u_failure_send_funds.reporting_public_key,
        rand_nonce: u_failure_send_funds.rand_nonce,
        signature,
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum UnsignedFriendTcOp {
    EnableRequests,
    DisableRequests,
    SetRemoteMaxDebt(u128),
    RequestSendFunds(RequestSendFunds),
    ResponseSendFunds(ResponseSendFunds),
    UnsignedResponseSendFunds(UnsignedResponseSendFunds),
    FailureSendFunds(FailureSendFunds),
    UnsignedFailureSendFunds(UnsignedFailureSendFunds),
}



/// Keep information from a RequestSendFunds message.
/// This information will be used later to deal with a corresponding {Response,Failure}SendFunds messages,
/// as those messages do not repeat the information sent in the request.
pub fn create_pending_request<P>(request_send_funds: &RequestSendFunds<P>) -> PendingRequest<P> 
where
    P: Clone,
{
    PendingRequest {
        request_id: request_send_funds.request_id,
        route: request_send_funds.route.clone(),
        dest_payment: request_send_funds.dest_payment,
        invoice_id: request_send_funds.invoice_id.clone(),
    }
}


#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct MoveTokenHashed<P,MS> {
    /// Hash of operations and local_address
    pub prefix_hash: HashResult,
    pub inconsistency_counter: u64,
    pub move_token_counter: u128,
    pub balance: i128,
    pub local_pending_debt: u128,
    pub remote_pending_debt: u128,
    pub rand_nonce: RandValue,
    pub new_token: TSignature<MS>,
}

/*
 * TODO: Find a safe way to implement this:

pub fn create_unsigned_move_token<A,P,RS>(operations: Vec<FriendTcOp<P,RS>>,
                 opt_local_address: Option<A>,
                 old_token: TSignature<S>,
                 inconsistency_counter: u64,
                 move_token_counter: u128,
                 balance: i128,
                 local_pending_debt: u128,
                 remote_pending_debt: u128,
                 rand_nonce: RandValue) -> UnsignedMoveToken<A> {

    MoveToken {
        operations,
        opt_local_address,
        old_token,
        inconsistency_counter,
        move_token_counter,
        balance,
        local_pending_debt,
        remote_pending_debt,
        rand_nonce,
        new_token: (),
    }
}
*/

/*
pub async fn create_move_token<A>(operations: Vec<FriendTcOp>,
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

    let mut move_token = MoveToken {
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

    let sig_buffer = move_token_signature_buff(&move_token);
    move_token.new_token = await!(identity_client.request_signature(sig_buffer)).unwrap();
    move_token
}
*/



/// Create a hashed version of the MoveToken.
/// Hashed version contains the hash of the operations instead of the operations themselves,
/// hence it is usually shorter.
pub fn create_hashed<A>(move_token: &MoveToken<A>) -> MoveTokenHashed
where
    A: CanonicalSerialize,
{
    MoveTokenHashed {
        prefix_hash: prefix_hash(move_token),
        inconsistency_counter: move_token.inconsistency_counter,
        move_token_counter: move_token.move_token_counter,
        balance: move_token.balance,
        local_pending_debt: move_token.local_pending_debt,
        remote_pending_debt: move_token.remote_pending_debt,
        rand_nonce: move_token.rand_nonce.clone(),
        new_token: move_token.new_token.clone(),
    }
}




#[derive(Debug, Clone)]
pub enum IncomingLivenessMessage<P> {
    Online(TPublicKey<P>),
    Offline(TPublicKey<P>),
}


#[allow(unused)]
pub struct FriendInconsistencyError<P,S> {
    pub reset_token: TSignature<S>,
    pub balance_for_reset: i128,
}

#[derive(Debug)]
pub struct ChannelerAddFriend<A,P> {
    pub friend_public_key: TPublicKey<P>,
    pub friend_address: A,
    pub local_addresses: Vec<A>,
}

#[derive(Debug)]
pub enum ChannelerConfig<A,P> {
    /// Set relay address for local node
    /// This is the address the Channeler will connect to 
    /// and listen for new connections
    SetAddress(Option<A>),
    AddFriend(ChannelerAddFriend<A,P>),
    RemoveFriend(TPublicKey<P>),
}

#[derive(Debug, Clone)]
pub enum FunderIncomingComm<A,P,RS,FS,MS> {
    Liveness(IncomingLivenessMessage<P>),
    Friend((TPublicKey<P>, FriendMessage<A,P,RS,FS,MS>)),
}

/// An incoming message to the Funder:
#[derive(Clone, Debug)]
pub enum FunderIncoming<A,P,RS,FS,MS> {
    Init,
    Control(FunderIncomingControl<A,P,RS,MS>),
    Comm(FunderIncomingComm<A,P,RS,FS,MS>),
}

#[allow(unused)]
#[derive(Debug)]
pub enum FunderOutgoing<A: Clone,P,RS,FS,MS> {
    Control(FunderOutgoingControl<A,P,RS,MS>),
    Comm(FunderOutgoingComm<A,P,RS,FS,MS>),
}

#[derive(Debug)]
pub enum FunderOutgoingComm<A,P,RS,FS,MS> {
    FriendMessage((TPublicKey<P>, FriendMessage<A,P,RS,FS,MS>)),
    ChannelerConfig(ChannelerConfig<A,P>),
}
