use core::sync::atomic::Ordering;
use crate::Config;
use crate::_Histograms;

#[cfg(feature = "serde-serialize")]
#[derive(Default, Serialize, Deserialize)]
pub struct Histogram {
	a: u8,
	b: u8,
	n: u8,
	index: Vec<usize>,
	count: Vec<u64>,
}

#[cfg(not(feature = "serde-serialize"))]
pub struct Histogram {
	a: u8,
	b: u8,
	n: u8,
	index: Vec<usize>,
	count: Vec<u64>,
}

impl _Histograms for Histogram {
	fn config(&self) -> Config {
		Config::new(self.a, self.b, self.n).unwrap()
	}

	fn total_count(&self) -> u128 {
		self.count.iter().map(|c| *c as u128).sum()
	}

	fn get_count(&self, index: usize) -> u64 {
		if let Ok(index) = self.index.binary_search(&index) {
			*self.count.get(index).unwrap_or(&0)
		} else {
			0
		}
	}
}

impl From<&crate::Histogram> for Histogram {
	fn from(other: &crate::Histogram) -> Self {
		let (a, b, n) = other.config().params();
		let mut index = Vec::new();
        let mut count = Vec::new();

        for (i, c) in other
            .buckets
            .iter()
            .enumerate()
            .filter(|(_i, c)| **c != 0)
        {
            index.push(i);
            count.push(*c);
        }

        Self {
            a,
            b,
            n,
            index,
            count,
        }
	}
}

impl From<&crate::atomic::Histogram> for Histogram {
	fn from(other: &crate::atomic::Histogram) -> Self {
		let (a, b, n) = other.config().params();
		let mut index = Vec::new();
        let mut count = Vec::new();

        for (i, c) in other
            .buckets
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .enumerate()
            .filter(|(_i, c)| *c != 0)
        {
            index.push(i);
            count.push(c);
        }

        Self {
            a,
            b,
            n,
            index,
            count,
        }
	}
}
