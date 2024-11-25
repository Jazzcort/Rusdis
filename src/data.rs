use crate::rdb_file_reader::{Dataset, ValueType};
//use crate::utils::generate_random_string;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct ReplicationInfo {
    role: ReplicaRole,
    master_replid: String,
    master_repl_offset: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplicaRole {
    Master,
    Slave,
}

impl ReplicationInfo {
    pub fn new() -> Self {
        ReplicationInfo {
            role: ReplicaRole::Master,
            master_replid: "8371b4fb1155b71f4a04d3e1bc3e18c4a990aeeb".to_string(),
            master_repl_offset: 0,
        }
    }

    pub fn get_role(&self) -> ReplicaRole {
        self.role.clone()
    }

    pub fn get_master_replid(&self) -> &String {
        &self.master_replid
    }

    pub fn get_master_repl_offset(&self) -> u64 {
        self.master_repl_offset
    }

    pub fn change_role(&mut self, new_role: ReplicaRole) {
        self.role = new_role
    }

    pub fn set_master_replid(&mut self, id: String) {
        self.master_replid = id
    }

    // Todo: Handle the overflow cases
    pub fn increment_offset(&mut self, num: u64) {
        self.master_repl_offset += num
    }
}

#[derive(Clone, Debug)]
pub struct StringData {
    data: String,
    expiration: Option<SystemTime>,
}

impl StringData {
    pub fn new(data: String, expiration: Option<SystemTime>) -> Self {
        StringData { data, expiration }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expiration) = self.expiration {
            let now = SystemTime::now();

            if now >= expiration {
                true
            } else {
                false
            }
        } else {
            return false;
        }
    }

    pub fn get_data(&self) -> &String {
        &self.data
    }

    pub fn set_data(&mut self, s: String) {
        self.data = s;
    }
}

#[derive(Debug)]
pub struct Admin {
    databases: Vec<Database>,
    cur_idx: usize,
}

impl Admin {
    pub fn new(preload_datasets: Vec<Dataset>) -> Self {
        let length = preload_datasets.len().max(16);
        let mut databases = vec![Database::default(); length];

        for (idx, dataset) in preload_datasets.into_iter().enumerate() {
            let mut string_data_vec = vec![];
            for (key, value, expiration) in dataset.get_pairs().into_iter() {
                match value {
                    ValueType::String(string) => string_data_vec.push((
                        key,
                        StringData {
                            data: string,
                            expiration,
                        },
                    )),
                }
            }

            let string_data = string_data_vec
                .into_iter()
                .collect::<HashMap<String, StringData>>();

            databases[idx].string_data = Arc::new(Mutex::new(string_data));
        }
        Admin {
            databases,
            cur_idx: 0,
        }
    }

    pub fn select_database(&mut self, idx: usize) {
        if idx < self.databases.len() {
            self.cur_idx = idx;
        }
    }

    pub fn get_string_data_map(&self) -> Arc<Mutex<HashMap<String, StringData>>> {
        let arc = self.databases[self.cur_idx].string_data.clone();
        arc
    }
}

#[derive(Default, Debug, Clone)]
pub struct Database {
    string_data: Arc<Mutex<HashMap<String, StringData>>>,
}

#[cfg(test)]

mod test {
    use super::*;

    #[test]
    fn test_data_expired() {
        let now = SystemTime::now();
        let fu = now.checked_add(Duration::from_millis(20)).unwrap();
        let data = StringData::new("Data stores here".to_string(), Some(fu));

        assert!(!data.is_expired());

        std::thread::sleep(Duration::from_millis(30));

        assert!(data.is_expired());

        let data2 = StringData::new("Never expire".to_string(), None);
        std::thread::sleep(Duration::from_millis(30));
        assert!(!data2.is_expired());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!data2.is_expired());
    }

    #[test]
    fn test_database() {
        let d1 = Dataset {
            pairs: vec![(
                "key1".to_string(),
                ValueType::String("value1".to_string()),
                Some(SystemTime::now()),
            )],
        };
        let d2 = Dataset {
            pairs: vec![
                (
                    "key1".to_string(),
                    ValueType::String("value1".to_string()),
                    Some(SystemTime::now()),
                ),
                (
                    "car".to_string(),
                    ValueType::String("BMW".to_string()),
                    None,
                ),
            ],
        };

        let datasets = vec![d1, d2];
        let mut admin = Admin::new(Some(datasets));
        admin.select_database(1);
    }
}
