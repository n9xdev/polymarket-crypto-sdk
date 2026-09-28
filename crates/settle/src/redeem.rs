use std::time::Duration;

use tracing::info;

pub fn spawn_redeem_timer(dry_run: bool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;
            if dry_run {
                continue;
            }
            info!("redeem timer tick (stub — wire relayer in production)");
        }
    })
}
