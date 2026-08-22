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
