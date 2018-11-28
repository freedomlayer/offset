
#[derive(Debug, PartialEq, Eq)]
pub enum KaMessage {
    KeepAlive,
    Message(Vec<u8>),
}
