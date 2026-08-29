use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

struct PoolItem<V> {
    value: V,
    used: bool,
}

/// キーごとに複数のリソースインスタンスを管理するプール。
///
/// 並列パイプライン実行時に、同じキーを持つ複数のサブパイプラインが
/// それぞれ独立したリソースを取得できることを保証します。
/// フレーム開始時に `reset_used` を呼ぶことで、前フレームのリソースを再利用可能にします。
pub(crate) struct ResourcePool<K, V> {
    pool: HashMap<K, Vec<PoolItem<V>>>,
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
    /// used フラグが立っていないインスタンスがあれば再利用し、なければ `create` で新規作成します。
    /// 取得したリソースには used フラグが立てられ、`reset_used` が呼ばれるまで
    /// 他の呼び出しからは返されません。
    pub fn acquire(&mut self, key: K, create: impl FnOnce() -> V) -> V {
        let items = self.pool.entry(key).or_insert_with(Vec::new);

        // used フラグが立っていない最初のアイテムを探して返す
        if let Some(item) = items.iter_mut().find(|item| !item.used) {
            item.used = true;
            return item.value.clone();
        }

        // 未使用アイテムがなければ新規作成
        let value = create();
        items.push(PoolItem {
            value: value.clone(),
            used: true,
        });
        value
    }

    /// used フラグをリセットします。
    /// 前フレームで一度も使用されなかったキーはエントリごと削除します。
    /// 使用されたキーについて、アイテム数が上限を超えている場合は未使用(used=false)のアイテムを
    /// 先に削除してから残りの used フラグをリセットします。使用中(used=true)のアイテムは上限超過時も削除しません。
    pub fn reset_used(&mut self) {
        self.pool.retain(|_, items| {
            // 前フレームで一度も使われなかったキーはエントリごと削除
            let any_used = items.iter().any(|item| item.used);
            if !any_used {
                return false;
            }

            if items.len() > self.max_per_key {
                // 未使用アイテムを削除して上限以内に収める（使用中は保持）
                items.retain(|item| item.used);
            }
            // used フラグをリセット
            for item in items.iter_mut() {
                item.used = false;
            }
            true
        });
    }

    pub fn set_max_per_key(&mut self, max: usize) {
        self.max_per_key = max;
        self.pool.retain(|_, items| {
            if items.len() > max {
                items.retain(|item| item.used);
            }
            !items.is_empty()
        });
    }
}

/// LRU 方式で単一インスタンスをキャッシュする汎用キャッシュ。
///
/// パイプラインのように複数の呼び出し元が同じインスタンスを同時に共有できる
/// 読み取り専用・ステートレスなリソース向け。
pub struct LruCache<K, V> {
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
