use std::collections::HashMap;
use std::hash::Hash;


/// Transactional insert into a hashmap.
/// Keeps the original values before modification in orig.
fn trans_insert<K,V>(hmap: &mut HashMap<K,V>, orig: &mut HashMap<K,Option<V>>, key: K, value: V) -> Option<V> 
where
    K: Eq + Hash + Clone,
    V: Clone
{
    if !orig.contains_key(&key) {
        orig.insert(key.clone(), 
                    hmap.get(&key).map(|value| value.clone()));
    }
    hmap.insert(key, value)
}

/// Transactional remove from a hashmap.
/// Keeps the original values before modification in orig.
fn trans_remove<K,V>(hmap: &mut HashMap<K,V>, orig: &mut HashMap<K,Option<V>>, key: &K) -> Option<V> 
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    if !orig.contains_key(key) {
        orig.insert(key.clone(), 
                    hmap.get(key).map(|value| value.clone()));
    }
    hmap.remove(key)
}

/// Restore original Hashmap. hmap is the modified hashmap, and orig contains original values for
/// any keys that were modified.
fn restore<K,V>(mut hmap: HashMap<K,V>, orig: HashMap<K, Option<V>>) -> HashMap<K,V> 
where
    K: Eq + Hash,
{
    for (key, opt_value) in orig.into_iter() {
        match opt_value {
            Some(value) => {hmap.insert(key, value);},
            None => {hmap.remove(&key);},
        }
    }
    hmap
}


pub struct TransHashMap<K,V> {
    hmap: HashMap<K,V>,
    orig: HashMap<K,Option<V>>,
}

impl<K,V> TransHashMap<K,V> 
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create from an existing hash map.
    pub fn new(hmap: HashMap<K,V>) -> Self {
        TransHashMap {
            hmap,
            orig: HashMap::new(),
        }
    }

    /// Insert an element (key, value) into the hash map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        trans_insert(&mut self.hmap, &mut self.orig, key, value)
    }

    /// Remove an element from the hash map according to its key.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        trans_remove(&mut self.hmap, &mut self.orig, key)
    }

    /// Get a reference to the current state of the HashMap.
    pub fn get_hmap(&self) -> &HashMap<K,V> {
        &self.hmap
    }

    /// Cancel all modifications and return the original hash map (Before any changes)
    pub fn cancel(self) -> HashMap<K,V> {
        restore(self.hmap, self.orig)
    }

    /// Commit to all changes to hashmap and return the hashmap.
    pub fn commit(self) -> HashMap<K,V> {
        self.hmap
    }
}
