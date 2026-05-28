//! Behavioural spec for the bounded rolling window (Phase 4, test-first).

use prometheon_netmodel::window::RollingWindow;

#[test]
fn empty_window_has_no_stats() {
    let w = RollingWindow::new(4);
    assert!(w.is_empty());
    assert_eq!(w.len(), 0);
    assert_eq!(w.mean(), None);
    assert_eq!(w.variance(), None);
    assert_eq!(w.latest(), None);
}

#[test]
fn mean_and_latest_track_pushed_values() {
    let mut w = RollingWindow::new(4);
    w.push(10.0);
    w.push(20.0);
    w.push(30.0);
    assert_eq!(w.len(), 3);
    assert_eq!(w.mean(), Some(20.0));
    assert_eq!(w.latest(), Some(30.0));
}

#[test]
fn capacity_evicts_oldest() {
    let mut w = RollingWindow::new(3);
    for v in [1.0, 2.0, 3.0, 4.0] {
        w.push(v);
    }
    // 1.0 evicted; window holds [2,3,4].
    assert_eq!(w.len(), 3);
    assert_eq!(w.mean(), Some(3.0));
    assert_eq!(w.latest(), Some(4.0));
}

#[test]
fn variance_is_population_variance() {
    let mut w = RollingWindow::new(8);
    for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
        w.push(v);
    }
    // Classic dataset: mean 5, population variance 4.
    assert_eq!(w.mean(), Some(5.0));
    assert_eq!(w.variance(), Some(4.0));
}

#[test]
fn single_value_has_zero_variance() {
    let mut w = RollingWindow::new(4);
    w.push(42.0);
    assert_eq!(w.variance(), Some(0.0));
}

#[test]
fn capacity_is_clamped_to_at_least_one() {
    let mut w = RollingWindow::new(0);
    w.push(1.0);
    w.push(2.0);
    assert_eq!(w.len(), 1);
    assert_eq!(w.latest(), Some(2.0));
}
