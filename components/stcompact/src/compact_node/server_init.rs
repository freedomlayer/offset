use futures::SinkExt;

use database::DatabaseClient;

use crate::compact_node::persist::{CompactState, OpenPaymentStatus};
use crate::compact_node::types::{ConnPairCompact, CompactServerError};
use crate::compact_node::messages::{PaymentDone, CompactToUser, CompactToUserAck, PaymentCommit};
use crate::gen::GenUid;

#[allow(unused)]
pub async fn server_init<CG>(
    conn_pair_compact: &mut ConnPairCompact,
    compact_state: &mut CompactState,
    database_client: &mut DatabaseClient<CompactState>,
    compact_gen: &mut CG) -> Result<(), CompactServerError>
where   
    CG: GenUid,
{

    let payment_ids: Vec<_> = compact_state.open_payments.keys().cloned().collect();
    for payment_id in &payment_ids {
        let open_payment = compact_state.open_payments.get_mut(payment_id).unwrap();
        match &open_payment.status {
            OpenPaymentStatus::SearchingRoute(_) 
            | OpenPaymentStatus::FoundRoute(_) 
            | OpenPaymentStatus::Sending(_) => {
                // We can still cancel the payment. Let's cancel it.
                
                // Note: We could actually try to resume the payment in the `::Sending` case,
                // (Though we will have to store more information about the sent transactions).
                // We decided not to do that at this point.

                // Set payment as failure:
                let ack_uid = compact_gen.gen_uid();
                open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                database_client.mutate(vec![compact_state.clone()])
                    .await
                    .map_err(|_| CompactServerError::DatabaseMutateError)?;

                // Send failure message to user:
                let payment_done = PaymentDone::Failure(ack_uid);
                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact.sender.send(CompactToUserAck::CompactToUser(compact_to_user)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
            OpenPaymentStatus::Commit(commit, _fees) => {
                // At this point we can not cancel the payment, because it is possible
                // that the user has already handed over the commit to the seller.
                
                // Resend commit to user:
                let payment_commit = PaymentCommit {
                    payment_id: payment_id.clone(),
                    commit: commit.clone().into(),
                };
                let compact_to_user = CompactToUser::PaymentCommit(payment_commit);
                conn_pair_compact.sender.send(CompactToUserAck::CompactToUser(compact_to_user)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
            OpenPaymentStatus::Success(receipt, fees, ack_uid) => {
                // Resend success to user:
                let payment_done = PaymentDone::Success(receipt.clone(), *fees, ack_uid.clone());
                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact.sender.send(CompactToUserAck::CompactToUser(compact_to_user)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
            OpenPaymentStatus::Failure(ack_uid) => {
                // Resend failure to user:
                let payment_done = PaymentDone::Failure(ack_uid.clone());
                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact.sender.send(CompactToUserAck::CompactToUser(compact_to_user)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
        }
    }
    Ok(())
}
