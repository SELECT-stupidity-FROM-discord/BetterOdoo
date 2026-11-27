use std::collections::{HashMap, HashSet};

pub struct Row {
    pub idx: usize,
    pub depth: usize,
    pub kids: usize,
    pub open: bool,
    pub forced: bool,
}

struct Tree<'a> {
    nodes: &'a [(i64, Option<i64>)],
    own: &'a [bool],
    expanded: &'a HashSet<i64>,
    kids: Vec<Vec<usize>>,
    has: Vec<bool>,
}

impl Tree<'_> {
    fn mark(&mut self, i: usize) -> bool {
        let mut has = self.own[i];
        for k in self.kids[i].clone() {
            has |= self.mark(k);
        }
        self.has[i] = has;
        has
    }

    fn emit(&self, i: usize, depth: usize, out: &mut Vec<Row>) {
        if !self.has[i] {
            return;
        }
        let shown: Vec<usize> = self.kids[i]
            .iter()
            .copied()
            .filter(|&k| self.has[k])
            .collect();
        let forced = !self.own[i];
        let open = !shown.is_empty() && (forced || self.expanded.contains(&self.nodes[i].0));
        out.push(Row {
            idx: i,
            depth,
            kids: shown.len(),
            open,
            forced,
        });
        if open {
            for k in shown {
                self.emit(k, depth + 1, out);
            }
        }
    }
}

pub fn rows(nodes: &[(i64, Option<i64>)], own: &[bool], expanded: &HashSet<i64>) -> Vec<Row> {
    let at: HashMap<i64, usize> = nodes.iter().enumerate().map(|(i, n)| (n.0, i)).collect();
    let mut kids = vec![Vec::new(); nodes.len()];
    let mut roots = Vec::new();
    for (i, &(_, parent)) in nodes.iter().enumerate() {
        match parent.and_then(|p| at.get(&p)) {
            Some(&p) if p != i => kids[p].push(i),
            _ => roots.push(i),
        }
    }
    let mut tree = Tree {
        nodes,
        own,
        expanded,
        kids,
        has: vec![false; nodes.len()],
    };
    for &r in &roots {
        tree.mark(r);
    }
    let mut out = Vec::new();
    for &r in &roots {
        tree.emit(r, 0, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(rows: &[Row]) -> Vec<(usize, usize, bool)> {
        rows.iter().map(|r| (r.idx, r.depth, r.open)).collect()
    }

    #[test]
    fn nests_and_collapses() {
        let nodes = [
            (1, None),
            (2, Some(1)),
            (3, Some(2)),
            (4, None),
            (5, Some(99)),
        ];
        let all = [true; 5];
        let none = HashSet::new();
        assert_eq!(
            shape(&rows(&nodes, &all, &none)),
            [(0, 0, false), (3, 0, false), (4, 0, false)]
        );
        let open: HashSet<i64> = [1, 2].into();
        assert_eq!(
            shape(&rows(&nodes, &all, &open)),
            [
                (0, 0, true),
                (1, 1, true),
                (2, 2, false),
                (3, 0, false),
                (4, 0, false)
            ]
        );
    }

    #[test]
    fn filtered_children_pull_in_their_parents() {
        let nodes = [(1, None), (2, Some(1)), (3, Some(1))];
        let own = [false, false, true];
        let r = rows(&nodes, &own, &HashSet::new());
        assert_eq!(shape(&r), [(0, 0, true), (2, 1, false)]);
        assert!(r[0].forced && r[0].kids == 1);
    }
}
