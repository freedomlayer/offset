use futures::{Sink, SinkExt, Stream, StreamExt};

use crate::conn::FutTransform;

#[derive(Debug)]
pub enum TransformPoolLoopError {}

/// Transform a stream of incoming items to outgoing items.
/// The transformation is asynchronous, therefore outgoing items
/// might not be in the same order in which the incoming items entered.
///
/// max_concurrent is the maximum amount of concurrent transformations.
pub async fn transform_pool_loop<IN, OUT, I, O, T>(
    incoming: I,
    outgoing: O,
    transform: T,
    max_concurrent: usize,
) -> Result<(), TransformPoolLoopError>
where
    IN: Send + 'static,
    OUT: Send,
    T: FutTransform<Input = IN, Output = Option<OUT>> + Clone + Send + 'static,
    I: Stream<Item = IN> + Unpin,
    O: Sink<OUT> + Clone + Send + Unpin + 'static,
{
    incoming
        .for_each_concurrent(Some(max_concurrent), move |input_value| {
            let mut c_outgoing = outgoing.clone();
            let mut c_transform = transform.clone();
            async move {
                if let Some(output_value) = c_transform.transform(input_value).await {
                    let _ = c_outgoing.send(output_value).await;
                }
            }
        })
        .await;
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
