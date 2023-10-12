use common::Project;
use math::{Vector, X, Y};

use crate::{
	interface::Interface,
	loaded_manager::LoadedManager,
	lod,
	state::State,
	tree::{Node, Tree},
};

pub struct Game {
	window: render::Window,
	tree: Tree,
	pipeline: render::Pipeline3D,
	project: Project,
	fps_counter: FpsCounter,
	path: String,
	project_time: std::time::SystemTime,

	state: &'static State,
	mouse: input::Mouse,
	keyboard: input::Keyboard,
	time: Time,

	ui: render::UI,
	eye_dome: render::EyeDome,
	interface: Interface,
}

impl Game {
	fn camera_changed(&mut self) {
		self.window.request_redraw();
	}

	pub fn new(state: &'static State, path: String, runner: &render::Runner) -> Self {
		let project_path = format!("{}/project.epc", path);
		let project = Project::from_file(&project_path);

		let tree = Tree::new(state, &project, path.clone());
		let window = render::Window::new(state, &runner.event_loop, &path);

		let eye_dome = render::EyeDome::new(state, window.config(), window.depth_texture(), 5.0, 0.005);
		let ui = render::UI::new(state, window.config());
		let mut interface = Interface::new();
		interface.update_eye_dome_settings(eye_dome.strength, eye_dome.sensitivity);

		Self {
			ui,
			eye_dome,
			interface,

			window,
			tree,
			pipeline: render::Pipeline3D::new(state),
			project,
			fps_counter: FpsCounter::new(),
			path,
			project_time: std::fs::metadata(project_path).unwrap().modified().unwrap(),

			state,
			mouse: input::Mouse::new(),
			keyboard: input::Keyboard::new(),
			time: Time::new(),
		}
	}

	fn check_reload(&mut self) {
		let project_path = format!("{}/project.epc", self.path);
		let meta = match std::fs::metadata(&project_path) {
			Ok(v) => v,
			Err(_) => return,
		};
		let project_time = match meta.modified() {
			Ok(v) => v,
			Err(_) => return,
		};
		if self.project_time == project_time {
			return;
		}
		if project_time.elapsed().unwrap() < std::time::Duration::from_millis(1000) {
			return;
		}
		self.project_time = project_time;
		self.project = Project::from_file(project_path);
		self.tree.root = Node::new(&self.project.root);

		self.tree.loaded_manager = LoadedManager::new(self.state, self.path.clone());
	}
}

impl render::Game for Game {
	fn render(&mut self, _window_id: render::WindowId) {
		self.tree.root.update(
			lod::Checker::new(&self.tree.camera.lod),
			&self.tree.camera,
			&mut self.tree.loaded_manager,
		);

		self.ui.queue(self.state, &self.interface);

		self.window
			.render(self.state, &self.pipeline, &self.tree.camera.gpu, self);
	}

	fn resize_window(&mut self, _window_id: render::WindowId, _size: Vector<2, u32>) -> render::ControlFlow {
		self.window.resized(self.state);
		self.tree.camera.cam.aspect = self.window.get_aspect();
		self.tree.camera.gpu = render::Camera3DGPU::new(
			self.state,
			&self.tree.camera.cam,
			&self.tree.camera.transform,
		);
		self.camera_changed();
		self.ui.resize(self.state, self.window.config());
		self.eye_dome
			.update_depth(self.state, self.window.depth_texture());
		render::ControlFlow::Poll
	}

	fn close_window(&mut self, _window_id: render::WindowId) -> render::ControlFlow {
		render::ControlFlow::Exit
	}

	fn time(&mut self) -> render::ControlFlow {
		let delta = self.time.elapsed();
		let mut direction: Vector<2, f32> = [0.0, 0.0].into();
		if self.keyboard.pressed(input::KeyCode::D) {
			direction[X] += 1.0;
		}
		if self.keyboard.pressed(input::KeyCode::S) {
			direction[Y] += 1.0;
		}
		if self.keyboard.pressed(input::KeyCode::A) {
			direction[X] -= 1.0;
		}
		if self.keyboard.pressed(input::KeyCode::W) {
			direction[Y] -= 1.0;
		}
		let l = direction.length();
		if l > 0.0 {
			direction *= 10.0 * delta.as_secs_f32() / l;
			self.tree.camera.movement(direction, self.state);
			self.camera_changed();
		}

		{
			let amount = 0.5 * delta.as_secs_f32();
			let mut update = false;
			if self.keyboard.pressed(input::KeyCode::U) {
				self.eye_dome.strength /= 1.0 + amount;
				update = true;
			}
			if self.keyboard.pressed(input::KeyCode::I) {
				self.eye_dome.strength *= 1.0 + amount;
				update = true;
			}
			if self.keyboard.pressed(input::KeyCode::J) {
				self.eye_dome.sensitivity /= 1.0 + amount;
				update = true;
			}
			if self.keyboard.pressed(input::KeyCode::K) {
				self.eye_dome.sensitivity *= 1.0 + amount;
				update = true;
			}
			if update {
				self.eye_dome.update_settings(self.state);
				self.interface
					.update_eye_dome_settings(self.eye_dome.strength, self.eye_dome.sensitivity);
			}
		}

		if let Some(fps) = self.fps_counter.update(delta.as_secs_f64()) {
			self.interface.update_fps(fps);
		}
		self.interface
			.update_workload(self.tree.loaded_manager.workload());

		self.check_reload();

		self.window.request_redraw(); // todo: toggle

		self.tree.loaded_manager.update();

		render::ControlFlow::Poll
	}

	fn key_changed(
		&mut self,
		_window_id: render::WindowId,
		key: input::KeyCode,
		key_state: input::State,
	) -> render::ControlFlow {
		self.keyboard.update(key, key_state);
		match (key, key_state) {
			(input::KeyCode::Escape, input::State::Pressed) => return render::ControlFlow::Exit,
			(input::KeyCode::R, input::State::Pressed) => {
				self.tree.camera.lod.increase_detail();
				self.window.request_redraw();
			},
			(input::KeyCode::F, input::State::Pressed) => {
				self.tree.camera.lod.decrese_detail();
				self.window.request_redraw();
			},
			(input::KeyCode::L, input::State::Pressed) => {
				self.tree.camera.change_lod(self.project.level as usize);
				self.window.request_redraw();
			},
			(input::KeyCode::C, input::State::Pressed) => {
				self.tree.camera.change_controller();
				self.window.request_redraw();
			},
			(input::KeyCode::N, input::State::Pressed) => {
				self.tree.camera.change_controller();
				self.window.request_redraw();
			},
			_ => {},
		}
		render::ControlFlow::Poll
	}

	fn modifiers_changed(&mut self, modifiers: input::Modifiers) {
		self.keyboard.update_modifiers(modifiers);
	}

	fn mouse_wheel(&mut self, delta: f32) -> render::ControlFlow {
		self.tree.camera.scroll(delta, self.state);
		self.camera_changed();
		render::ControlFlow::Poll
	}

	fn mouse_pressed(
		&mut self,
		_window_id: render::WindowId,
		button: input::MouseButton,
		button_state: input::State,
	) -> render::ControlFlow {
		self.mouse.update(button, button_state);
		render::ControlFlow::Poll
	}

	fn mouse_moved(&mut self, _window_id: render::WindowId, position: Vector<2, f64>) -> render::ControlFlow {
		let delta = self.mouse.delta(position);
		if self.mouse.pressed(input::MouseButton::Left) {
			self.tree.camera.rotate(delta, self.state);
			self.camera_changed();
		}
		render::ControlFlow::Poll
	}
}

impl render::Renderable<State> for Game {
	fn render<'a>(&'a self, render_pass: render::RenderPass<'a>, state: &'a State) -> render::RenderPass<'a> {
		self.tree.render(render_pass, state)
	}

	fn post_process<'a>(&'a self, render_pass: render::RenderPass<'a>, _state: &'a State) -> render::RenderPass<'a> {
		let render_pass = self.eye_dome.render(render_pass);
		self.ui.render(render_pass)
	}
}

struct FpsCounter {
	count: usize,
	time: f64,
}

impl FpsCounter {
	pub fn new() -> Self {
		Self { count: 0, time: 0.0 }
	}
	pub fn update(&mut self, delta: f64) -> Option<usize> {
		self.count += 1;
		self.time += delta;
		if self.time >= 1.0 {
			let fps = self.count;
			self.count = 0;
			self.time -= 1.0;
			Some(fps)
		} else {
			None
		}
	}
}

struct Time {
	last: std::time::Instant,
}

impl Time {
	pub fn new() -> Self {
		Self { last: std::time::Instant::now() }
	}

	pub fn elapsed(&mut self) -> std::time::Duration {
		let delta = self.last.elapsed();
		self.last = std::time::Instant::now();
		delta
	}
}
