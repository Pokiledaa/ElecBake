/// Groups items that are connected. The representative of a group is its smallest
/// index, which keeps results in a stable order.
pub struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    pub fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
        }
    }

    pub fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    pub fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[a.max(b)] = a.min(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joined_items_share_the_smallest_representative() {
        let mut groups = UnionFind::new(5);
        groups.union(3, 4);
        groups.union(1, 3);
        assert_eq!(groups.find(4), 1);
        assert_eq!(groups.find(0), 0);
        assert_eq!(groups.find(2), 2);
    }
}
