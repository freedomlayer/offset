use std::collections::HashMap;
use std::hash::Hash;


/// Backup key from hmap to orig.
fn backup_key<K,V>(hmap: &HashMap<K,V>, orig: &mut HashMap<K,Option<V>>, key: &K) 
where
    K: Eq + Hash + Clone,
    V: Clone
{
    if !orig.contains_key(&key) {
        orig.insert(key.clone(), 
                    hmap.get(&key).map(|value| value.clone()));
    }
}

/// Transactional insert into a hashmap.
/// Keeps the original values before modification in orig.
fn trans_insert<K,V>(hmap: &mut HashMap<K,V>, orig: &mut HashMap<K,Option<V>>, key: K, value: V) -> Option<V> 
where
    K: Eq + Hash + Clone,
    V: Clone
{
    backup_key(hmap, orig, &key);
    hmap.insert(key, value)
}

/// Transactional remove from a hashmap.
/// Keeps the original values before modification in orig.
fn trans_remove<K,V>(hmap: &mut HashMap<K,V>, orig: &mut HashMap<K,Option<V>>, key: &K) -> Option<V> 
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    backup_key(hmap, orig, key);
    hmap.remove(key)
}

/// Restore original Hashmap. hmap is the modified hashmap, and orig contains original values for
/// any keys that were modified.
fn trans_restore<K,V>(hmap: &mut HashMap<K,V>, orig: HashMap<K, Option<V>>) 
where
    K: Eq + Hash,
{
    for (key, opt_value) in orig.into_iter() {
        match opt_value {
            Some(value) => {hmap.insert(key, value);},
            None => {hmap.remove(&key);},
        }
    }
}


/// Transactional HashMap.
/// This struct wraps a hashmap, allowing to insert and remove entries.
/// It is possible to restore all changes made to the original hashmap by calling abort(), or
/// accept all the changes by calling commit().
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
        let TransHashMap {mut hmap, orig} = self;
        trans_restore(&mut hmap, orig);
        hmap
    }

    /// Commit to all changes to hashmap and return the hashmap.
    pub fn commit(self) -> HashMap<K,V> {
        self.hmap
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_key_exists() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);

        backup_key(&hmap, &mut orig, &1);
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&1), Some(&Some(11)));
    }

    #[test]
    fn test_backup_key_nonexistent() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);

        backup_key(&hmap, &mut orig, &5);
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&5), Some(&None));
    }

    #[test]
    fn test_backup_key_twice_exists() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);

        backup_key(&hmap, &mut orig, &2);
        backup_key(&hmap, &mut orig, &2);
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&2), Some(&Some(12)));
    }

    #[test]
    fn test_backup_key_twice_nonexistent() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);

        backup_key(&hmap, &mut orig, &5);
        backup_key(&hmap, &mut orig, &5);
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&5), Some(&None));
    }

    #[test]
    fn test_trans_insert_nonexistent() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let res = trans_insert(&mut hmap, &mut orig, 4, 14);
        assert_eq!(res, None);

        assert_eq!(hmap.len(), 5);
        assert_eq!(hmap.get(&4), Some(&14usize));
        assert_eq!(orig.get(&4), Some(&None));
        assert_eq!(orig.len(), 1);

        trans_restore(&mut hmap, orig);
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_insert_exists() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let res = trans_insert(&mut hmap, &mut orig, 2, 22);
        assert_eq!(res, Some(12));

        assert_eq!(hmap.len(), 4);
        assert_eq!(hmap.get(&2), Some(&22usize));
        assert_eq!(orig.get(&2), Some(&Some(12usize)));
        assert_eq!(orig.len(), 1);

        trans_restore(&mut hmap, orig);
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_remove_nonexistent() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let res = trans_remove(&mut hmap, &mut orig, &5);
        assert_eq!(res, None);

        assert_eq!(hmap.len(), 4);
        assert_eq!(orig.get(&5), Some(&None));
        assert_eq!(orig.len(), 1);
        assert_eq!(hmap.get(&5), None);

        trans_restore(&mut hmap, orig);
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_remove_exists() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let res = trans_remove(&mut hmap, &mut orig, &1);
        assert_eq!(res, Some(11));

        assert_eq!(hmap.len(), 3);
        assert_eq!(hmap.get(&1), None);
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&1), Some(&Some(11)));

        trans_restore(&mut hmap, orig);
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_insert_twice() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let res = trans_insert(&mut hmap, &mut orig, 2, 22);
        assert_eq!(res, Some(12));
        let res = trans_insert(&mut hmap, &mut orig, 2, 32);
        assert_eq!(res, Some(22));

        assert_eq!(hmap.len(), 4);
        assert_eq!(hmap.get(&2), Some(&32));
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&2), Some(&Some(12)));

        trans_restore(&mut hmap, orig);
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_remove_twice() {
        let mut orig = HashMap::new();
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let res = trans_remove(&mut hmap, &mut orig, &1);
        assert_eq!(res, Some(11));
        let res = trans_remove(&mut hmap, &mut orig, &1);
        assert_eq!(res, None);

        assert_eq!(hmap.len(), 3);
        assert_eq!(hmap.get(&1), None);
        assert_eq!(orig.len(), 1);
        assert_eq!(orig.get(&1), Some(&Some(11)));

        trans_restore(&mut hmap, orig);
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_hash_map_cancel() {
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let mut tmap = TransHashMap::new(hmap);
        let res = tmap.insert(2,22);
        assert_eq!(res, Some(12));
        assert_eq!(tmap.get_hmap().get(&3), Some(&13));
        let res = tmap.remove(&3);
        assert_eq!(res, Some(13));

        let hmap = tmap.cancel();
        assert_eq!(hmap, saved_hmap);
    }

    #[test]
    fn test_trans_hash_map_commit() {
        let mut hmap = HashMap::<usize,usize>::new();
        hmap.insert(0,10);
        hmap.insert(1,11);
        hmap.insert(2,12);
        hmap.insert(3,13);
        let saved_hmap = hmap.clone();

        let mut tmap = TransHashMap::new(hmap);
        tmap.insert(2,22);
        let hmap = tmap.commit();
        assert_eq!(hmap.get(&2), Some(&22));
        assert_eq!(hmap.get(&0), Some(&10));
        assert_ne!(hmap, saved_hmap);
    }
}
