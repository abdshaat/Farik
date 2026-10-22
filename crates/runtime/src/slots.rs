//! What several teams share in one `catervas serve` (`docs/SPEC.md` 5.5; ADR 0059): the places for
//! agent sessions, given to the teams in turn; the AI account's sleep, which every team waits out;
//! and the computer's daily cap, which all the teams' spending counts toward. Held in memory by
//! one serve; `catervas run` and `catervas start` have none.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use catervas_protocol::clock::Clock;
use chrono::{DateTime, NaiveDate, Utc};
use tokio::sync::oneshot;

use crate::sleep::{Sleeper, TokioSleeper};

/// The system's own clock, for a serve that was given none.
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// What the places and the limits are, behind one lock.
#[derive(Default)]
struct Inner {
    limit: Option<usize>,
    in_use: usize,
    /// Each team's waiting takers, first in first out.
    queues: BTreeMap<String, VecDeque<oneshot::Sender<()>>>,
    /// The teams in the order they first asked, and the one served last.
    order: Vec<String>,
    last_served: Option<usize>,
    asleep_until: Option<DateTime<Utc>>,
    daily_cap_usd: Option<f64>,
    /// Each team's spend and the UTC day it is of.
    spent: BTreeMap<String, (NaiveDate, f64)>,
}

fn locked(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Inner {
    /// Gives free places to the waiting teams, one in turn each, until none is free or none waits.
    fn dispatch(&mut self) {
        while self.limit.is_none_or(|limit| self.in_use < limit) {
            let Some(index) = self.next_waiting() else {
                return;
            };
            self.last_served = Some(index);
            let team = self.order[index].clone();
            let Some(queue) = self.queues.get_mut(&team) else {
                return;
            };
            let Some(taker) = queue.pop_front() else {
                continue;
            };
            self.in_use += 1;
            if taker.send(()).is_err() {
                // The taker gave up while it waited: the place is free again.
                self.in_use -= 1;
            }
        }
    }

    /// The first team after the one served last, in the order they first asked, with a taker waiting.
    fn next_waiting(&self) -> Option<usize> {
        let count = self.order.len();
        let start = self.last_served.map_or(0, |last| last + 1);
        (0..count)
            .map(|step| (start + step) % count)
            .find(|&index| {
                self.queues
                    .get(&self.order[index])
                    .is_some_and(|queue| !queue.is_empty())
            })
    }

    fn know(&mut self, team: &str) -> usize {
        if let Some(index) = self.order.iter().position(|known| known == team) {
            return index;
        }
        self.order.push(team.to_string());
        self.order.len() - 1
    }
}

/// The places, the account's sleep and the computer's day, shared by every team of one serve.
pub struct Slots {
    inner: Arc<Mutex<Inner>>,
    clock: Arc<dyn Clock + Send + Sync>,
    sleeper: Arc<dyn Sleeper>,
}

/// A place for one agent session, given back when it is dropped.
pub struct Place {
    inner: Arc<Mutex<Inner>>,
}

impl Drop for Place {
    fn drop(&mut self) {
        let mut inner = locked(&self.inner);
        inner.in_use = inner.in_use.saturating_sub(1);
        inner.dispatch();
    }
}

/// A taker waiting in a queue; if it is dropped before its place comes, the place that may have
/// come is given back.
struct Waiting {
    inner: Arc<Mutex<Inner>>,
    receiver: oneshot::Receiver<()>,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.receiver.close();
        if self.receiver.try_recv().is_ok() {
            drop(Place {
                inner: Arc::clone(&self.inner),
            });
        }
    }
}

impl Slots {
    /// Places for `limit` sessions at once (none: no limit) and a daily cap of `daily_cap_usd` for
    /// all the teams together (none: no cap), on the system's clock.
    #[must_use]
    pub fn new(limit: Option<usize>, daily_cap_usd: Option<f64>) -> Arc<Slots> {
        let clock: Arc<dyn Clock + Send + Sync> = Arc::new(SystemClock);
        Slots::with_clock(limit, daily_cap_usd, clock)
    }

    /// `new`, waiting out the account's sleep on `clock`.
    #[must_use]
    pub fn with_clock(
        limit: Option<usize>,
        daily_cap_usd: Option<f64>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Arc<Slots> {
        let sleeper: Arc<dyn Sleeper> = Arc::new(TokioSleeper {
            clock: Arc::clone(&clock),
        });
        Slots::with_sleeper(limit, daily_cap_usd, clock, sleeper)
    }

    /// `with_clock`, waiting on `sleeper`, which a test moves by hand.
    #[must_use]
    pub fn with_sleeper(
        limit: Option<usize>,
        daily_cap_usd: Option<f64>,
        clock: Arc<dyn Clock + Send + Sync>,
        sleeper: Arc<dyn Sleeper>,
    ) -> Arc<Slots> {
        Arc::new(Slots {
            inner: Arc::new(Mutex::new(Inner {
                limit,
                daily_cap_usd,
                ..Inner::default()
            })),
            clock,
            sleeper,
        })
    }

    /// Waits until the AI account's limit has reset, then until a place is free for `team`.
    pub async fn take(&self, team: &str) -> Place {
        while let Some(until) = self.asleep_until() {
            if until <= self.clock.now() {
                break;
            }
            self.sleeper.sleep_until(until).await;
        }
        let (sender, receiver) = oneshot::channel();
        {
            let mut inner = locked(&self.inner);
            inner.know(team);
            inner
                .queues
                .entry(team.to_string())
                .or_default()
                .push_back(sender);
            inner.dispatch();
        }
        let waiting = Waiting {
            inner: Arc::clone(&self.inner),
            receiver,
        };
        let mut waiting = waiting;
        // A closed channel is only possible after the slots are dropped, with them the places.
        let _ = (&mut waiting.receiver).await;
        Place {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Changes how many sessions run at once; running sessions are never stopped.
    pub fn set_limit(&self, limit: Option<usize>) {
        let mut inner = locked(&self.inner);
        inner.limit = limit;
        inner.dispatch();
    }

    /// Changes the computer's daily cap.
    pub fn set_daily_cap(&self, cap: Option<f64>) {
        locked(&self.inner).daily_cap_usd = cap;
    }

    /// How many sessions run at once at most, when that is limited.
    #[must_use]
    pub fn limit(&self) -> Option<usize> {
        locked(&self.inner).limit
    }

    /// The computer's daily cap, when there is one.
    #[must_use]
    pub fn daily_cap(&self) -> Option<f64> {
        locked(&self.inner).daily_cap_usd
    }

    /// Every team waits until `until` for the AI account's limit to reset; an earlier time than the
    /// one held is ignored.
    pub fn account_asleep_until(&self, until: DateTime<Utc>) {
        let mut inner = locked(&self.inner);
        inner.asleep_until = Some(inner.asleep_until.map_or(until, |held| held.max(until)));
    }

    /// When the account's limit resets, while it is asleep.
    #[must_use]
    pub fn asleep_until(&self) -> Option<DateTime<Utc>> {
        locked(&self.inner)
            .asleep_until
            .filter(|until| *until > self.clock.now())
    }

    /// Adds `usd`, spent by `team` at `at`, to its spend of that UTC day.
    pub fn record_spend(&self, team: &str, usd: f64, at: DateTime<Utc>) {
        let day = at.date_naive();
        let mut inner = locked(&self.inner);
        let entry = inner.spent.entry(team.to_string()).or_insert((day, 0.0));
        if entry.0 != day {
            *entry = (day, 0.0);
        }
        entry.1 += usd;
    }

    /// What the teams other than `team` spent on `today`.
    #[must_use]
    pub fn spent_elsewhere(&self, team: &str, today: NaiveDate) -> f64 {
        locked(&self.inner)
            .spent
            .iter()
            .filter(|(name, (day, _))| name.as_str() != team && *day == today)
            .map(|(_, (_, usd))| usd)
            .sum()
    }

    /// What every team together spent on `today`.
    #[must_use]
    pub fn spent_today(&self, today: NaiveDate) -> f64 {
        locked(&self.inner)
            .spent
            .values()
            .filter(|(day, _)| *day == today)
            .map(|(_, usd)| usd)
            .sum()
    }

    /// What every team together spent on the UTC day it is now.
    #[must_use]
    pub fn spent_today_now(&self) -> f64 {
        self.spent_today(self.clock.now().date_naive())
    }

    /// Whether the teams together reached the computer's daily cap on `today`.
    #[must_use]
    pub fn day_capped(&self, today: NaiveDate) -> bool {
        let cap = locked(&self.inner).daily_cap_usd;
        cap.is_some_and(|cap| self.spent_today(today) >= cap)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use catervas_protocol::clock::Clock;
    use chrono::{DateTime, TimeZone, Utc};

    use super::Slots;

    fn t() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 22, 9, 0, 0).unwrap()
    }

    /// A clock that moves with tokio's paused time.
    struct PausedClock {
        base: DateTime<Utc>,
        start: tokio::time::Instant,
    }

    impl Clock for PausedClock {
        fn now(&self) -> DateTime<Utc> {
            self.base + chrono::Duration::from_std(self.start.elapsed()).unwrap()
        }
    }

    /// Whether `take` is given within a moment of paused time.
    async fn given(slots: &Slots, team: &str) -> Option<super::Place> {
        tokio::time::timeout(Duration::from_millis(10), slots.take(team))
            .await
            .ok()
    }

    #[tokio::test(start_paused = true)]
    async fn gives_places_in_turn_across_teams() {
        let slots = Slots::new(Some(1), None);
        let first = slots.take("a").await;
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut waiting = Vec::new();
        for (name, tag) in [("a", "a2"), ("a", "a3"), ("b", "b1")] {
            let (slots, order) = (Arc::clone(&slots), Arc::clone(&order));
            waiting.push(tokio::spawn(async move {
                let place = slots.take(name).await;
                order.lock().unwrap().push(tag);
                tokio::time::sleep(Duration::from_millis(5)).await;
                drop(place);
            }));
            tokio::task::yield_now().await;
        }
        assert!(order.lock().unwrap().is_empty(), "the place is held");
        drop(first);
        for handle in waiting {
            handle.await.unwrap();
        }
        // b's turn comes before a's second, though a asked first.
        assert_eq!(*order.lock().unwrap(), vec!["b1", "a2", "a3"]);

        let free = Slots::new(None, None);
        let mut places = Vec::new();
        for _ in 0..10 {
            places.push(given(&free, "a").await.expect("given at once"));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_account_and_the_computer_s_day() {
        let clock: Arc<dyn Clock + Send + Sync> = Arc::new(PausedClock {
            base: t(),
            start: tokio::time::Instant::now(),
        });
        let slots = Slots::with_clock(None, Some(10.0), clock);
        slots.account_asleep_until(t() + chrono::Duration::hours(1));
        slots.account_asleep_until(t() + chrono::Duration::minutes(5));
        assert_eq!(slots.asleep_until(), Some(t() + chrono::Duration::hours(1)));
        let started = tokio::time::Instant::now();
        let place = slots.take("a").await;
        assert!(
            started.elapsed() >= Duration::from_secs(3600),
            "{:?}",
            started.elapsed()
        );
        drop(place);
        assert_eq!(slots.asleep_until(), None);

        let today = t().date_naive();
        slots.record_spend("a", 6.0, t());
        assert!(!slots.day_capped(today));
        slots.record_spend("b", 4.0, t());
        assert!(slots.day_capped(today));
        assert!((slots.spent_elsewhere("a", today) - 4.0).abs() < 1e-9);
        assert!(!slots.day_capped(today.succ_opt().unwrap()));
    }
}
