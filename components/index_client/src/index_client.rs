use common::safe_arithmetic::SafeSignedArithmetic;

use crypto::identity::PublicKey;
use proto::index_client::messages::{AppServerToIndexClient, IndexClientToAppServer,
                                    IndexClientMutation, IndexClientState};
use proto::funder::report::{FunderReportMutation, FunderReport, 
    FriendStatusReport, FriendReport, FriendLivenessReport};


use futures::{Stream, Sink, SinkExt};


pub async fn index_client_loop<ISA,A,FAS,TAS>(from_app_server: FAS,
                               to_app_server: TAS) 
where
    FAS: Stream<Item=AppServerToIndexClient<ISA>>,
    TAS: Sink<SinkItem=IndexClientToAppServer<ISA>>,
{
    unimplemented!();

}
