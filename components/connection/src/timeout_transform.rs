use futures::{future, select, FutureExt, StreamExt};

use common::conn::{BoxFuture, FutTransform};
use timer::TimerClient;

/// A future transform's wrapper, adding timeout
#[derive(Debug)]
pub struct TimeoutTransform<FT> {
    fut_transform: FT,
    timer_client: TimerClient,
    timeout_ticks: usize,
}

impl<FT> TimeoutTransform<FT> {
    pub fn new(fut_transform: FT, timer_client: TimerClient, timeout_ticks: usize) -> Self {
        Self {
            fut_transform,
            timer_client,
            timeout_ticks,
        }
    }
}

impl<FT> FutTransform for TimeoutTransform<FT>
where
    FT: FutTransform + Send,
    FT::Input: Send,
{
    type Input = FT::Input;
    type Output = Option<FT::Output>;

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
                fut_output = fut_output.fuse() => Some(fut_output),
            }
        })
    }
}
