use futures::{Stream, Sink};
use proto::relay::messages::{InitConnection, TunnelMessage};

/// Run the keepalive maintenance, exposing to the user the ability to send and receive Vec<u8>
/// frames.
pub async fn client_tunnel<TTS,TTSE,FTR,UFTS,UFTSE,UTTR>(to_tunnel_sender: TTS, from_tunnel_receiver: FTR, 
                           user_from_tunnel_sender: UFTS, user_to_tunnel_receiver: UTTR,
                           keepalive_ticks: usize) -> Result<(),()> 
where
    TTS: Sink<SinkItem=TunnelMessage, SinkError=TTSE>,
    FTR: Stream<Item=TunnelMessage>,
    UFTS: Sink<SinkItem=Vec<u8>, SinkError=UFTSE>,
    UTTR: Stream<Item=Vec<u8>>,
{
    // TODO: Take keepalive related argument somehow.
    // TODO:
    // - Forwared TunnelMessages.
    // - Handle keepalives:
    //      - Send a keepalive periodically.
    //      - Disconnect if keepalives don't show up on time.
    Ok(())
}

