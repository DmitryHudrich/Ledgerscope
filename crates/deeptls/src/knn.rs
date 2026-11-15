pub struct Knn<L> {
    k: usize,
    mean: Vec<f64>,
    scale: Vec<f64>,
    samples: Vec<(Vec<f64>, L)>,
}

impl<L: Copy + PartialEq> Knn<L> {
    pub fn fit(train: &[(Vec<f64>, L)], k: usize) -> Self {
        let n = train.len() as f64;
        let dims = train.first().map_or(0, |(x, _)| x.len());
        let column = |d: usize| train.iter().map(move |(x, _)| x[d]);
        let mean: Vec<f64> = (0..dims).map(|d| column(d).sum::<f64>() / n).collect();
        let scale = (0..dims)
            .map(|d| (column(d).map(|v| (v - mean[d]).powi(2)).sum::<f64>() / n).sqrt().max(1e-9))
            .collect();

        let mut model = Self { k: k.clamp(1, train.len()), mean, scale, samples: Vec::new() };
        model.samples = train.iter().map(|(x, label)| (model.normalize(x), *label)).collect();
        model
    }

    pub fn predict(&self, x: &[f64]) -> L {
        self.predict_scored(x).0
    }

    pub fn predict_scored(&self, x: &[f64]) -> (L, f64) {
        let x = self.normalize(x);
        let mut nearest: Vec<(f64, L)> = self
            .samples
            .iter()
            .map(|(s, label)| (s.iter().zip(&x).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt(), *label))
            .collect();
        nearest.select_nth_unstable_by(self.k - 1, |a, b| a.0.total_cmp(&b.0));

        let mut votes: Vec<(L, f64)> = Vec::new();
        let mut total = 0.0;
        for &(distance, label) in &nearest[..self.k] {
            let weight = 1.0 / (distance + 1e-9);
            total += weight;
            match votes.iter_mut().find(|(l, _)| *l == label) {
                Some(vote) => vote.1 += weight,
                None => votes.push((label, weight)),
            }
        }
        let (label, weight) = votes
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("k >= 1");
        (label, weight / total)
    }

    fn normalize(&self, x: &[f64]) -> Vec<f64> {
        x.iter().zip(&self.mean).zip(&self.scale).map(|((v, m), s)| (v - m) / s).collect()
    }
}
