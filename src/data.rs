use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Data {
    data: String,
    expiration: Option<Instant>,
}

impl Data {
    pub fn new(data: String, expiration: Option<Instant>) -> Self {
        Data { data, expiration }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expiration) = self.expiration {
            let now = Instant::now();

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
}

#[cfg(test)]

mod test {
    use super::*;

    #[test]
    fn test_data_expired() {
        let now = Instant::now();
        let fu = now.checked_add(Duration::from_millis(20)).unwrap();
        let data = Data::new("Data stores here".to_string(), Some(fu));

        assert!(!data.is_expired());

        std::thread::sleep(Duration::from_millis(30));

        assert!(data.is_expired());

        let data2 = Data::new("Never expire".to_string(), None);
        std::thread::sleep(Duration::from_millis(30));
        assert!(!data2.is_expired());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!data2.is_expired());
    }
}
