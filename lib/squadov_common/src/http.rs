use crate::SquadOvError;
use tokio::sync::{Semaphore};
use std::sync::Arc;

pub struct RateLimiter {
    seconds: u64,
    limiter: Arc<Semaphore>,
}

impl RateLimiter {
    pub fn new(requests: usize, seconds: u64) -> Self {
        Self {
            seconds,
            limiter: Arc::new(Semaphore::new(requests)),
        }
    }

    pub async fn consume(&self) -> Result<(), SquadOvError> {
        let permit = self.limiter.acquire().await?;
        permit.forget();

        let seconds = self.seconds;
        let limiter = self.limiter.clone();
        tokio::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_secs(seconds)).await;
            limiter.add_permits(1);
        });
        Ok(())
    }
}