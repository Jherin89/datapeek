//! Uniform samples without replacement.
//!
//! Algorithm R keeps N rows while streaming a file. The same seed produces
//! the same indices. Callers that materialize the sample sort those indices
//! so the profiled frame is in file order.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

pub fn rng(seed: Option<u64>) -> StdRng {
    match seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_entropy(),
    }
}

/// Indices of a uniform sample of `n` items from `0..population`.
///
/// The result is sorted. When `n` is zero or `n >= population`, every index
/// is returned (or none, when the population is empty).
pub fn sample_indices(population: usize, n: usize, seed: Option<u64>) -> Vec<usize> {
    if population == 0 || n == 0 {
        return Vec::new();
    }
    if n >= population {
        return (0..population).collect();
    }
    let mut rng = rng(seed);
    let mut chosen = Vec::with_capacity(n);
    for i in 0..population {
        if i < n {
            chosen.push(i);
        } else {
            let j = rng.gen_range(0..=i);
            if j < n {
                chosen[j] = i;
            }
        }
    }
    chosen.sort_unstable();
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_is_identical_and_sorted() {
        let a = sample_indices(10_000, 25, Some(1));
        let b = sample_indices(10_000, 25, Some(1));
        assert_eq!(a, b);
        assert!(a.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(a.len(), 25);
    }

    #[test]
    fn different_seeds_generally_differ() {
        let a = sample_indices(10_000, 25, Some(1));
        let b = sample_indices(10_000, 25, Some(2));
        assert_ne!(a, b);
    }

    #[test]
    fn n_at_least_population_returns_every_index() {
        assert_eq!(sample_indices(4, 4, Some(1)), vec![0, 1, 2, 3]);
        assert_eq!(sample_indices(4, 100, Some(9)), vec![0, 1, 2, 3]);
        assert!(sample_indices(0, 5, Some(1)).is_empty());
    }
}
