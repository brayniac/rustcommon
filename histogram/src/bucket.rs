#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Bucket {
    pub(crate) count: u64,
    pub(crate) lower: u64,
    pub(crate) upper: u64,
}

impl Bucket {
    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn range(&self) -> std::ops::RangeInclusive<u64> {
        std::ops::RangeInclusive::new(self.lower, self.upper)
    }

    pub fn lower(&self) -> u64 {
        self.lower
    }

    pub fn upper(&self) -> u64 {
        self.upper
    }
}
