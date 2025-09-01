//! Common type aliases used in many modules internally.

use dashmap::DashMap;
use nohash_hasher::BuildNoHashHasher;

/// Fast integer [`DashMap`]
pub type IntDashMap<K, V> = DashMap<K, V, BuildNoHashHasher<K>>;
