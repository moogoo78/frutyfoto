/// Groups ids whose perceptual hashes are within `threshold` bits of each other.
/// Pairwise comparison with union-find; only groups of 2+ are returned.
pub fn group(items: &[(i64, u64)], threshold: u32) -> Vec<Vec<i64>> {
    let n = items.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if (items[i].1 ^ items[j].1).count_ones() <= threshold {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }

    let mut groups: std::collections::HashMap<usize, Vec<i64>> = Default::default();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(items[i].0);
    }
    let mut out: Vec<Vec<i64>> = groups.into_values().filter(|g| g.len() > 1).collect();
    out.sort_by_key(|g| g[0]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_close_hashes() {
        let items = [(1, 0b0000), (2, 0b0011), (3, u64::MAX), (4, 0b0111)];
        assert_eq!(group(&items, 2), vec![vec![1, 2, 4]]);
        assert!(group(&items, 0).is_empty());
    }
}
