//! A deterministic project shaped like a real large one: a wide disc of
//! unlinked notes, a few dozen small linked clusters and one dense hub, capped
//! at the 3 000 files the graph shows.
#![allow(dead_code)]

/// What the generator produces.
#[derive(Clone, Copy, Debug)]
pub struct BigProject {
    /// Files in total (the view's cap).
    pub total: usize,
    /// Linked clusters of three to ten files (stars, rings, chains, cliques).
    pub clusters: usize,
    /// Files of the hub, each linking to the next `hub_reach` ones.
    pub hub: usize,
    pub hub_reach: usize,
    pub seed: u64,
}

impl Default for BigProject {
    fn default() -> Self {
        BigProject {
            total: 3_000,
            clusters: 80,
            hub: 50,
            hub_reach: 24,
            seed: 20_261_023,
        }
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() as usize) % (hi - lo + 1)
    }
}

fn note(path: &str, links: &[String]) -> (String, String) {
    let mut text = String::from("# note\n");
    for link in links {
        text.push_str(&format!("[[{link}]] "));
    }
    text.push('\n');
    (path.to_string(), text)
}

impl BigProject {
    /// Relative path and text of every file.
    pub fn files(&self) -> Vec<(String, String)> {
        let mut rng = Lcg(self.seed);
        let mut out: Vec<(String, String)> = Vec::new();
        let name = |dir: &str, i: usize| format!("{dir}-{i:04}");
        // the hub
        for i in 0..self.hub {
            let links: Vec<String> = (1..=self.hub_reach)
                .map(|k| name("hub", (i + k) % self.hub))
                .collect();
            out.push(note(&format!("hub/{}.md", name("hub", i)), &links));
        }
        // clusters
        for c in 0..self.clusters {
            let size = rng.range(3, 10);
            let id = |i: usize| name(&format!("cl{c:02}"), i);
            let links = |i: usize| -> Vec<String> {
                match c % 4 {
                    // star around file 0
                    0 => {
                        if i == 0 {
                            (1..size).map(id).collect()
                        } else {
                            Vec::new()
                        }
                    }
                    // ring
                    1 => vec![id((i + 1) % size)],
                    // chain
                    2 => {
                        if i + 1 < size {
                            vec![id(i + 1)]
                        } else {
                            Vec::new()
                        }
                    }
                    // clique
                    _ => (i + 1..size).map(id).collect(),
                }
            };
            for i in 0..size {
                out.push(note(&format!("cluster/{c:02}/{}.md", id(i)), &links(i)));
            }
        }
        // the unlinked disc fills the rest up to the cap
        let rest = self.total.saturating_sub(out.len());
        for i in 0..rest {
            out.push(note(
                &format!("loose/{:02}/{}.md", i % 40, name("loose", i)),
                &[],
            ));
        }
        out
    }

    /// Writes the files below `root`.
    pub fn write(&self, root: &std::path::Path) {
        for (rel, text) in self.files() {
            super::write(root, &rel, &text);
        }
    }
}
