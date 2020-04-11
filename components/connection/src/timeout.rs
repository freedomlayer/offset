use futures::{future, select, FutureExt, StreamExt};

use common::conn::{BoxFuture, FutTransform};
use timer::TimerClient;

/// A future transform's wrapper, adding timeout
#[derive(Debug, Clone)]
pub struct TimeoutFutTransform<FT> {
    fut_transform: FT,
    timer_client: TimerClient,
    timeout_ticks: usize,
}

impl<FT> TimeoutFutTransform<FT> {
    pub fn new(fut_transform: FT, timer_client: TimerClient, timeout_ticks: usize) -> Self {
        Self {
            fut_transform,
            timer_client,
            timeout_ticks,
        }
    }
}

impl<FT, I, O> FutTransform for TimeoutFutTransform<FT>
where
    FT: FutTransform<Input = I, Output = Option<O>> + Send,
    I: Send + 'static,
    O: Send,
{
    type Input = I;
    type Output = Option<O>;

    fn transform(&mut self, input: Self::Input) -> BoxFuture<'_, Self::Output> {
        Box::pin(async move {
            let timer_stream = match self.timer_client.request_timer_stream().await {
                Ok(timer_stream) => timer_stream,
                Err(e) => {
                    error!("TimeoutTransform: request_timer_stream() error: {:?}", e);
                    return None;
                }
            };

            // A future that waits `timeout_ticks`:
            let fut_timeout = timer_stream
                .take(self.timeout_ticks)
                .for_each(|_| future::ready(()));
            let fut_output = self.fut_transform.transform(input);

            // Race execution of `fut_transform` against the timer:
            select! {
                _fut_timeout = fut_timeout.fuse() => None,
                fut_output = fut_output.fuse() => fut_output,
            }
        })
    }
}
