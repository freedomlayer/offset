#[derive(Debug, PartialEq, Eq, Clone)]
pub enum KaMessage {
    KeepAlive,
    Message(Vec<u8>),
}
