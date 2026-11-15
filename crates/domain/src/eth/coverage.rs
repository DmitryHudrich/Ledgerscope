#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct BlockRange {
    from_block: u64,
    to_block: u64,
}

impl BlockRange {
    pub fn new(from_block: u64, to_block: u64) -> Self {
        Self {
            from_block: from_block.min(to_block),
            to_block: from_block.max(to_block),
        }
    }

    pub fn from_block(&self) -> u64 {
        self.from_block
    }

    pub fn to_block(&self) -> u64 {
        self.to_block
    }

    pub fn block_count(&self) -> u64 {
        (self.to_block - self.from_block).saturating_add(1)
    }

    pub fn contains(&self, block: u64) -> bool {
        (self.from_block..=self.to_block).contains(&block)
    }

    pub fn blocks(&self) -> std::ops::RangeInclusive<u64> {
        self.from_block..=self.to_block
    }
}

#[derive(Clone, Debug, Default)]
pub struct IndexCoverage {
    ranges: Vec<BlockRange>,
    tx_count: u64,
}

impl IndexCoverage {
    pub fn new(mut ranges: Vec<BlockRange>, tx_count: u64) -> Self {
        ranges.sort_unstable();

        let mut merged: Vec<BlockRange> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.from_block <= last.to_block.saturating_add(1) => {
                    last.to_block = last.to_block.max(range.to_block);
                }
                _ => merged.push(range),
            }
        }

        Self {
            ranges: merged,
            tx_count,
        }
    }

    pub fn from_blocks(blocks: &[u64], tx_count: u64) -> Self {
        let mut sorted: Vec<u64> = blocks.to_vec();
        sorted.sort_unstable();
        sorted.dedup();

        let mut ranges: Vec<BlockRange> = Vec::new();
        for block in sorted {
            match ranges.last_mut() {
                Some(last) if block == last.to_block + 1 => last.to_block = block,
                _ => ranges.push(BlockRange::new(block, block)),
            }
        }

        Self { ranges, tx_count }
    }

    pub fn ranges(&self) -> &[BlockRange] {
        &self.ranges
    }

    pub fn tx_count(&self) -> u64 {
        self.tx_count
    }

    pub fn block_count(&self) -> u64 {
        self.ranges.iter().map(BlockRange::block_count).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn lowest_block(&self) -> Option<u64> {
        self.ranges.first().map(BlockRange::from_block)
    }

    pub fn highest_block(&self) -> Option<u64> {
        self.ranges.last().map(BlockRange::to_block)
    }

    pub fn indexed_within(&self, wanted: BlockRange) -> u64 {
        self.ranges
            .iter()
            .filter_map(|range| overlap(range, &wanted))
            .map(|range| range.block_count())
            .sum()
    }

    pub fn gaps(&self, wanted: BlockRange) -> Vec<BlockRange> {
        let mut gaps = Vec::new();
        let mut cursor = wanted.from_block;

        for range in &self.ranges {
            if range.to_block < cursor {
                continue;
            }
            if range.from_block > wanted.to_block {
                break;
            }
            if range.from_block > cursor {
                gaps.push(BlockRange::new(cursor, range.from_block - 1));
            }
            if range.to_block >= wanted.to_block {
                return gaps;
            }
            cursor = range.to_block + 1;
        }

        if cursor <= wanted.to_block {
            gaps.push(BlockRange::new(cursor, wanted.to_block));
        }

        gaps
    }

    pub fn covers(&self, wanted: BlockRange) -> bool {
        self.gaps(wanted).is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BlockBucket {
    range: BlockRange,
    indexed_blocks: u64,
    tx_count: u64,
}

impl BlockBucket {
    pub fn new(range: BlockRange, indexed_blocks: u64, tx_count: u64) -> Self {
        Self {
            range,
            indexed_blocks,
            tx_count,
        }
    }

    pub fn range(&self) -> BlockRange {
        self.range
    }

    pub fn indexed_blocks(&self) -> u64 {
        self.indexed_blocks
    }

    pub fn tx_count(&self) -> u64 {
        self.tx_count
    }
}

fn overlap(left: &BlockRange, right: &BlockRange) -> Option<BlockRange> {
    let from = left.from_block.max(right.from_block);
    let to = left.to_block.min(right.to_block);

    (from <= to).then(|| BlockRange::new(from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(from: u64, to: u64) -> BlockRange {
        BlockRange::new(from, to)
    }

    #[test]
    fn touching_ranges_are_merged() {
        let coverage = IndexCoverage::new(vec![range(10, 12), range(13, 15), range(20, 21)], 0);

        assert_eq!(coverage.ranges(), [range(10, 15), range(20, 21)]);
        assert_eq!(coverage.block_count(), 8);
    }

    #[test]
    fn loose_blocks_become_runs() {
        let coverage = IndexCoverage::from_blocks(&[3, 1, 2, 7, 8, 5], 0);

        assert_eq!(coverage.ranges(), [range(1, 3), range(5, 5), range(7, 8)]);
        assert_eq!(coverage.lowest_block(), Some(1));
        assert_eq!(coverage.highest_block(), Some(8));
    }

    #[test]
    fn a_fully_indexed_span_leaves_no_gaps() {
        let coverage = IndexCoverage::new(vec![range(10, 20)], 0);

        assert!(coverage.covers(range(12, 18)));
        assert!(coverage.gaps(range(12, 18)).is_empty());
    }

    #[test]
    fn the_holes_of_a_span_are_reported() {
        let coverage = IndexCoverage::new(vec![range(10, 12), range(16, 18)], 0);

        assert_eq!(
            coverage.gaps(range(8, 20)),
            vec![range(8, 9), range(13, 15), range(19, 20)]
        );
        assert_eq!(coverage.indexed_within(range(8, 20)), 6);
    }

    #[test]
    fn an_unindexed_span_is_one_gap() {
        let coverage = IndexCoverage::default();

        assert_eq!(coverage.gaps(range(10, 20)), vec![range(10, 20)]);
        assert!(!coverage.covers(range(10, 20)));
        assert_eq!(coverage.indexed_within(range(10, 20)), 0);
    }

    #[test]
    fn ranges_outside_the_span_are_ignored() {
        let coverage = IndexCoverage::new(vec![range(1, 5), range(30, 40)], 0);

        assert_eq!(coverage.gaps(range(10, 20)), vec![range(10, 20)]);
    }
}
