use std::num::NonZeroU32;

use math::{Dimension, Mat, Vector, X, Y, Z};

use crate::point::Point;

pub const MAX_NEIGHBORS: usize = 64 - 1;

pub struct SegmentInformation {
	pub trunk_height: common::Value,
	pub crown_height: common::Value,
}

pub fn calculate(data: Vec<Vector<3, f32>>, segment: NonZeroU32) -> (Vec<Point>, SegmentInformation) {
	let neighbors_tree = NeighborsTree::new(&data);

	let (min, max) = {
		let mut min = data[0][Y];
		let mut max = data[0][Y];
		for p in data.iter().skip(1) {
			if p[Y] < min {
				min = p[Y];
			} else if p[Y] > max {
				max = p[Y];
			}
		}
		(min, max)
	};
	let height = max - min;

	let (slices, slice_width, trunk_crown_sep) = {
		let slice_width = 0.05;

		let slices = ((height / slice_width).ceil() as usize) + 1;
		let mut means = vec![(Vector::new([0.0, 0.0]), 0); slices];
		for pos in data.iter().copied() {
			let idx = ((pos[Y] - min) / slice_width) as usize;
			means[idx].0 += [pos[X], pos[Z]].into();
			means[idx].1 += 1;
		}
		for mean in means.iter_mut() {
			mean.0 /= mean.1 as f32;
		}
		let mut variance = vec![0.0f32; slices];
		for pos in data.iter().copied() {
			let idx = ((pos[Y] - min) / slice_width) as usize;
			variance[idx] += (means[idx].0 - [pos[X], pos[Z]].into()).length_squared();
		}
		let mut max_var = 0.0;
		for i in 0..variance.len() {
			variance[i] /= (means[i].1 as f32).sqrt();
			if variance[i] > max_var {
				max_var = variance[i];
			}
		}
		let mut mapped = vec![0; slices];
		for i in 0..variance.len() {
			let percent = variance[i] / max_var;
			mapped[i] = map_to_u32(percent);
		}

		let sep = mapped
			.iter()
			.enumerate()
			.find(|&(_, &v)| v > u32::MAX / 3)
			.map(|(index, _)| index)
			.unwrap_or(0);

		(mapped, slice_width, min + slice_width * sep as f32)
	};

	let res = (0..data.len())
		.map(|i| {
			let mut neighbors = [(0.0, 0); MAX_NEIGHBORS];
			let neighbors = neighbors_tree.get(data[i], &mut neighbors);

			let mean = {
				let mut mean = Vector::<3, f32>::new([0.0, 0.0, 0.0]);
				for (_, neighbor) in neighbors {
					mean += data[*neighbor];
				}
				mean / neighbors.len() as f32
			};
			let variance = {
				let mut variance = Mat::<3, f32>::default();
				for (_, neigbhor) in neighbors {
					let difference = data[*neigbhor] - mean;
					for x in X.to(Z) {
						for y in X.to(Z) {
							variance[x + y] += difference[x] * difference[y];
						}
					}
				}
				for x in X.to(Z) {
					for y in X.to(Z) {
						variance[x + y] /= neighbors.len() as f32;
					}
				}
				variance
			};

			let eigen_values = variance.fast_eigenvalues();
			let eigen_vectors = variance.calculate_eigenvectors(eigen_values);

			let size = neighbors[1..]
				.iter()
				.copied()
				.map(|(dist, _)| dist)
				.sum::<f32>();
			let size = (1.0 / 3.0) * size / (neighbors.len() - 1) as f32;

			Point {
				render: render::Point {
					position: data[i],
					normal: eigen_vectors[Z],
					size,
				},
				segment,
				slice: slices[((data[i][Y] - min) / slice_width) as usize],
				sub_index: ((data[i][Y] - min) / (max - min) * u32::MAX as f32) as u32,
				curve: map_to_u32((3.0 * eigen_values[Z]) / (eigen_values[X] + eigen_values[Y] + eigen_values[Z])),
			}
		})
		.collect::<Vec<Point>>();

	let trunk_heigth = trunk_crown_sep - min;
	let crown_heigth = max - trunk_crown_sep;
	(
		res,
		SegmentInformation {
			trunk_height: common::Value::RelativeHeight {
				absolute: trunk_heigth,
				percent: trunk_heigth / height,
			},
			crown_height: common::Value::RelativeHeight {
				absolute: crown_heigth,
				percent: crown_heigth / height,
			},
		},
	)
}

struct Adapter;
impl k_nearest::Adapter<3, f32, Vector<3, f32>> for Adapter {
	fn get(point: &Vector<3, f32>, dimension: Dimension) -> f32 {
		point[dimension]
	}
	fn get_all(point: &Vector<3, f32>) -> [f32; 3] {
		point.data()
	}
}

//todo: check if precalculated is better
// pub struct Neighbors(Vec<(usize, [(f32, usize); MAX_NEIGHBORS])>);
pub struct NeighborsTree(k_nearest::KDTree<3, f32, Vector<3, f32>, Adapter, k_nearest::EuclideanDistanceSquared>);

impl NeighborsTree {
	pub fn new(points: &[Vector<3, f32>]) -> Self {
		Self(k_nearest::KDTree::new(points))
	}

	pub fn get<'a>(&self, point: Vector<3, f32>, data: &'a mut [(f32, usize)]) -> &'a [(f32, usize)] {
		let l = self.0.k_nearest(&point, data, 1000.0);
		&data[0..l]
	}
}

pub fn map_to_u32(value: f32) -> u32 {
	(value * u32::MAX as f32) as u32
}
