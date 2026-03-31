// Basilisk terminal shader

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) bg_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) bg_color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Convert from pixel coordinates to clip space (-1 to 1)
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    out.bg_color = in.bg_color;
    return out;
}

@group(0) @binding(0)
var t_atlas: texture_2d<f32>;
@group(0) @binding(1)
var s_atlas: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Sample the glyph atlas (grayscale)
    let alpha = textureSample(t_atlas, s_atlas, in.tex_coords).r;
    
    // Blend foreground and background based on glyph alpha
    let fg = in.color;
    let bg = in.bg_color;
    
    // Mix colors: bg * (1-alpha) + fg * alpha
    let color = mix(bg, fg, alpha);
    
    return color;
}
