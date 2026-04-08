use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

struct PoolEntry<V> {
    available: Vec<V>,
    in_use: Vec<V>,
}

/// キーごとに複数のリソースインスタンスを管理するプール。
///
/// 並列パイプライン実行時に、同じキーを持つ複数のサブパイプラインが
/// それぞれ独立したリソースを取得できることを保証します。
/// フレーム開始時に `release_all` を呼ぶことで、前フレームのリソースを再利用可能にします。
pub(crate) struct ResourcePool<K, V> {
    pool: HashMap<K, PoolEntry<V>>,
    max_per_key: usize,
}

impl<K: Hash + Eq + Clone, V: Clone> ResourcePool<K, V> {
    pub fn new(max_per_key: usize) -> Self {
        Self {
            pool: HashMap::new(),
            max_per_key,
        }
    }

    /// キーに対応するリソースを取得します。
    /// available なインスタンスがあれば再利用し、なければ `create` で新規作成します。
    /// 取得したリソースは in_use としてマークされ、`release_all` が呼ばれるまで
    /// 他の呼び出しからは返されません。
    pub fn acquire(&mut self, key: K, create: impl FnOnce() -> V) -> V {
        let entry = self.pool.entry(key).or_insert_with(|| PoolEntry {
            available: Vec::new(),
            in_use: Vec::new(),
        });
        let item = entry.available.pop().unwrap_or_else(create);
        entry.in_use.push(item.clone());
        item
    }

    /// in_use のすべてのリソースを available に返します。
    /// フレームの開始時に呼び出してください。
    pub fn release_all(&mut self) {
        for entry in self.pool.values_mut() {
            entry.available.append(&mut entry.in_use);
            // キーごとの上限を超えた分は古い順に削除
            if entry.available.len() > self.max_per_key {
                let excess = entry.available.len() - self.max_per_key;
                entry.available.drain(..excess);
            }
        }
    }

    pub fn set_max_per_key(&mut self, max: usize) {
        self.max_per_key = max;
        for entry in self.pool.values_mut() {
            if entry.available.len() > max {
                let excess = entry.available.len() - max;
                entry.available.drain(..excess);
            }
        }
    }
}

/// LRU 方式で単一インスタンスをキャッシュする汎用キャッシュ。
///
/// パイプラインのように複数の呼び出し元が同じインスタンスを同時に共有できる
/// 読み取り専用・ステートレスなリソース向け。
pub(crate) struct LruCache<K, V> {
    items: HashMap<K, V>,
    order: VecDeque<K>,
    max_size: usize,
}

impl<K: Hash + Eq + Clone, V: Clone> LruCache<K, V> {
    pub fn new(max_size: usize) -> Self {
        Self {
            items: HashMap::new(),
            order: VecDeque::new(),
            max_size,
        }
    }

    /// キャッシュからエントリを取得し、LRU 順序を更新します。
    pub fn get(&mut self, key: &K) -> Option<V> {
        let v = self.items.get(key)?.clone();
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
        self.order.push_front(key.clone());
        Some(v)
    }

    /// キャッシュにエントリを挿入します。容量を超えた場合は最も古いエントリを削除します。
    pub fn insert(&mut self, key: K, value: V) {
        self.items.insert(key.clone(), value);
        self.order.push_front(key);
        if self.order.len() > self.max_size {
            if let Some(oldest) = self.order.pop_back() {
                self.items.remove(&oldest);
            }
        }
    }

    /// キャッシュから取得し、なければ `create` で作成してキャッシュに挿入します。
    pub fn get_or_insert_with(&mut self, key: K, create: impl FnOnce() -> V) -> V {
        if let Some(v) = self.get(&key) {
            return v;
        }
        let v = create();
        self.insert(key, v.clone());
        v
    }

    pub fn set_max_size(&mut self, max: usize) {
        self.max_size = max;
        while self.order.len() > self.max_size {
            if let Some(oldest) = self.order.pop_back() {
                self.items.remove(&oldest);
            }
        }
    }
}
