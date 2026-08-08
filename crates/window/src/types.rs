use dashmap::DashMap;
use nohash_hasher::BuildNoHashHasher;

/// Fast integer [`DashMap`]
pub(crate) type IntDashMap<K, V> = DashMap<K, V, BuildNoHashHasher<K>>;
