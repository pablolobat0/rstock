use chrono::NaiveDate;

pub trait Clock: Send + Sync {
    fn today(&self) -> NaiveDate;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        chrono::Local::now().date_naive()
    }
}

#[allow(dead_code)] // Constructed by external callers and integration tests through the library crate.
pub struct FixedClock {
    today: NaiveDate,
}

#[allow(dead_code)]
impl FixedClock {
    pub fn new(today: NaiveDate) -> Self {
        Self { today }
    }
}

impl Clock for FixedClock {
    fn today(&self) -> NaiveDate {
        self.today
    }
}
