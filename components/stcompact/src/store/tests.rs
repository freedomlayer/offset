use futures::executor::{block_on, ThreadPool};

use crate::store::file_store::open_file_store;

#[test]
fn test_file_store() {
    let file_spawner = ThreadPool::new().unwrap();
}
