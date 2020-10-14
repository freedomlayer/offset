use futures::channel::oneshot;

#[derive(Debug)]
pub enum OpError {
    SendOpFailed,
    ResponseOpFailed(oneshot::Canceled),
}

/*
macro_rules! ops_enum_func {
    ($variant_snake:ident($($arg_name: $arg_type),*) -> $ret_type) => {
        paste! {
            async fn $variant_snake(&mut self, $($arg_name: $arg_type),*) -> Result<$ret_type, OpError> {
                let (op_sender, op_receiver) = oneshot::channel();
                let op = $op_enum::[<$variant_snake:camel>]($($arg_name),*, op_sender);
                self.sender
                    .send(op)
                    .await
                    .map_err(|_| OpError::SendOpFailed)?;
                op_receiver.await.map_err(OpError::ResponseOpFailed)?
            }
        }
    };

    ($variant_snake:ident($($arg_name: $arg_type),*)) => {
        paste! {
            async fn $variant_snake(&mut self, $($arg_name: $arg_type),*) {
                let (op_sender, op_receiver) = oneshot::channel();
                let op = $op_enum::[<$variant_snake:camel>]($($arg_name),*, op_sender);
                self.sender
                    .send(op)
                    .await
                    .map_err(|_| OpError::SendOpFailed)?;
                op_receiver.await.map_err(OpError::ResponseOpFailed)?
            }
        }
    };
}
*/

// A helper macro, allowing to have functions with unit "()" return value.
#[doc(hidden)]
#[macro_export]
macro_rules! get_out_type {
    ($ret_type:path) => {
        $ret_type
    };
    () => {
        ()
    };
}

#[macro_export]
/// Create an async interface for request/response messages
/// Generates an enum of all possible RPC messages, and an async interface that knows how to send
/// those RPC messages.
macro_rules! ops_enum {
    (($op_enum:ident $(<$($ty:ident),*>)?, $transaction:ident) => {
        $(
            $variant_snake:ident ($($($arg_name:ident: $arg_type:path),+)?) $(-> $ret_type:path)?
        );*
        // Possibly an extra semicolon:
        $(;)?
    }) => {
        paste! {
            // Enum for all possible operations
            pub enum $op_enum$(<$($ty),*>)? {
                $(
                    [<$variant_snake:camel>]($($($arg_type),+ ,)? oneshot::Sender<Result< get_out_type!($($ret_type)?) , OpError>>)
                ),*
            }
        }

        // A transaction client
        pub struct $transaction$(<$($ty),*>)? {
            sender: mpsc::Sender<$op_enum$(<$($ty),*>)?>,
        }

        impl$(<$($ty),*>)? $transaction$(<$($ty),*>)? {
            pub fn new(sender: mpsc::Sender<$op_enum$(<$($ty),*>)?>) -> Self {
                Self { sender }
            }
            $(
                paste! {
                    async fn $variant_snake(&mut self $(, $($arg_name: $arg_type),+)?) -> Result< get_out_type!($($ret_type)?) , OpError> {
                        let (op_sender, op_receiver) = oneshot::channel();
                        let op = $op_enum::[<$variant_snake:camel>]($($($arg_name),+,)? op_sender);
                        self.sender
                            .send(op)
                            .await
                            .map_err(|_| OpError::SendOpFailed)?;
                        op_receiver.await.map_err(OpError::ResponseOpFailed)?
                    }
                }
            )*
        }
    };

}

#[allow(dead_code)]
#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::mpsc;
    use futures::SinkExt;
    use paste::paste;

    #[test]
    fn test_rpc_enums() {
        ops_enum!((TcOp1<B>, TcTransaction1) => {
            func1(hello: Option<B>) -> u32;
            func2() -> u8;
            func3();
            func4(world: u64) -> u32;
            func5(world: u64);
        });

        ops_enum!((TcOp2, TcTransaction2) => {
            func1(hello: String) -> Result<u32, u64>;
            func2(world: String) -> u32
        });
    }
}
