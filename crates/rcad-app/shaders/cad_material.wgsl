// Solid shading for the rcad viewport (foundation A2).
//
// The CPU uploads one colour per vertex: the project's face colour, already
// blended with the selection/hover tint. Lighting happens here, in linear
// space, and the sRGB target converts the result back. Normals are the exact
// per-vertex normals cadrum evaluates on the B-Rep surface.

struct Uniforms {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world_position: vec3<f32>,
};

struct FragmentIn {
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world_position: vec3<f32>,
    @builtin(front_facing) front_facing: bool,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = uniforms.view_proj * vec4<f32>(input.position, 1.0);
    out.normal = input.normal;
    out.color = input.color;
    out.world_position = input.position;
    return out;
}

fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    // The colours arrive as sRGB bytes scaled to 0..1.
    return pow(color, vec3<f32>(2.2));
}

@fragment
fn fs_main(input: FragmentIn) -> @location(0) vec4<f32> {
    // A back face that survives the depth test must not be shaded with an
    // outward normal pointing away from the camera.
    let face_normal = normalize(input.normal);
    let normal = select(-face_normal, face_normal, input.front_facing);

    let light_dir = normalize(uniforms.light.xyz);
    let view_dir = normalize(uniforms.eye.xyz - input.world_position);
    let half_dir = normalize(light_dir + view_dir);

    let ambient = 0.35;
    let diffuse = max(dot(normal, light_dir), 0.0);
    let specular = pow(max(dot(normal, half_dir), 0.0), 40.0) * 0.12;

    let base = srgb_to_linear(input.color);
    let lit = base * (ambient + 0.65 * diffuse) + vec3<f32>(specular);
    return vec4<f32>(lit, 1.0);
}
