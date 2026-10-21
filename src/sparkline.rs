/// 8-level unicode block sparkline used for numeric histograms.
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Number of bins in the compact histogram / sparkline.
pub const BINS: usize = 8;

/// Render a compact unicode sparkline from non-negative bin counts.
///
/// Zero-count bins still emit `▁` so the sparkline keeps a stable width.
pub fn sparkline(counts: &[u64]) -> String {
    if counts.is_empty() {
        return String::new();
    }
    let max = counts.iter().copied().max().unwrap_or(0);
    counts
        .iter()
        .map(|&c| {
            if max == 0 || c == 0 {
                BLOCKS[0]
            } else {
                let idx = ((c as f64 / max as f64) * (BLOCKS.len() - 1) as f64).round() as usize;
                BLOCKS[idx.min(BLOCKS.len() - 1)]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_empty() {
        assert_eq!(sparkline(&[]), "");
    }

    #[test]
    fn zeros_are_underscores() {
        assert_eq!(sparkline(&[0, 0, 0, 0]), "▁▁▁▁");
    }

    #[test]
    fn peak_is_full_block() {
        let s = sparkline(&[1, 2, 4, 8]);
        assert_eq!(s.chars().count(), 4);
        assert_eq!(s.chars().last().unwrap(), '█');
        assert_ne!(s.chars().next().unwrap(), '█');
    }

    #[test]
    fn equal_counts_are_full_blocks() {
        assert_eq!(sparkline(&[5, 5, 5]), "███");
    }
}
