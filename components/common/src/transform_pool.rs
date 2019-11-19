use futures::channel::mpsc;
use futures::stream::select;
use futures::task::{Spawn, SpawnExt};
use futures::{future, stream, Sink, SinkExt, Stream, StreamExt};

use crate::conn::FutTransform;

#[derive(Debug)]
pub enum TransformPoolLoopError {
    SpawnError,
}

enum TransformPoolEvent<I> {
    Incoming(I),
    IncomingClosed,
    TransformDone,
}

/// Transform a stream of incoming items to outgoing items.
/// The transformation is asynchronous, therefore outgoing items
/// might not be in the same order in which the incoming items entered.
///
/// max_concurrent is the maximum amount of concurrent transformations.
pub async fn transform_pool_loop<IN, OUT, I, O, T, S>(
    incoming: I,
    outgoing: O,
    transform: T,
    max_concurrent: usize,
    spawner: S,
) -> Result<(), TransformPoolLoopError>
where
    IN: Send + 'static,
    OUT: Send,
    T: FutTransform<Input = IN, Output = Option<OUT>> + Clone + Send + 'static,
    I: Stream<Item = IN> + Unpin,
    O: Sink<OUT> + Clone + Send + Unpin + 'static,
    S: Spawn,
{
    let incoming = incoming
        .map(TransformPoolEvent::Incoming)
        .chain(stream::once(future::ready(
            TransformPoolEvent::IncomingClosed,
        )));

    let (close_sender, close_receiver) = mpsc::channel::<()>(0);
    let close_receiver = close_receiver.map(|()| TransformPoolEvent::TransformDone);

    let mut incoming_events = select(incoming, close_receiver);
    let mut num_concurrent: usize = 0;
    let mut incoming_closed = false;
    while let Some(event) = incoming_events.next().await {
        match event {
            TransformPoolEvent::Incoming(input_value) => {
                if num_concurrent >= max_concurrent {
                    warn!("transform_pool_loop: Dropping connection: max_concurrent exceeded");
                    // We drop the input value because we don't have any room to process it.
                    continue;
                }
                num_concurrent = num_concurrent.checked_add(1).unwrap();
                let mut c_outgoing = outgoing.clone();
                let mut c_transform = transform.clone();
                let mut c_close_sender = close_sender.clone();
                let fut = async move {
                    if let Some(output_value) = c_transform.transform(input_value).await {
                        let _ = c_outgoing.send(output_value).await;
                    }
                    let _ = c_close_sender.send(()).await;
                };
                spawner
                    .spawn(fut)
                    .map_err(|_| TransformPoolLoopError::SpawnError)?;
            }
            TransformPoolEvent::IncomingClosed => {
                incoming_closed = true;
            }
            TransformPoolEvent::TransformDone => {
                num_concurrent = num_concurrent.checked_sub(1).unwrap();
            }
        }
        if incoming_closed && num_concurrent == 0 {
            break;
        }
    }
    Ok(())
}

// TODO: Add tests!

/*

#[derive(Debug)]
pub enum TransformPoolError {
    SpawnError,
}

pub fn create_transform_pool<IN,OUT,T,S>(transform: T,
                                         max_concurrent: usize,
                                         mut spawner: S)
    -> Result<(mpsc::Sender<IN>, mpsc::Receiver<OUT>),  TransformPoolError>

where
    IN: Send + 'static,
    OUT: Send + 'static,
    T: FutTransform<Input=IN, Output=Option<OUT>> + Clone + Send + 'static,
    S: Spawn + Clone + Send + 'static,
{
    let (input_sender, incoming) = mpsc::channel(0);
    let (outgoing, output_receiver) = mpsc::channel(0);

    let loop_fut = transform_pool_loop(incoming,
                        outgoing,
                        transform,
                        max_concurrent,
                        spawner.clone())
        .map_err(|e| error!("transform_pool_loop() error: {:?}", e))
        .map(|_| ());

    spawner.spawn(loop_fut)
        .map_err(|_| TransformPoolError::SpawnError)?;

    Ok((input_sender, output_receiver))
}
*/
