use futures::SinkExt;

use app::conn::{buyer, AppConnTuple, AppToAppServer};
use database::DatabaseClient;

use crate::compact_node::messages::{
    CompactToUser, CompactToUserAck, PaymentCommit, PaymentDone, PaymentDoneStatus,
};
use crate::compact_node::persist::{CompactState, OpenPaymentStatus};
use crate::compact_node::types::{CompactNodeError, ConnPairCompact};
use crate::gen::GenUid;

/// Assume that the server was abruptly closed, and fix any possible issues by:
/// - Resend relevant communication
/// - Cancel requests that were in very early stage.
pub async fn compact_node_init<CG>(
    app_conn_tuple: &mut AppConnTuple,
    conn_pair_compact: &mut ConnPairCompact,
    compact_state: &mut CompactState,
    database_client: &mut DatabaseClient<CompactState>,
    compact_gen: &mut CG,
) -> Result<(), CompactNodeError>
where
    CG: GenUid,
{
    let (_, _, conn_pair_app) = app_conn_tuple;
    let payment_ids: Vec<_> = compact_state.open_payments.keys().cloned().collect();
    for payment_id in &payment_ids {
        let open_payment = compact_state.open_payments.get_mut(payment_id).unwrap();
        match &open_payment.status {
            OpenPaymentStatus::SearchingRoute(_) | OpenPaymentStatus::FoundRoute(_) => {
                // We can still cancel the payment. Let's cancel it.

                // Set payment as failure:
                let ack_uid = compact_gen.gen_uid();
                open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                database_client
                    .mutate(vec![compact_state.clone()])
                    .await
                    .map_err(|_| CompactNodeError::DatabaseMutateError)?;

                // Send failure message to user:
                let payment_done = PaymentDone {
                    payment_id: payment_id.clone(),
                    status: PaymentDoneStatus::Failure(ack_uid),
                };
                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact
                    .sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }
            OpenPaymentStatus::Sending(_) => {
                // We can still cancel the payment. Let's cancel it.

                // Note: We could actually try to resume the payment in this case,
                // (Though we will have to store more information about the sent transactions).
                // We decided not to do that at this point.

                // Resend RequestClosePayment, in case it was not sent yet, or in case we have
                // missed an incoming `ResponseClosePayment` message:
                let app_request = buyer::request_close_payment(payment_id.clone());
                let app_to_app_server = AppToAppServer {
                    app_request_id: compact_gen.gen_uid(),
                    app_request,
                };
                conn_pair_app
                    .sender
                    .send(app_to_app_server)
                    .await
                    .map_err(|_| CompactNodeError::AppSenderError)?;

                // Set payment as failure:
                let ack_uid = compact_gen.gen_uid();
                open_payment.status = OpenPaymentStatus::Failure(ack_uid.clone());
                database_client
                    .mutate(vec![compact_state.clone()])
                    .await
                    .map_err(|_| CompactNodeError::DatabaseMutateError)?;

                // Send failure message to user:
                let payment_done = PaymentDone {
                    payment_id: payment_id.clone(),
                    status: PaymentDoneStatus::Failure(ack_uid),
                };
                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact
                    .sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }
            OpenPaymentStatus::Commit(commit, _fees) => {
                // At this point we can not cancel the payment, because it is possible
                // that the user has already handed over the commit to the seller.

                // Resend RequestClosePayment, in case it was not sent yet, or in case we have
                // missed an incoming `ResponseClosePayment` message:
                let app_request = buyer::request_close_payment(payment_id.clone());
                let app_to_app_server = AppToAppServer {
                    app_request_id: compact_gen.gen_uid(),
                    app_request,
                };
                conn_pair_app
                    .sender
                    .send(app_to_app_server)
                    .await
                    .map_err(|_| CompactNodeError::AppSenderError)?;

                // Resend commit to user:
                let payment_commit = PaymentCommit {
                    payment_id: payment_id.clone(),
                    commit: commit.clone().into(),
                };
                let compact_to_user = CompactToUser::PaymentCommit(payment_commit);
                conn_pair_compact
                    .sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }
            OpenPaymentStatus::Success(receipt, fees, ack_uid) => {
                // Resend success to user:
                let payment_done = PaymentDone {
                    payment_id: payment_id.clone(),
                    status: PaymentDoneStatus::Success(receipt.clone(), *fees, ack_uid.clone()),
                };
                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact
                    .sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }
            OpenPaymentStatus::Failure(ack_uid) => {
                // Resend failure to user:
                let payment_done = PaymentDone {
                    payment_id: payment_id.clone(),
                    status: PaymentDoneStatus::Failure(ack_uid.clone()),
                };

                let compact_to_user = CompactToUser::PaymentDone(payment_done);
                conn_pair_compact
                    .sender
                    .send(CompactToUserAck::CompactToUser(compact_to_user))
                    .await
                    .map_err(|_| CompactNodeError::UserSenderError)?;
            }
        }
    }
    Ok(())
}
