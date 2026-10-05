/// Rowspan occupancy with logarithmic free-column and row-width queries.
pub(super) struct ColumnReservations {
    limit: usize,
    nodes: Vec<(usize, usize)>,
}

impl ColumnReservations {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            nodes: Vec::new(),
        }
    }

    pub(super) fn hold(&mut self, column: usize, until: usize) {
        if self.nodes.is_empty() {
            self.nodes.resize(4 * self.limit, (0, 0));
        }
        self.set(1, 0, self.limit, column, until);
    }

    pub(super) fn next_free(&self, from: usize, row: usize) -> usize {
        if self.nodes.is_empty() || self.nodes[1].1 <= row || from >= self.limit {
            return from;
        }
        self.find_free(1, 0, self.limit, from, row)
    }

    pub(super) fn reach(&self, row: usize) -> usize {
        if self.nodes.is_empty() || self.nodes[1].1 <= row {
            return 0;
        }
        let (mut node, mut left, mut right) = (1, 0, self.limit);
        while right - left > 1 {
            let middle = (left + right) / 2;
            if self.nodes[2 * node + 1].1 > row {
                node = 2 * node + 1;
                left = middle;
            } else {
                node *= 2;
                right = middle;
            }
        }
        right
    }

    fn set(&mut self, node: usize, left: usize, right: usize, column: usize, until: usize) {
        if right - left == 1 {
            let end = self.nodes[node].1.max(until);
            self.nodes[node] = (end, end);
            return;
        }
        let middle = (left + right) / 2;
        if column < middle {
            self.set(2 * node, left, middle, column, until);
        } else {
            self.set(2 * node + 1, middle, right, column, until);
        }
        self.nodes[node] = (
            self.nodes[2 * node].0.min(self.nodes[2 * node + 1].0),
            self.nodes[2 * node].1.max(self.nodes[2 * node + 1].1),
        );
    }

    fn find_free(&self, node: usize, left: usize, right: usize, from: usize, row: usize) -> usize {
        if right <= from || self.nodes[node].0 > row {
            return self.limit;
        }
        if self.nodes[node].1 <= row {
            return left.max(from);
        }
        let middle = (left + right) / 2;
        let found = self.find_free(2 * node, left, middle, from, row);
        if found < self.limit {
            found
        } else {
            self.find_free(2 * node + 1, middle, right, from, row)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ColumnReservations;

    #[test]
    fn sparse_holds_overlapping_extensions_and_expiry_match_column_scans() {
        let mut index = ColumnReservations::new(64);
        let mut held = [0usize; 64];
        assert_eq!(index.next_free(0, 0), 0);
        assert_eq!(index.reach(0), 0);
        for row in 0..20 {
            if row < 12 {
                for j in 0..16 {
                    let col = (row * 17 + j * 13) % 64;
                    let end = row + 2 + j % 7;
                    held[col] = held[col].max(end);
                    index.hold(col, end);
                }
            }
            let reach = held
                .iter()
                .rposition(|end| *end > row)
                .map_or(0, |col| col + 1);
            assert_eq!(index.reach(row), reach);
            for from in 0..=65 {
                let mut expected = from;
                while held.get(expected).copied().unwrap_or(0) > row {
                    expected += 1;
                }
                assert_eq!(index.next_free(from, row), expected);
            }
        }
    }
}
