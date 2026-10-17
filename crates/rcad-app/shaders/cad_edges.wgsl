// B-Rep edge pass for the rcad viewport (foundation A4).
//
// Edges are drawn after the shaded faces, with the depth test on so hidden
// edges stay hidden. The CPU pre-classifies tangent edges from the dihedral
// angle and sends a dimmed colour for them.

struct Uniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    out.color = input.color;
    return out;
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    return pow(color, vec3<f32>(2.2));
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(srgb_to_linear(input.color), 1.0);
}
