mod cache;
mod calculations;
mod laz;
mod level_of_detail;
mod point;
mod progress;
mod segment;
mod tree;
mod writer;

use std::{num::NonZeroU32, path::PathBuf};

use math::{X, Y, Z};
use point::PointsCollection;
use progress::Progress;
use rand::seq::SliceRandom;
use rayon::prelude::*;
use writer::Writer;

use tree::Tree;

use crate::{cache::Cache, progress::Stage, segment::Segmenter};

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("No input file")]
	NoInputFile,

	#[error("No output folder")]
	NoOutputFolder,

	#[error(transparent)]
	InvalidFile(#[from] std::io::Error),

	#[error("Corrupt file")]
	CorruptFile,

	#[error(transparent)]
	LasZipError(#[from] ::laz::LasZipError),

	#[error("Output folder is file")]
	OutputFolderIsFile,

	#[error("Output folder is not empty")]
	OutputFolderIsNotEmpty,

	#[error("Atleast two Threads are required")]
	NotEnoughThreads,
}

#[derive(clap::Args)]
pub struct Settings {
	/// Minimum size for segments. Segments with less points are removed.
	#[arg(long, default_value_t = 100)]
	min_segment_size: usize,

	/// Width of the horizontal slice in meters
	#[arg(long, default_value_t = 1.0)]
	segmenting_slice_width: f32,

	/// Distance to combine segments in meters
	#[arg(long, default_value_t = 1.0)]
	segmenting_max_distance: f32,

	/// Maximum count for neighbors search
	#[arg(long, default_value_t = 31)]
	neighbors_count: usize,

	/// Maximum distance in meters for the neighbors search
	#[arg(long, default_value_t = 1.0)]
	neighbors_max_distance: f32,

	/// Scale for the size of the combined point
	#[arg(long, default_value_t = 0.95)]
	lod_size_scale: f32,
}

#[derive(clap::Parser)]
pub struct Command {
	/// Input file location. Open File Dialog if not specified.
	input_file: Option<PathBuf>,

	/// Output folder location. Open File Dialog if not specified.
	#[arg(long, short)]
	output_folder: Option<PathBuf>,

	/// Maximal thread count for multithreading. 0 for the amount of logical cores.
	#[arg(long, default_value_t = 0)]
	max_threads: usize,

	#[command(flatten)]
	settings: Settings,
}

#[derive(Default, serde::Serialize)]
pub struct Statistics {
	source_points: usize,
	leaf_points: usize,
	branch_points: usize,
	segments: usize,
	times: Times,
}

#[derive(Default, serde::Serialize)]
pub struct Times {
	setup: f32,
	import: f32,
	segment: f32,
	calculate: f32,
	project: f32,
	lods: f32,
}

pub fn run(command: Command) -> Result<(), Error> {
	let input = match command.input_file {
		Some(file) => file,
		None => rfd::FileDialog::new()
			.set_title("Select Input File")
			.add_filter("Input File", &["las", "laz"])
			.pick_file()
			.ok_or(Error::NoInputFile)?,
	};

	let output = match command.output_folder {
		Some(folder) => folder,
		None => rfd::FileDialog::new()
			.set_title("Select Output Folder")
			.pick_folder()
			.ok_or(Error::NoOutputFolder)?,
	};

	if command.max_threads == 1 {
		return Err(Error::NotEnoughThreads);
	}

	let settings = command.settings;

	rayon::ThreadPoolBuilder::new()
		.num_threads(command.max_threads)
		.build()
		.unwrap()
		.install(|| import(settings, input, output))
}

fn import(settings: Settings, input: PathBuf, output: PathBuf) -> Result<(), Error> {
	let mut cache = Cache::new(4_000_000_000);
	let mut statistics = Statistics::default();
	let stage = Stage::new("Setup Files");

	Writer::setup(&output)?;

	let laz = laz::Laz::new(&input)?;
	let min = laz.min;
	let max = laz.max;
	let diff = max - min;
	let total_points = laz.total;
	statistics.source_points = total_points;

	statistics.times.setup = stage.finish();

	let mut progress = Progress::new("Import", total_points);

	let mut segmenter = Segmenter::new(min, max, &mut cache, &settings);

	let (sender, reciever) = crossbeam::channel::bounded(4);

	rayon::join(
		|| {
			laz.read(|chunk| sender.send(chunk).unwrap())?;
			drop(sender);
			Result::<(), Error>::Ok(())
		},
		|| {
			for chunk in reciever {
				let l = chunk.length();
				for point in chunk {
					segmenter.add_point(point, &mut cache);
				}
				progress.step_by(l);
			}
		},
	)
	.0?;

	statistics.times.import = progress.finish();

	let mut segments = segmenter.segments(&mut statistics, &mut cache);
	statistics.segments = segments.len();
	segments.shuffle(&mut rand::thread_rng());

	let mut progress = Progress::new("Calculate", total_points);

	let mut tree = Tree::new(min, diff[X].max(diff[Y]).max(diff[Z]));
	let segments_information = vec![String::from("Trunk"), String::from("Crown")];

	let (sender, reciever) = crossbeam::channel::bounded(2);
	let (_, segment_values) = rayon::join(
		|| {
			segments
				.into_par_iter()
				.enumerate()
				.for_each(|(index, segment)| {
					let index = NonZeroU32::new(index as u32 + 1).unwrap();
					let (points, information) = calculations::calculate(segment.points(), index, &settings);
					sender.send((points, index, information)).unwrap();
				});
			drop(sender);
		},
		|| {
			let mut path = output.clone();
			path.push("segments");
			std::fs::create_dir(&path).unwrap();
			let mut segment_writer = Writer::new(path, statistics.segments);
			let mut segment_values =
				vec![project::Value::Percent(0.0); statistics.segments * segments_information.len()];
			for (points, segment, information) in reciever {
				let collection = PointsCollection::from_points(&points);
				segment_writer.save(segment.get() as usize - 1, &collection);
				let offset = (segment.get() - 1) as usize;
				segment_values[offset * segments_information.len()] = information.trunk_height;
				segment_values[offset * segments_information.len() + 1] = information.crown_height;
				let l = points.len();
				for point in points {
					tree.insert(point, &mut cache);
				}
				progress.step_by(l);
			}
			segment_values
		},
	);

	statistics.times.calculate = progress.finish();

	let stage = Stage::new("Save Project");

	let properties = [
		("segment", "Segment", statistics.segments as u32),
		("height", "Height", u32::MAX),
		("slice", "Expansion", u32::MAX),
		("curve", "Curvature", u32::MAX),
	];

	let (tree, project) = tree.flatten(
		&properties,
		input.display().to_string(),
		cache,
		segments_information,
		segment_values,
	);

	let mut writer = Writer::new(output, project.root.index as usize + 1);
	writer.save_project(&project);

	statistics.times.project = stage.finish();

	tree.save(writer, &settings, statistics);

	Ok(())
}
