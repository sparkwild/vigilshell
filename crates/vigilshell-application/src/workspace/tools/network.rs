use std::collections::BTreeMap;
type Counters = BTreeMap<String, (u64, u64)>;
#[derive(Default)]
pub(super) struct NetworkRate {
    previous: Option<(f64, Counters)>,
}
impl NetworkRate {
    pub fn sample(
        &mut self,
        now: Option<f64>,
        counters: Option<Counters>,
    ) -> (Option<f64>, Option<f64>) {
        let previous = self.previous.take();
        let (Some(now), Some(counters)) = (now.filter(|n| n.is_finite() && *n >= 0.0), counters)
        else {
            return (None, None);
        };
        let result = previous.and_then(|(time, old)| {
            let seconds = now - time;
            if seconds <= 0.0 || counters.is_empty() || !old.keys().eq(counters.keys()) {
                return None;
            }
            let (rx, tx) =
                counters
                    .iter()
                    .try_fold((0u64, 0u64), |(rx, tx), (name, (received, sent))| {
                        let before = old.get(name)?;
                        Some((
                            rx.checked_add(received.checked_sub(before.0)?)?,
                            tx.checked_add(sent.checked_sub(before.1)?)?,
                        ))
                    })?;
            Some((rx as f64 / seconds, tx as f64 / seconds))
        });
        self.previous = Some((now, counters));
        result.map_or((None, None), |(rx, tx)| (Some(rx), Some(tx)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_elapsed_time_and_resets_on_missing_counters_or_interfaces() {
        let now = 100.0;
        let mut rate = NetworkRate::default();
        let counters = |n| Some(BTreeMap::from([("eth0".into(), (n, n * 2))]));
        assert_eq!(rate.sample(Some(now), counters(100)), (None, None));
        assert_eq!(
            rate.sample(Some(now + 2.0), counters(300)),
            (Some(100.0), Some(200.0))
        );
        assert_eq!(rate.sample(Some(now + 3.0), counters(10)), (None, None));
        assert_eq!(rate.sample(Some(now + 4.0), None), (None, None));
        assert_eq!(rate.sample(Some(now + 5.0), counters(30)), (None, None));
        assert_eq!(
            rate.sample(Some(now + 6.0), Some(BTreeMap::new())),
            (None, None)
        );
    }
    #[test]
    fn missing_or_non_increasing_server_time_never_uses_local_delivery_time() {
        let mut rate = NetworkRate::default();
        let counters = |n| Some(BTreeMap::from([("eth0".into(), (n, n))]));
        assert_eq!(rate.sample(Some(10.0), counters(10)), (None, None));
        assert_eq!(
            rate.sample(Some(11.0), counters(110)),
            (Some(100.0), Some(100.0))
        );
        assert_eq!(rate.sample(Some(11.0), counters(120)), (None, None));
        assert_eq!(rate.sample(Some(1.0), counters(130)), (None, None));
        assert_eq!(rate.sample(None, counters(140)), (None, None));
        assert_eq!(rate.sample(Some(3.0), counters(150)), (None, None));
        assert_eq!(rate.sample(Some(f64::NAN), counters(160)), (None, None));
    }
}
