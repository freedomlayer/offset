use common::canonical_serialize::CanonicalSerialize;
use crypto::identity::{PublicKey, Signature, 
    verify_signature};

use proto::funder::messages::{UnsignedMoveToken, SignedMoveToken, 
    SignedResponse, UnsignedResponse, 
    SignedFailure, UnsignedFailure, 
    PendingRequest, TPublicKey,
    SendFundsReceipt};

use proto::funder::signature_buff::{create_response_signature_buffer,
                                    create_failure_signature_buffer,
                                    create_receipt_signature_buffer,
                                    move_token_signature_buff};

use common::conn::BoxFuture;

pub trait Verify {
    fn verify(&self) -> bool;
}

pub trait SignMoveToken<A,P,RS,FS,MS> {
    fn sign(&mut self, u_move_token: UnsignedMoveToken<A,P,RS,FS,MS>) 
        -> BoxFuture<'_, SignedMoveToken<A,P,RS,FS,MS>>;
}

pub trait SignResponse<RS> {
    fn sign(&mut self, u_response: UnsignedResponse) 
        -> BoxFuture<'_, SignedResponse<RS>>;
}

pub trait SignFailure<P,FS> {
    fn sign(&mut self, u_response: UnsignedFailure<P>) 
        -> BoxFuture<'_, SignedFailure<P,FS>>;
}

/// Verify a response signature
impl Verify for (SignedResponse<Signature>, PendingRequest<PublicKey>) {

    fn verify(&self) -> bool {

        let (response_send_funds, pending_request) = self;
        let dest_public_key = match pending_request.route.public_keys
            .last() {
            Some(dest_public_key) => dest_public_key,
            None => return false,
        };

        let response_signature_buffer = create_response_signature_buffer(
                                            &response_send_funds,
                                            &pending_request);
        // Verify response funds signature:
        if !verify_signature(&response_signature_buffer, 
                             &dest_public_key.public_key,
                             &response_send_funds.signature.signature) {
            return false;
        }
        true
    }
}

/// Verify a failure signature
impl Verify for (&SignedFailure<PublicKey, Signature>, &PendingRequest<PublicKey>) {

    fn verify(&self) -> bool {
        let (failure_send_funds, pending_request) = self;

        let failure_signature_buffer = create_failure_signature_buffer(
                                            &failure_send_funds,
                                            &pending_request);
        let reporting_public_key = &failure_send_funds.reporting_public_key;
        // Make sure that the reporting_public_key is on the route:
        // TODO: Should we check that it is after us? Is it checked somewhere else?
        if let None = pending_request.route.pk_to_index(&reporting_public_key) {
            return false;
        }

        if !verify_signature(&failure_signature_buffer, 
                         &reporting_public_key.public_key, 
                         &failure_send_funds.signature.signature) {
            return false;
        }
        true
    }
}

/// Verify a receipt
impl Verify for (SendFundsReceipt<Signature>, TPublicKey<PublicKey>) {
    fn verify(&self) -> bool {
        let (receipt, public_key) = self;
        let data = create_receipt_signature_buffer(receipt);
        verify_signature(&data, &public_key.public_key, &receipt.signature.signature)
    }
}

/// Verify a MoveToken
impl<A> Verify for (SignedMoveToken<A,PublicKey,Signature,Signature,Signature>, TPublicKey<PublicKey>) 
where
    A: CanonicalSerialize,
{
    fn verify(&self) -> bool {
        let (move_token, public_key) = self;
        let sig_buffer = move_token_signature_buff(move_token);
        verify_signature(&sig_buffer, &public_key.public_key, &move_token.new_token.signature)
    }
}

