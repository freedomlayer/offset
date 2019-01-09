use common::canonical_serialize::CanonicalSerialize;
use common::conn::BoxFuture;

use crypto::identity::{PublicKey, Signature, SIGNATURE_LEN, verify_signature};

use crypto::crypto_rand::{RandValue, CryptoRandom};

use proto::funder::messages::{MoveToken, 
    UnsignedMoveToken, SignedMoveToken, 
    ResponseSendFunds, SignedResponse, UnsignedResponse, 
    FailureSendFunds, SignedFailure, UnsignedFailure, 
    PendingRequest, TPublicKey, TSignature,
    SendFundsReceipt};

use proto::funder::signature_buff::{create_response_signature_buffer,
                                    create_failure_signature_buffer,
                                    create_receipt_signature_buffer,
                                    move_token_signature_buff};

use identity::IdentityClient;


pub trait Verify {
    fn verify(&self) -> bool;
}

pub trait SignMoveToken<A,P,RS,FS,MS> {
    fn sign_move_token(&mut self, u_move_token: UnsignedMoveToken<A,P,RS,FS,MS>) 
        -> BoxFuture<'_, Option<SignedMoveToken<A,P,RS,FS,MS>>>;
}

pub trait SignResponse<P,RS> {
    fn sign_response(&mut self, u_response: UnsignedResponse, 
                     pending_request: &PendingRequest<P>) 
        -> BoxFuture<'_, Option<SignedResponse<RS>>>;
}

pub trait SignFailure<P,FS> {
    fn sign_failure(&mut self, u_failure: UnsignedFailure<P>,
                    pending_request: &PendingRequest<P>) 
        -> BoxFuture<'_, Option<SignedFailure<P,FS>>>;
}


pub trait GenRandToken<MS> {
    fn gen_rand_token(&mut self) -> TSignature<MS>;
}

pub trait GenRandNonce {
    fn gen_rand_nonce(&mut self) -> RandValue;
}


// -------------------------------------------------
// -------------------------------------------------

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
        // TODO: Maybe check this somewhere else, outside of sign_verify.rs?
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


impl<A> SignMoveToken<A,PublicKey,Signature,Signature,Signature> for IdentityClient 
where
    A: CanonicalSerialize + Send,
{
    fn sign_move_token(&mut self, 
                       u_move_token: UnsignedMoveToken<A,PublicKey,Signature,Signature,Signature>) 
        -> BoxFuture<'_, Option<SignedMoveToken<A,PublicKey,Signature,Signature,Signature>>> {

        Box::pin(async move {
            let sign_buff = move_token_signature_buff(&u_move_token);
            let new_token = TSignature::new(await!(self.request_signature(sign_buff)).ok()?);

            Some(MoveToken {
                operations: u_move_token.operations,
                opt_local_address: u_move_token.opt_local_address,
                old_token: u_move_token.old_token, 
                local_public_key: u_move_token.local_public_key,
                remote_public_key: u_move_token.remote_public_key,
                inconsistency_counter: u_move_token.inconsistency_counter,
                move_token_counter: u_move_token.move_token_counter,
                balance: u_move_token.balance,
                local_pending_debt: u_move_token.local_pending_debt,
                remote_pending_debt: u_move_token.remote_pending_debt,
                rand_nonce: u_move_token.rand_nonce, 
                new_token,
            })
        })
    }
}

impl SignResponse<PublicKey, Signature> for IdentityClient {
    fn sign_response(&mut self, u_response: UnsignedResponse,
                     pending_request: &PendingRequest<P>) 
        -> BoxFuture<'_, Option<SignedResponse<Signature>>> {

        Box::pin(async move {
            let sign_buff = create_response_signature_buffer(&u_response, 
                                                             pending_request);
            let signature = TSignature::new(await!(self.request_signature(sign_buff)).ok()?);

            Some(ResponseSendFunds {
                request_id: u_response.request_id,
                rand_nonce: u_response.rand_nonce,
                signature,
            })
        })
    }
}

impl SignFailure<PublicKey,Signature> for IdentityClient {
    fn sign_failure(&mut self, u_failure: UnsignedFailure<PublicKey>,
                    pending_request: &PendingRequest<PublicKey>) 
        -> BoxFuture<'_, Option<SignedFailure<PublicKey,Signature>>> {

        Box::pin(async move {
            let sign_buff = create_failure_signature_buffer(&u_failure,
                                                            pending_request);
            let signature = TSignature::new(await!(self.request_signature(sign_buff)).ok()?);

            Some(FailureSendFunds {
                request_id: u_failure.request_id,
                reporting_public_key: u_failure.reporting_public_key,
                rand_nonce: u_failure.rand_nonce,
                signature,
            })
        })
    }
}


impl<Signature,R> GenRandToken<Signature> for R 
where
    R: CryptoRandom,
{
    fn gen_rand_token(&mut self) -> TSignature<Signature> {
        let mut buff = [0; SIGNATURE_LEN];
        self.fill(&mut buff).unwrap();
        TSignature::new(Signature::from(buff))
    }
}

impl<R> GenRandNonce for R 
where
    R: CryptoRandom,
{
    fn gen_rand_nonce(&mut self) -> RandNonce {
        RandValue::new(&*self)
    }
}
