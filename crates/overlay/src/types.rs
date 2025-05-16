use dashmap::DashMap;
use nohash_hasher::BuildNoHashHasher;

pub type IntDashMap<K, V> = DashMap<K, V, BuildNoHashHasher<K>>;
