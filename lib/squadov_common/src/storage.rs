use serde::Deserialize;
use std::collections::HashMap;
use async_std::sync::RwLock;

#[derive(Deserialize,Debug,Clone)]
pub struct CloudStorageBucketsConfig {
    pub global: String,
    pub legacy: String,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum CloudStorageLocation {
    Global,
}

pub struct StorageManager<T> {
    buckets: RwLock<HashMap<String, T>>,
    location_to_bucket: HashMap<CloudStorageLocation, String>,
}

impl<T> StorageManager<T>
where
    T: Clone
{
    pub fn new() -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
            location_to_bucket: HashMap::new(),
        }
    }

    pub async fn new_bucket(&self, bucket: &str, manager: T) {
        let mut buckets = self.buckets.write().await;
        buckets.insert(bucket.to_string(), manager);
    }

    pub async fn get_bucket(&self, nm: &str) -> Option<T> {
        let buckets = self.buckets.read().await;
        buckets.get(nm).map(|x| { x.clone() })
    }

    pub fn set_location_map(&mut self, location: CloudStorageLocation, bucket: &str) {
        self.location_to_bucket.insert(location, bucket.to_string());
    }

    pub fn get_bucket_for_location(&self, loc: CloudStorageLocation) -> Option<String> {
        self.location_to_bucket.get(&loc).map(|x| { x.to_string() })
    }
}