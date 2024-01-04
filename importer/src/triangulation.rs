use std::{
	collections::{ HashMap, HashSet },
	ops::Not,
};

use math::{ Vector, X, Y, Z };

use crate::{ calculations::Adapter, Settings };


type Tree = k_nearest::KDTree<3, f32, Vector<3, f32>, Adapter, k_nearest::EuclideanDistanceSquared>;


pub fn triangulate(data: &[Vector<3, f32>], tree: Tree, settings: &Settings) -> Vec<u32> {
	let mut used = vec![false; data.len()];

	//assume one connected segmnent
	let Some((seed, center)) = seed(data, &used, &tree, settings.alpha) else {
		return Vec::new();
	};
	let mut indices = vec![seed[X] as u32, seed[Y] as u32, seed[Z] as u32];
	used[seed[X]] = true;
	used[seed[Y]] = true;
	used[seed[Z]] = true;
	let mut found = HashSet::new();

	let mut edges = [
		(Edge::new(seed[X], seed[Z], center), seed[Y]),
		(Edge::new(seed[Z], seed[Y], center), seed[X]),
		(Edge::new(seed[Y], seed[X], center), seed[Z]),
	]
		.into_iter()
		.collect::<HashMap<_, _>>();

	while let Some(edge) = edges.keys().next().copied() {
		let old = edges.remove(&edge).unwrap();
		found.insert(edge);
		let (first, second, center) = (edge.active_1, edge.active_2, edge.center);
		if let Some((third, center)) = find_third(data, first, second, &tree, old, settings.alpha, center) {
			indices.push(first as u32);
			indices.push(second as u32);
			indices.push(third as u32);
			used[third] = true;
			for edge in [
				(Edge::new(first, third, center), second),
				(Edge::new(third, second, center), first),
			] {
				if let std::collections::hash_map::Entry::Vacant(e) = edges.entry(edge.0) {
					if found.contains(&edge.0).not() {
						e.insert(edge.1);
					}
				} else {
					edges.remove(&edge.0);
					found.insert(edge.0);
				}
			}
		}
	}
	indices
}


#[derive(Clone, Copy)]
struct Edge {
	active_1: usize,
	active_2: usize,
	center: Vector<3, f32>,
}


impl Edge {
	fn new(active_1: usize, active_2: usize, center: Vector<3, f32>) -> Self {
		Self { active_1, active_2, center }
	}
}


impl std::cmp::Eq for Edge { }


impl std::cmp::PartialEq for Edge {
	fn eq(&self, other: &Self) -> bool {
		self.active_1 == other.active_1 && self.active_2 == other.active_2
			|| self.active_1 == other.active_2 && self.active_2 == other.active_1
	}
}


impl std::hash::Hash for Edge {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		if self.active_1 < self.active_2 {
			self.active_1.hash(state);
			self.active_2.hash(state);
		} else {
			self.active_2.hash(state);
			self.active_1.hash(state);
		}
	}
}


fn seed(data: &[Vector<3, f32>], used: &[bool], tree: &Tree, alpha: f32) -> Option<(Vector<3, usize>, Vector<3, f32>)> {
	for (first, point) in data.iter().enumerate().filter(|&(idx, _)| used[idx].not()) {
		let nearest = tree.nearest(point, (2.0 * alpha).powi(2));
		if nearest.len() <= 2 {
			continue;
		}
		for (second_index, second) in nearest
			.iter()
			.enumerate()
			.skip(1)
			.filter(|(_, entry)| used[entry.index].not()) {
			for third in nearest
				.iter()
				.skip(second_index + 1)
				.filter(|entry| used[entry.index].not()) {
				let Some(center) = sphere_location(
					data[first],
					data[second.index],
					data[third.index],
					alpha,
				) else {
					continue;
				};

				if tree.empty(&center, (alpha - 0.001).powi(2)) {
					return Some(([first, second.index, third.index].into(), center));
				}

				let Some(center) = sphere_location(
					data[second.index],
					data[first],
					data[third.index],
					alpha,
				) else {
					continue;
				};

				if tree.empty(&center, (alpha - 0.001).powi(2)) {
					return Some(([second.index, first, third.index].into(), center));
				}
			}
		}
	}
	None
}


fn find_third(
	data: &[Vector<3, f32>],
	first: usize,
	second: usize,
	tree: &Tree,
	old: usize,
	alpha: f32,
	center: Vector<3, f32>,
) -> Option<(usize, Vector<3, f32>)> {
	let a = data[first];
	let c = data[second];
	let bar = (c - a).normalized();
	let mid_point = (a + c) / 2.0;
	let to_center = (center - mid_point).normalized();

	let search_distance = alpha + (alpha.powi(2) - a.distance(mid_point).powi(2)).sqrt();

	let nearest = tree.nearest(&mid_point, search_distance.powi(2));
	let mut best = None;
	let mut best_angle = std::f32::consts::TAU;
	for third in nearest
		.iter()
		.skip(1)
		.filter(|entry| entry.index != first && entry.index != second && entry.index != old) {
		let Some(center_2) = sphere_location(data[first], data[second], data[third.index], alpha) else {
			continue;
		};
		let to_center_2 = (center_2 - mid_point).normalized();
		let angle = to_center.dot(to_center_2).clamp(-1.0, 1.0).acos();
		let angle = if to_center.cross(to_center_2).dot(bar) < 0.0 {
			std::f32::consts::TAU - angle
		} else {
			angle
		};
		if angle >= best_angle {
			continue;
		}

		best_angle = angle;
		best = Some((third.index, center));
	}
	best
}


/// https://stackoverflow.com/a/34326390
fn sphere_location(
	point_a: Vector<3, f32>,
	point_b: Vector<3, f32>,
	point_c: Vector<3, f32>,
	alpha: f32,
) -> Option<Vector<3, f32>> {
	let ac = point_c - point_a;
	let ab = point_b - point_a;
	let out = ab.cross(ac);

	let to = (out.cross(ab) * ac.length_squared() + ac.cross(out) * ab.length_squared()) / (2.0 * out.length_squared());
	let circumcenter = point_a + to;

	let dist = alpha * alpha - to.length_squared();
	if dist <= 0.0 {
		return None;
	}
	Some(circumcenter - out.normalized() * dist.sqrt())
}
