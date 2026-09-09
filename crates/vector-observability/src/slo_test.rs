#[cfg(test)]
mod tests {
    use crate::metrics::SloTracker;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_slo_tracker_meets_and_fails_target() {
        // Fast task meeting SLO (100ms target)
        let tracker = SloTracker::start("fast_task", 100);
        thread::sleep(Duration::from_millis(5));
        let metric = tracker.finish();

        assert_eq!(metric.name, "fast_task");
        assert_eq!(metric.target_slo_ms, 100);
        assert!(metric.met, "Expected fast task to meet SLO");

        // Slow task violating SLO (5ms target)
        let tracker_slow = SloTracker::start("slow_task", 5);
        thread::sleep(Duration::from_millis(15));
        let metric_slow = tracker_slow.finish();

        assert_eq!(metric_slow.name, "slow_task");
        assert!(!metric_slow.met, "Expected slow task to violate SLO");
    }
}
