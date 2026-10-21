use serde::{Deserialize, Serialize};

pub const MIN_PROBABILITY: f64 = 0.02;
pub const MAX_PROBABILITY: f64 = 0.98;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub x: f64,
    pub y: f64,
    pub weight: f64,
}

impl Sample {
    pub const fn unit(x: f64, y: f64) -> Self {
        Self { x, y, weight: 1.0 }
    }

    fn usable(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.weight.is_finite() && self.weight > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IsotonicMap {
    points: Vec<Point>,
}

#[derive(Debug, Clone, Copy)]
struct Block {
    from: f64,
    value: f64,
    weight: f64,
}

impl Block {
    fn merged(self, next: Self) -> Self {
        let weight = self.weight + next.weight;
        Self {
            from: self.from,
            value: (self.value * self.weight + next.value * next.weight) / weight,
            weight,
        }
    }
}

fn ties_pooled(sorted: &[Sample]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    for sample in sorted {
        let next = Block {
            from: sample.x,
            value: sample.y,
            weight: sample.weight,
        };
        match blocks.last_mut() {
            Some(last) if last.from == sample.x => *last = last.merged(next),
            _ => blocks.push(next),
        }
    }
    blocks
}

impl IsotonicMap {
    pub fn fit(samples: &[Sample]) -> Option<Self> {
        let mut usable: Vec<Sample> = samples.iter().copied().filter(Sample::usable).collect();
        if usable.is_empty() {
            return None;
        }
        usable.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut stack: Vec<Block> = Vec::new();
        for block in ties_pooled(&usable) {
            let mut current = block;
            while let Some(previous) = stack.last().copied() {
                if previous.value < current.value {
                    break;
                }
                stack.pop();
                current = previous.merged(current);
            }
            stack.push(current);
        }
        Some(Self {
            points: stack
                .into_iter()
                .map(|block| Point {
                    x: block.from,
                    y: block.value,
                })
                .collect(),
        })
    }

    pub fn from_points(points: Vec<Point>) -> Option<Self> {
        let valid = !points.is_empty()
            && points
                .iter()
                .all(|point| point.x.is_finite() && point.y.is_finite())
            && points
                .windows(2)
                .all(|pair| pair[0].x < pair[1].x && pair[0].y <= pair[1].y);
        valid.then_some(Self { points })
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    pub fn predict(&self, x: f64) -> f64 {
        let index = self.points.partition_point(|point| point.x <= x);
        let value = self.points[index.saturating_sub(1)].y;
        value.clamp(MIN_PROBABILITY, MAX_PROBABILITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(pairs: &[(f64, f64)]) -> IsotonicMap {
        let samples: Vec<Sample> = pairs.iter().map(|(x, y)| Sample::unit(*x, *y)).collect();
        IsotonicMap::fit(&samples).unwrap()
    }

    fn values(map: &IsotonicMap) -> Vec<f64> {
        map.points().iter().map(|point| point.y).collect()
    }

    #[test]
    fn an_already_monotone_series_is_kept() {
        let map = fit(&[(0.1, 0.0), (0.2, 0.25), (0.3, 0.5), (0.4, 1.0)]);
        assert_eq!(values(&map), [0.0, 0.25, 0.5, 1.0]);
    }

    #[test]
    fn violators_are_pooled_into_their_mean() {
        let map = fit(&[(0.1, 1.0), (0.2, 0.0), (0.3, 1.0)]);
        assert_eq!(map.points().len(), 2);
        assert!((map.points()[0].y - 0.5).abs() < 1e-12);
        assert_eq!(map.points()[0].x, 0.1);
        assert_eq!(map.points()[1].x, 0.3);
        assert!((map.points()[1].y - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_output_is_monotone_whatever_the_order_of_the_input() {
        let pairs = [
            (0.9, 0.0),
            (0.1, 1.0),
            (0.5, 1.0),
            (0.7, 0.0),
            (0.3, 0.0),
            (0.2, 1.0),
            (0.8, 1.0),
            (0.6, 0.0),
            (0.4, 1.0),
        ];
        let map = fit(&pairs);
        let fitted = values(&map);
        assert!(fitted.windows(2).all(|pair| pair[0] <= pair[1]));
        let xs: Vec<f64> = map.points().iter().map(|point| point.x).collect();
        assert!(xs.windows(2).all(|pair| pair[0] < pair[1]));
        let predicted: Vec<f64> = (0..=100)
            .map(|step| map.predict(f64::from(step) / 100.0))
            .collect();
        assert!(predicted.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn samples_sharing_a_score_are_averaged_before_pooling() {
        let map = fit(&[(0.5, 1.0), (0.5, 0.0), (0.5, 0.0), (0.5, 0.0), (0.9, 1.0)]);
        assert_eq!(map.points().len(), 2);
        assert!((map.points()[0].y - 0.25).abs() < 1e-12);
        assert_eq!(map.points()[0].x, 0.5);
    }

    #[test]
    fn equal_neighbouring_values_are_merged() {
        let map = fit(&[(0.1, 0.0), (0.2, 0.0), (0.3, 1.0), (0.4, 1.0)]);
        assert_eq!(values(&map), [0.0, 1.0]);
    }

    #[test]
    fn weights_move_the_pooled_value() {
        let samples = [
            Sample {
                x: 0.1,
                y: 1.0,
                weight: 3.0,
            },
            Sample {
                x: 0.2,
                y: 0.0,
                weight: 1.0,
            },
        ];
        let map = IsotonicMap::fit(&samples).unwrap();
        assert_eq!(map.points().len(), 1);
        assert!((map.points()[0].y - 0.75).abs() < 1e-12);
    }

    #[test]
    fn unusable_samples_are_ignored() {
        let samples = [
            Sample::unit(f64::NAN, 1.0),
            Sample {
                x: 0.3,
                y: 1.0,
                weight: 0.0,
            },
            Sample::unit(0.4, 0.0),
        ];
        let map = IsotonicMap::fit(&samples).unwrap();
        assert_eq!(map.points().len(), 1);
        assert_eq!(map.points()[0].x, 0.4);
        assert!(IsotonicMap::fit(&[]).is_none());
        assert!(IsotonicMap::fit(&[Sample::unit(f64::INFINITY, 0.0)]).is_none());
    }

    #[test]
    fn a_single_class_gives_a_flat_map_clamped_to_the_bounds() {
        let ones = fit(&[(0.1, 1.0), (0.5, 1.0), (0.9, 1.0)]);
        assert_eq!(ones.points().len(), 1);
        assert_eq!(ones.predict(0.0), MAX_PROBABILITY);
        assert_eq!(ones.predict(1.0), MAX_PROBABILITY);
        let zeros = fit(&[(0.1, 0.0), (0.9, 0.0)]);
        assert_eq!(zeros.predict(0.5), MIN_PROBABILITY);
    }

    #[test]
    fn prediction_is_a_step_function_over_the_block_lower_bounds() {
        let map = fit(&[(0.2, 0.1), (0.4, 0.3), (0.6, 0.6), (0.8, 0.9)]);
        assert_eq!(map.predict(0.0), 0.1);
        assert_eq!(map.predict(0.2), 0.1);
        assert_eq!(map.predict(0.39), 0.1);
        assert_eq!(map.predict(0.4), 0.3);
        assert_eq!(map.predict(0.79), 0.6);
        assert_eq!(map.predict(0.8), 0.9);
        assert_eq!(map.predict(5.0), 0.9);
    }

    #[test]
    fn predictions_stay_within_the_probability_bounds() {
        let map = fit(&[(0.1, 0.0), (0.5, 0.5), (0.9, 1.0)]);
        assert_eq!(map.predict(0.1), MIN_PROBABILITY);
        assert_eq!(map.predict(0.5), 0.5);
        assert_eq!(map.predict(0.9), MAX_PROBABILITY);
        assert!(
            (0..=100)
                .map(|step| map.predict(f64::from(step) / 100.0))
                .all(|p| (MIN_PROBABILITY..=MAX_PROBABILITY).contains(&p))
        );
    }

    #[test]
    fn points_round_trip_through_json_and_are_validated() {
        let map = fit(&[(0.1, 0.0), (0.5, 0.5), (0.9, 1.0)]);
        let json = serde_json::to_value(&map).unwrap();
        assert!(json.is_array());
        let back: IsotonicMap = serde_json::from_value(json).unwrap();
        assert_eq!(back, map);
        assert!(IsotonicMap::from_points(map.points().to_vec()).is_some());
        assert!(IsotonicMap::from_points(Vec::new()).is_none());
        let decreasing = vec![Point { x: 0.1, y: 0.9 }, Point { x: 0.2, y: 0.1 }];
        assert!(IsotonicMap::from_points(decreasing).is_none());
        let unordered = vec![Point { x: 0.2, y: 0.1 }, Point { x: 0.1, y: 0.2 }];
        assert!(IsotonicMap::from_points(unordered).is_none());
    }
}
