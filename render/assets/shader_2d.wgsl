struct CameraUniform {
    view_proj: mat3x3<f32>,
	depth: i32,
};

struct ModelUniform {
    transform: mat3x3<f32>,
	depth: i32,
    width: f32,
    height: f32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(
    in: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = in.tex_coords;
	let depth = 1.0 / f32(model.depth - camera.depth + 1);
    let position = camera.view_proj * model.transform * vec3<f32>(in.position * vec2<f32>(model.width, model.height), 1.0);
    out.clip_position = vec4<f32>(position.xy, depth, 1.0);
    return out;
}

@group(2) @binding(0)
var t_diffuse: texture_2d<f32>;

@group(2) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
}
