use futures::channel::{mpsc, oneshot};
use futures::SinkExt;

#[derive(Debug)]
pub enum OpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

macro_rules! ops_enum {
    // Enum with type arguments:
    ({$op_enum:ident, $transaction:ident},
    $(
        $variant_snake:ident, $variant_camel:ident($($arg_name:ident: $arg_type:path),*) -> $ret_type:ident
    ),*

    ) => {
        // Enum for all possible operations
        pub enum $op_enum {
            $(
                $variant_camel($($arg_type),* , oneshot::Sender<Result<$ret_type, OpError>>)
            )*
        }


        // A transaction client
        pub struct $transaction {
            sender: mpsc::Sender<$op_enum>,
        }

        impl $transaction {
            $(
                async fn $variant_snake(&mut self, $($arg_name: $arg_type),*) -> Result<$ret_type, OpError> {
                    let (op_sender, op_receiver) = oneshot::channel();
                    let op = $op_enum::$variant_camel($($arg_name),*, op_sender);
                    self.sender
                        .send(op)
                        .await
                        .map_err(|_| OpError::SendOpFailed)?;
                    op_receiver.await.map_err(OpError::ResponseOpFailed)?
                }
            )*
        }
    };

    // Enum with type arguments:
    ({$op_enum:ident <$($ty:ident),*>, $transaction:ident},
    $(
        $variant_snake:ident, $variant_camel:ident($($arg_name:ident: $arg_type:path),*) -> $ret_type:ident
    ),*

    ) => {
        // Enum for all possible operations
        pub enum $op_enum<$($ty),*> {
            $(
                $variant_camel($($arg_type),* , oneshot::Sender<Result<$ret_type, OpError>>)
            )*
        }


        // A transaction client
        pub struct $transaction<$($ty),*> {
            sender: mpsc::Sender<$op_enum<$($ty),*>>,
        }

        impl<$($ty),*> $transaction<$($ty),*> {
            $(
                async fn $variant_snake(&mut self, $($arg_name: $arg_type),*) -> Result<$ret_type, OpError> {
                    let (op_sender, op_receiver) = oneshot::channel();
                    let op = $op_enum::$variant_camel($($arg_name),*, op_sender);
                    self.sender
                        .send(op)
                        .await
                        .map_err(|_| OpError::SendOpFailed)?;
                    op_receiver.await.map_err(OpError::ResponseOpFailed)?
                }
            )*
        }
    };

}

ops_enum!({TcOp<B>, TcTransaction},
    mc_get_balance, McGetBalance(hello: Option::<B>) -> u32
);

ops_enum!({TcOp2, TcTransaction2},
    mc_get_balance, McGetBalance(hello: String) -> u32
);
