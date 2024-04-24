use futures::future::{FutureExt as _, Shared};
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

#[derive(Clone)]
pub enum CachePolicy {
    // Stays in the cache forever
    Indefinite,
    // Stays in the cache for a finite amount of time
    Timed(Duration),
    // Is not cached, but the answer is shared between multiple consumers that
    // were already waiting for a result
    Shared,
    // No caching at all
    None,
}
#[derive(Clone)]
pub struct Cached<T> {
    pub value: T,
    pub policy: CachePolicy,
}

/// A cache for async functions
/// * Reentrancy:
///   The cache will not allow for any concurrency for a given key.
///   Instead, all callers to `get` will share the same future.
/// * Clone:
///   Clone is both cheap and referential
pub struct AsyncCache<F, Key, Fut: Future> {
    futures: Arc<RwLock<HashMap<Key, Shared<Fut>>>>,
    // This is taken out of the lock to limit sending it across threads in the
    // cleanup function, which reduces the ceremony in the where clause.
    ctor: Arc<F>,
}

impl<F, Key, Fut: Future> Clone for AsyncCache<F, Key, Fut> {
    fn clone(&self) -> Self {
        Self {
            futures: self.futures.clone(),
            ctor: self.ctor.clone(),
        }
    }
}

// What a mouthful of generics... It wasn't meant to be this way.
impl<T, F, Fut, Key> AsyncCache<F, Key, Fut>
where
    T: Clone + Send + Sync,
    F: Fn(&Key) -> Fut,
    Fut: 'static + Future<Output = Cached<T>> + Send,
    Key: 'static + Hash + Eq + Clone + Send + Sync,
{
    pub fn new(f: F) -> Self {
        Self {
            futures: Arc::new(RwLock::new(HashMap::new())),
            ctor: Arc::new(f),
        }
    }

    pub async fn get(&self, k: Key) -> T {
        loop {
            // Try with the read lock first to optimize for cache hits.
            let read = self.futures.read().await;
            let f = read.get(&k).cloned();
            drop(read);

            if let Some(f) = f {
                let f = f.await;
                match f.policy {
                    CachePolicy::None => continue,
                    _ => return f.value,
                }
            }

            // Uncached. This fut has the responsibility to cache the future
            let mut write = self.futures.write().await;
            // Possible that the future was added to the cache between times we held
            // the lock. So check for that again.
            let f = write.get(&k).cloned();
            if let Some(f) = f {
                drop(write);
                let f = f.await;
                match f.policy {
                    CachePolicy::None => continue,
                    _ => return f.value,
                }
            }
            let f = (self.ctor)(&k).shared();
            write.insert(k.clone(), f.clone());
            drop(write);

            // `Shared` propagates panics. Otherwise we would need to create
            // a drop impl to un-cache the future or poison or something.
            let result = f.await;

            match result.policy {
                CachePolicy::Shared | CachePolicy::None => {
                    let mut write = self.futures.write().await;
                    write.remove(&k);
                }
                // TODO: This is a terrible way to do this.
                // A better way would have a single cleanup task that
                // would trigger when the lowest time in a queue passed
                // and have that task cancel when the cache is dropped
                // Also, this requires a whole bunch of constraints on the
                // types like Send + Sync all over to satisfy the spawn/clone
                CachePolicy::Timed(duration) => {
                    let futures = self.futures.clone();
                    tokio::spawn(async move {
                        sleep(duration).await;
                        let mut write = futures.write().await;
                        write.remove(&k);
                    });
                }
                CachePolicy::Indefinite => {}
            }

            return result.value;
        }
    }
}
