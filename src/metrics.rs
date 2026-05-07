use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficSnapshot {
    pub upload_bytes: u64,
    pub download_bytes: u64,
}

#[derive(Debug, Default)]
pub struct TrafficRegistry {
    counters: RwLock<HashMap<String, TrafficSnapshot>>,
}

impl TrafficRegistry {
    pub fn record(&self, user: impl Into<String>, upload_bytes: u64, download_bytes: u64) {
        let user = user.into();
        if user.is_empty() {
            return;
        }

        let mut counters = self.counters.write().expect("traffic registry lock poisoned");
        let counter = counters.entry(user).or_default();
        counter.upload_bytes = counter.upload_bytes.saturating_add(upload_bytes);
        counter.download_bytes = counter.download_bytes.saturating_add(download_bytes);
    }

    pub fn remove(&self, user: &str) -> Option<TrafficSnapshot> {
        self.counters
            .write()
            .expect("traffic registry lock poisoned")
            .remove(user)
    }

    pub fn snapshot(&self, user: &str) -> Option<TrafficSnapshot> {
        self.counters
            .read()
            .expect("traffic registry lock poisoned")
            .get(user)
            .copied()
    }

    pub fn all(&self) -> Vec<(String, TrafficSnapshot)> {
        let mut values: Vec<_> = self
            .counters
            .read()
            .expect("traffic registry lock poisoned")
            .iter()
            .map(|(user, traffic)| (user.clone(), *traffic))
            .collect();
        values.sort_by(|left, right| left.0.cmp(&right.0));
        values
    }

    pub fn totals(&self) -> TrafficSnapshot {
        self.all().into_iter().fold(TrafficSnapshot::default(), |mut total, (_, traffic)| {
            total.upload_bytes = total.upload_bytes.saturating_add(traffic.upload_bytes);
            total.download_bytes = total.download_bytes.saturating_add(traffic.download_bytes);
            total
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{TrafficRegistry, TrafficSnapshot};

    #[test]
    fn records_and_sums_user_traffic() {
        let registry = TrafficRegistry::default();

        registry.record("user-a", 10, 20);
        registry.record("user-a", 5, 7);
        registry.record("user-b", 1, 2);

        assert_eq!(
            registry.snapshot("user-a"),
            Some(TrafficSnapshot {
                upload_bytes: 15,
                download_bytes: 27,
            })
        );
        assert_eq!(
            registry.totals(),
            TrafficSnapshot {
                upload_bytes: 16,
                download_bytes: 29,
            }
        );
    }

    #[test]
    fn ignores_empty_user_tags() {
        let registry = TrafficRegistry::default();

        registry.record("", 10, 10);

        assert!(registry.all().is_empty());
    }
}
