//! Serializes provider transitions against commands and cancels old background work.
use crate::error::{Result, err};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, watch};

pub(super) struct SessionGate {
    gate: RwLock<()>,
    epoch: watch::Sender<i64>,
    active: std::sync::atomic::AtomicI64,
}

impl Default for SessionGate {
    fn default() -> Self {
        Self {
            gate: RwLock::new(()),
            epoch: watch::channel(1).0,
            active: std::sync::atomic::AtomicI64::new(1),
        }
    }
}

impl SessionGate {
    pub async fn snapshot(&self) -> RwLockReadGuard<'_, ()> {
        self.gate.read().await
    }
    pub fn current(&self) -> i64 {
        self.active.load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn subscribe(&self) -> watch::Receiver<i64> {
        self.epoch.subscribe()
    }

    pub async fn read(&self, expected: i64) -> Result<RwLockReadGuard<'_, ()>> {
        let lease = self.gate.read().await;
        if expected != self.current() {
            return Err(err("the music provider session changed; try again"));
        }
        Ok(lease)
    }

    /// Call while holding the daemon's transition mutex. Existing commands finish;
    /// background work observes the epoch change and releases its read lease.
    pub async fn advance(&self) -> RwLockWriteGuard<'_, ()> {
        self.epoch.send_modify(|epoch| *epoch += 1);
        let exclusive = self.gate.write().await;
        self.active
            .store(*self.epoch.borrow(), std::sync::atomic::Ordering::Release);
        exclusive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[tokio::test]
    async fn a_switch_waits_for_mutations_and_rejects_queued_old_work() {
        let session = Arc::new(SessionGate::default());
        let old = session.current();
        let lease = session.read(old).await.unwrap();
        let mut changed = session.subscribe();
        let s = session.clone();
        let switch = tokio::spawn(async move {
            let _exclusive = s.advance().await;
        });
        changed.changed().await.unwrap();
        assert!(
            !switch.is_finished(),
            "an already dispatched mutation must finish"
        );
        drop(lease);
        switch.await.unwrap();
        assert!(session.read(old).await.is_err());
        assert!(session.read(session.current()).await.is_ok());
    }
}
