//! The one registry of UCI engine processes: who owns each one, and how it starts and ends.
//!
//! An engine lives in a [`Slot`] under the key `(tab, engine path)`. The slot also names its
//! *owner*: the tab whose closing must end the engine. Engines the board starts under derived
//! tab ids (`<tab>white`, `<tab>black`) and Game Review engines (key tab `report_<tab>`) pass
//! the real tab as owner. [`EngineRegistry::kill_owner`] ends every slot whose owner or key tab
//! equals the given id exactly, including slots still starting up.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{watch, Mutex, OnceCell};

use crate::error::Error;

use super::process::EngineProcess;

pub type EngineKey = (String, String);
pub type EngineLines = tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>;

/// Time an engine gets to leave after `quit` before it is force-killed.
pub const KILL_GRACE: Duration = Duration::from_secs(2);

/// One engine's place in the registry; it exists from the start of start-up to the end of
/// the process.
pub struct Slot {
    owner: String,
    process: OnceCell<Arc<Mutex<EngineProcess>>>,
    reader: StdMutex<Option<EngineLines>>,
    closed: watch::Sender<bool>,
    /// The child's pid, so the exit hook can kill a process whose lock is held.
    pid: AtomicU32,
}

impl Slot {
    fn new(owner: String) -> Self {
        Self {
            owner,
            process: OnceCell::new(),
            reader: StdMutex::new(None),
            closed: watch::channel(false).0,
            pid: AtomicU32::new(0),
        }
    }

    /// Whether the slot was told to end (tab closed, review cancelled, app exiting).
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Turns true when the slot is told to end. A job holding the process lock watches this.
    pub fn closed(&self) -> watch::Receiver<bool> {
        self.closed.subscribe()
    }
}

/// An engine handed out by [`EngineRegistry::get_or_start`].
pub struct Started {
    pub slot: Arc<Slot>,
    pub process: Arc<Mutex<EngineProcess>>,
    /// The engine's output, `Some` for the call that created the process only.
    pub reader: Option<EngineLines>,
}

/// Every engine process of the app. Cloning shares the registry.
#[derive(Clone, Default)]
pub struct EngineRegistry {
    slots: Arc<DashMap<EngineKey, Arc<Slot>>>,
}

impl EngineRegistry {
    /// The started engine under `key`, if there is one.
    pub fn get(&self, key: &EngineKey) -> Option<Arc<Mutex<EngineProcess>>> {
        self.slots.get(key)?.process.get().cloned()
    }

    /// The engine under `key`, started from `path` for `owner` if it is not there yet.
    /// Overlapping calls for one key share one start-up and one process.
    ///
    /// # Errors
    /// `Error::EngineKilled` when the slot was killed before start-up finished (the process
    /// is then dead), or the start-up error.
    pub async fn get_or_start(
        &self,
        key: EngineKey,
        owner: String,
        path: PathBuf,
    ) -> Result<Started, Error> {
        let slot = self
            .slots
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Slot::new(owner)))
            .clone();
        let created = AtomicBool::new(false);
        let started = slot
            .process
            .get_or_try_init(|| async {
                let mut closed = slot.closed.subscribe();
                // Dropping the start-up future drops the child, which kills it.
                let (process, reader) = tokio::select! {
                    started = EngineProcess::new(path) => started?,
                    _ = closed.wait_for(|closed| *closed) => return Err(Error::EngineKilled),
                };
                slot.pid
                    .store(process.child.id().unwrap_or(0), Ordering::SeqCst);
                *slot.reader.lock().unwrap() = Some(reader);
                created.store(true, Ordering::SeqCst);
                Ok(Arc::new(Mutex::new(process)))
            })
            .await;
        let process = match started {
            Ok(process) => process.clone(),
            Err(e) => {
                self.remove(&key, &slot);
                return Err(e);
            }
        };
        if slot.is_closed() {
            // Killed just as start-up finished: the killer may have looked too early.
            self.remove(&key, &slot);
            process.lock().await.shutdown(Duration::ZERO).await.ok();
            return Err(Error::EngineKilled);
        }
        let reader = if created.load(Ordering::SeqCst) {
            slot.reader.lock().unwrap().take()
        } else {
            None
        };
        Ok(Started {
            slot,
            process,
            reader,
        })
    }

    /// Forget `slot` under `key` (not a newer slot under the same key).
    pub fn remove(&self, key: &EngineKey, slot: &Arc<Slot>) {
        self.slots.remove_if(key, |_, s| Arc::ptr_eq(s, slot));
    }

    /// End the engine under `key`, if any.
    pub async fn kill_key(&self, key: &EngineKey) -> Result<(), Error> {
        self.kill_where(|k, _| k == key).await
    }

    /// End every engine of `owner` (a tab id), and every engine whose key tab is exactly
    /// `owner`, starting ones included. "1" does not match "10".
    pub async fn kill_owner(&self, owner: &str) -> Result<(), Error> {
        self.kill_where(|k, slot| slot.owner == owner || k.0 == owner)
            .await
    }

    async fn kill_where(&self, matches: impl Fn(&EngineKey, &Slot) -> bool) -> Result<(), Error> {
        let hits: Vec<(EngineKey, Arc<Slot>)> = self
            .slots
            .iter()
            .filter(|e| matches(e.key(), e.value()))
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        let mut result = Ok(());
        for (key, slot) in hits {
            self.remove(&key, &slot);
            slot.closed.send_replace(true);
            if let Some(process) = slot.process.get() {
                if let Err(e) = process.lock().await.shutdown(KILL_GRACE).await {
                    result = Err(e);
                }
            }
        }
        result
    }

    /// Kill every engine without awaiting, for the synchronous app-exit hook. A process whose
    /// lock is held is killed through its pid.
    pub fn kill_all_now(&self) {
        use sysinfo::{Pid, PidExt, ProcessExt, System, SystemExt};
        for entry in self.slots.iter() {
            entry.closed.send_replace(true);
            let Some(process) = entry.process.get() else {
                continue; // still starting: the closed signal drops the child
            };
            match process.try_lock() {
                Ok(mut process) => {
                    let _ = process.child.start_kill();
                }
                Err(_) if entry.pid.load(Ordering::SeqCst) != 0 => {
                    let pid = Pid::from_u32(entry.pid.load(Ordering::SeqCst));
                    let mut system = System::new();
                    system.refresh_process(pid);
                    if let Some(process) = system.process(pid) {
                        process.kill();
                    }
                }
                Err(_) => {}
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::test_support::{alive, engine, eventually, pids};
    use super::*;

    fn key(tab: &str) -> EngineKey {
        (tab.to_string(), "engine".to_string())
    }

    #[tokio::test]
    async fn killing_a_tab_during_start_up_leaves_no_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = engine(dir.path(), "engine", "3");
        let registry = EngineRegistry::default();
        let start = tokio::spawn({
            let registry = registry.clone();
            async move { registry.get_or_start(key("tab"), "tab".into(), path).await }
        });
        assert!(eventually(5, || !pids(dir.path()).is_empty()).await);

        registry.kill_owner("tab").await.unwrap();

        assert!(matches!(
            start.await.unwrap().map(|_| ()),
            Err(Error::EngineKilled)
        ));
        let pid = pids(dir.path())[0];
        assert!(eventually(2, || !alive(pid)).await, "engine still alive");
    }

    #[tokio::test]
    async fn two_concurrent_starts_for_one_key_leave_one_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = engine(dir.path(), "engine", "0.3");
        let registry = EngineRegistry::default();

        let (a, b) = tokio::join!(
            registry.get_or_start(key("tab"), "tab".into(), path.clone()),
            registry.get_or_start(key("tab"), "tab".into(), path),
        );
        let (a, b) = (a.unwrap(), b.unwrap());

        assert!(Arc::ptr_eq(&a.process, &b.process));
        assert_eq!(a.reader.is_some() as u8 + b.reader.is_some() as u8, 1);
        let spawned = pids(dir.path());
        assert_eq!(spawned.len(), 1);
        assert!(alive(spawned[0]));
        registry.kill_owner("tab").await.unwrap();
    }

    #[tokio::test]
    async fn killing_tab_1_leaves_tab_10_alive() {
        let dir = tempfile::tempdir().unwrap();
        let path = engine(dir.path(), "engine", "0");
        let registry = EngineRegistry::default();
        for tab in ["1", "10"] {
            registry
                .get_or_start(key(tab), tab.into(), path.clone())
                .await
                .unwrap();
        }

        registry.kill_owner("1").await.unwrap();

        assert!(registry.get(&key("1")).is_none());
        let kept = registry.get(&key("10")).expect("tab 10 was killed");
        assert!(kept.lock().await.child.try_wait().unwrap().is_none());
        registry.kill_owner("10").await.unwrap();
    }

    #[tokio::test]
    async fn closing_a_tab_kills_the_engines_started_under_its_derived_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = engine(dir.path(), "engine", "0");
        let registry = EngineRegistry::default();
        for derived in ["1white", "1black"] {
            registry
                .get_or_start(key(derived), "1".into(), path.clone())
                .await
                .unwrap();
        }

        registry.kill_owner("1").await.unwrap();

        assert!(registry.get(&key("1white")).is_none());
        assert!(registry.get(&key("1black")).is_none());
        assert!(eventually(2, || pids(dir.path()).iter().all(|p| !alive(*p))).await);
    }

    #[tokio::test]
    async fn killing_engines_that_are_removed_concurrently_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = engine(dir.path(), "engine", "0");
        let registry = EngineRegistry::default();
        let started = registry
            .get_or_start(key("tab"), "tab".into(), path)
            .await
            .unwrap();

        let (a, b, _) = tokio::join!(
            registry.kill_owner("tab"),
            registry.kill_owner("tab"),
            async { registry.remove(&key("tab"), &started.slot) },
        );

        assert!(a.is_ok() && b.is_ok());
    }

    #[tokio::test]
    async fn kill_all_now_terminates_every_engine_even_one_whose_lock_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let path = engine(dir.path(), "engine", "0");
        let registry = EngineRegistry::default();
        let mut started = Vec::new();
        for tab in ["a", "b"] {
            started.push(
                registry
                    .get_or_start(key(tab), tab.into(), path.clone())
                    .await
                    .unwrap(),
            );
        }
        // Tab "b" is busy: something holds its process lock while the app exits.
        let held = started[1].process.lock().await;

        registry.kill_all_now();

        let pids = pids(dir.path());
        assert_eq!(pids.len(), 2);
        assert!(
            eventually(3, || pids.iter().all(|p| !alive(*p))).await,
            "an engine survived the exit hook"
        );
        drop(held);
    }
}
