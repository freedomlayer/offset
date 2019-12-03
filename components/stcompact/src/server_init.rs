use futures::SinkExt;

use database::DatabaseClient;

use crate::persist::{CompactState};
use crate::types::{ConnPairCompact, CompactServerError, GenId};
use crate::messages::{PaymentDone, ToUser, PaymentCommit, OpenPaymentStatus};

#[allow(unused)]
pub async fn server_init<GI>(
    conn_pair_compact: &mut ConnPairCompact,
    compact_state: &mut CompactState,
    database_client: &mut DatabaseClient<CompactState>,
    gen_id: &mut GI) -> Result<(), CompactServerError>
where   
    GI: GenId,
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
                let ack_uid = gen_id.gen_uid();
                open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                database_client.mutate(vec![compact_state.clone()])
                    .await
                    .map_err(|_| CompactServerError::DatabaseMutateError)?;

                // Send failure message to user:
                let payment_done = PaymentDone::Failure(ack_uid);
                conn_pair_compact.sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
            OpenPaymentStatus::Commit(commit, _fees) => {
                // At this point we can not cancel the payment, because it is possible
                // that the user has already handed over the commit to the seller.
                
                // Resend commit to user:
                let payment_commit = PaymentCommit {
                    payment_id: payment_id.clone(),
                    commit: commit.clone().into(),
                };
                conn_pair_compact.sender.send(ToUser::PaymentCommit(payment_commit)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
            OpenPaymentStatus::Success(receipt, fees, ack_uid) => {
                // Resend success to user:
                let payment_done = PaymentDone::Success(receipt.clone(), *fees, ack_uid.clone());
                conn_pair_compact.sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
            OpenPaymentStatus::Failure(ack_uid) => {
                // Resend failure to user:
                let payment_done = PaymentDone::Failure(ack_uid.clone());
                conn_pair_compact.sender.send(ToUser::PaymentDone(payment_done)).await.map_err(|_| CompactServerError::UserSenderError)?;
            },
        }
    }
    Ok(())
}
