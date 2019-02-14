use common::conn::{ConnPair, ConnPairVec, FutTransform};
use crypto::identity::PublicKey;
use identity::IdentityClient;

use proto::app_server::messages::{AppToAppServer, AppServerToApp};

use net::NetAddress;


pub fn identity_client_from_idfile(idfile_path: Path) -> IdentityClient {
    unimplemented!();
}

type NodeConnection = (AppPermissions, ConnPair<AppToAppServer,AppServerToApp>);

async fn connect_to_node(net_connector: C,
                   node_public_key: PublicKey,
                   app_identity_client: IdentityClient) -> NodeConnection 
where   
    C: FutTransform<Input=NetAddress, Output=Option<ConnPairVec>>
    
{

    // TODO:
    // - Create a timer service
    // - Create secure rng
    // - Create a TcpConnector
    // - Connect
    // - Apply the following over the connection:
    //   - Version prefix
    //   - Encryption (Expect node public key)
    //   - Keepalive
    //   - Obtain permissions
    //   - serialization



    unimplemented!();
}
