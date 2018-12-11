use futures::{Stream, Sink};

use common::conn::ConnPair;
use crypto::identity::{PublicKey};

use proto::funder::messages::{FunderOutgoingControl, FunderIncomingControl};
use proto::app_server::messages::{AppServerToApp, AppToAppServer};

type IncomingAppConnection<A> = (PublicKey, ConnPair<AppServerToApp<A>, AppToAppServer<A>>);


pub fn app_server_loop<A,FF,TF,IC>(from_funder: FF, to_funder: TF, 
                                incoming_connections: IC)
where
    A: Clone,
    FF: Stream<Item=FunderOutgoingControl<A>>,
    FF: Sink<SinkItem=FunderIncomingControl<A>>,
    IC: Stream<Item=IncomingAppConnection<A>>,
{
    unimplemented!();

}
