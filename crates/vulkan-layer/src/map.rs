use dashmap::DashMap;
use nohash_hasher::BuildNoHashHasher;

/// Fast integer keyed map without hashing overhead.
pub type IntDashMap<K, V> = DashMap<K, V, BuildNoHashHasher<K>>;
