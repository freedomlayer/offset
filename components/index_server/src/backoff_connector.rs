use std::marker::PhantomData;

use common::conn::{FutTransform, BoxFuture};
use timer::TimerClient;
use timer::utils::sleep_ticks;

pub struct BackoffConnector<I,O,C> {
    connector: C,
    timer_client: TimerClient,
    backoff_ticks: usize,
    phantom_i: PhantomData<I>,
    phantom_o: PhantomData<O>,
}


// Automatic Derive(Clone) doesn't work for BackoffConnector,
// possibly because of the I and O in the phantom data structs?
// We implement our own clone here:
impl<I,O,C> Clone for BackoffConnector<I,O,C> 
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

impl<I,O,C> BackoffConnector<I,O,C> 
where
    C: FutTransform<Input=I, Output=Option<O>> + Clone,
{
    pub fn new(connector: C,
               timer_client: TimerClient,
               backoff_ticks: usize) -> Self {

        BackoffConnector {
            connector, 
            timer_client,
            backoff_ticks,
            phantom_i: PhantomData,
            phantom_o: PhantomData,
        }
    }
}

impl<I,O,C> FutTransform for BackoffConnector<I,O,C> 
where
    I: Send + Clone,
    O: Send,
    C: FutTransform<Input=I, Output=Option<O>> + Clone + Send,
{
    type Input = I;
    type Output = Option<O>;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        let mut c_self = self.clone();
        Box::pin(async move {
            loop {
                if let Some(output) = await!(c_self.connector.transform(input.clone())) {
                    return Some(output);
                }
                // Wait before we attempt to reconnect:
                await!(sleep_ticks(c_self.backoff_ticks, c_self.timer_client.clone())).ok()?;
            }
        })
    }
}
