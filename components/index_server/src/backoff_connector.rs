use std::marker::PhantomData;

use common::conn::{BoxFuture, FutTransform};
use timer::utils::sleep_ticks;
use timer::TimerClient;

pub struct BackoffConnector<I, O, C> {
    connector: C,
    timer_client: TimerClient,
    backoff_ticks: usize,
    phantom_i: PhantomData<I>,
    phantom_o: PhantomData<O>,
}

// Automatic Derive(Clone) doesn't work for BackoffConnector,
// possibly because of the I and O in the phantom data structs?
// We implement our own clone here:
impl<I, O, C> Clone for BackoffConnector<I, O, C>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        BackoffConnector {
            connector: self.connector.clone(),
            timer_client: self.timer_client.clone(),
            backoff_ticks: self.backoff_ticks,
            phantom_i: PhantomData,
            phantom_o: PhantomData,
        }
    }
}

impl<I, O, C> BackoffConnector<I, O, C>
where
    C: FutTransform<Input = I, Output = Option<O>> + Clone,
{
    pub fn new(connector: C, timer_client: TimerClient, backoff_ticks: usize) -> Self {
        BackoffConnector {
            connector,
            timer_client,
            backoff_ticks,
            phantom_i: PhantomData,
            phantom_o: PhantomData,
        }
    }
}

impl<I, O, C> FutTransform for BackoffConnector<I, O, C>
where
    I: Send + Sync + Clone,
    O: Send,
    C: FutTransform<Input = I, Output = Option<O>> + Clone + Send,
{
    type Input = I;
    type Output = Option<O>;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        let mut c_self = self.clone();
        Box::pin(async move {
            loop {
                if let Some(output) = c_self.connector.transform(input.clone()).await {
                    return Some(output);
                }
                // Wait before we attempt to reconnect:
                sleep_ticks(c_self.backoff_ticks, c_self.timer_client.clone())
                    .await
                    .ok()?;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::channel::mpsc;
    use futures::executor::{ThreadPool, block_on};
    use futures::future::join;
    use futures::task::Spawn;
    use futures::{SinkExt, StreamExt};

    use common::conn::{ConnPairVec, ConnPair};
    use common::dummy_connector::DummyConnector;
    use timer::{dummy_timer_multi_sender, TimerTick};

    async fn task_backoff_connector_basic<S>(spawner: S)
    where
        S: Spawn,
    {
        let (mut tick_sender_receiver, timer_client) = dummy_timer_multi_sender(spawner);

        let (req_sender, mut req_receiver) = mpsc::channel(0);
        let dummy_connector = DummyConnector::<u32, Option<ConnPairVec>>::new(req_sender);

        let backoff_ticks = 8;

        let mut backoff_connector =
            BackoffConnector::new(dummy_connector, timer_client, backoff_ticks);

        let (opt_conn, _) = join(backoff_connector.transform(10u32), async move {
            // Connection attempt fails for the first 5 times:
            for _ in 0..5usize {
                let req = req_receiver.next().await.unwrap();
                assert_eq!(req.address, 10u32);
                req.reply(None); // Connection failed
                let mut tick_sender = tick_sender_receiver.next().await.unwrap();
                for _ in 0..8usize {
                    tick_sender.send(TimerTick).await.unwrap();
                }
            }
            let req = req_receiver.next().await.unwrap();
            assert_eq!(req.address, 10u32);

            // Finally we let the connection attempt succeed.
            // Return a dummy channel:
            let (sender, receiver) = mpsc::channel(1);
            req.reply(Some(ConnPair::from_raw(sender, receiver)));
        })
        .await;

        let (mut sender, mut receiver) = opt_conn.unwrap().split();
        sender.send(vec![1, 2, 3]).await.unwrap();
        assert_eq!(receiver.next().await.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_backoff_connector_basic() {
        let thread_pool = ThreadPool::new().unwrap();
        block_on(task_backoff_connector_basic(thread_pool.clone()));
    }
}
