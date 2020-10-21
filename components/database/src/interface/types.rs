use futures::channel::oneshot;

#[derive(Debug)]
pub enum DbOpError {
    // SendOpFailed,
// ResponseOpFailed(oneshot::Canceled),
}

pub type DbOpResult<T> = Result<T, DbOpError>;
pub type DbOpSenderResult<T> = oneshot::Sender<DbOpResult<T>>;
