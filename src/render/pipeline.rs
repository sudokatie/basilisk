//! wgpu render pipeline creation and management
//!
//! Defines the GPU pipeline for terminal text and image rendering.

use wgpu::*;
use wgpu::util::DeviceExt;

/// Instance data for instanced rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Instance {
    /// Cell position in normalized device coordinates
    pub cell_pos: [f32; 2],
    /// UV coordinates in atlas: x, y, width, height
    pub glyph_uv: [f32; 4],
    /// Foreground color (RGBA)
    pub fg_color: [f32; 4],
    /// Background color (RGBA)
    pub bg_color: [f32; 4],
}

impl Instance {
    /// Create a new instance
    pub fn new(
        cell_pos: [f32; 2],
        glyph_uv: [f32; 4],
        fg_color: [f32; 4],
        bg_color: [f32; 4],
    ) -> Self {
        Self {
            cell_pos,
            glyph_uv,
            fg_color,
            bg_color,
        }
    }

    /// Vertex attributes for instanced rendering
    pub const ATTRIBS: [VertexAttribute; 4] = vertex_attr_array![
        2 => Float32x2,  // cell_pos
        3 => Float32x4,  // glyph_uv
        4 => Float32x4,  // fg_color
        5 => Float32x4,  // bg_color
    ];

    /// Vertex buffer layout for instances
    pub fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Instance>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Configuration for creating a render pipeline
pub struct PipelineConfig<'a> {
    /// Shader source (WGSL)
    pub shader_source: &'a str,
    /// Vertex entry point
    pub vs_entry: &'a str,
    /// Fragment entry point
    pub fs_entry: &'a str,
    /// Target texture format
    pub format: TextureFormat,
    /// Whether to use alpha blending
    pub blend: bool,
}

impl<'a> Default for PipelineConfig<'a> {
    fn default() -> Self {
        Self {
            shader_source: "",
            vs_entry: "vs_main",
            fs_entry: "fs_main",
            format: TextureFormat::Bgra8UnormSrgb,
            blend: true,
        }
    }
}

/// Create the bind group layout for the glyph atlas
pub fn create_atlas_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Atlas Bind Group Layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

/// Create the bind group for the glyph atlas
pub fn create_atlas_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    texture_view: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("Atlas Bind Group"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(texture_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// Create the main render pipeline for text
pub fn create_text_pipeline(
    device: &Device,
    config: &PipelineConfig,
    bind_group_layout: &BindGroupLayout,
    vertex_layout: VertexBufferLayout,
) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("Text Shader"),
        source: ShaderSource::Wgsl(config.shader_source.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Text Pipeline Layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    let blend_state = if config.blend {
        Some(BlendState {
            color: BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            },
            alpha: BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            },
        })
    } else {
        None
    };

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Text Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some(config.vs_entry),
            buffers: &[vertex_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some(config.fs_entry),
            targets: &[Some(ColorTargetState {
                format: config.format,
                blend: blend_state,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    })
}

/// Create a pipeline for RGBA image rendering (sixel, kitty)
pub fn create_image_pipeline(
    device: &Device,
    config: &PipelineConfig,
    bind_group_layout: &BindGroupLayout,
    vertex_layout: VertexBufferLayout,
) -> RenderPipeline {
    // Uses same structure but different fragment shader entry
    let image_config = PipelineConfig {
        fs_entry: "fs_image",
        ..*config
    };

    create_text_pipeline(device, &image_config, bind_group_layout, vertex_layout)
}

/// Create a linear sampler for the atlas
pub fn create_atlas_sampler(device: &Device) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("Atlas Sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Nearest,
        ..Default::default()
    })
}

/// Create a nearest-neighbor sampler for pixel-perfect rendering
pub fn create_nearest_sampler(device: &Device) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("Nearest Sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        ..Default::default()
    })
}

/// Create a texture for the glyph atlas
pub fn create_atlas_texture(device: &Device, width: u32, height: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("Glyph Atlas"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Create an RGBA texture for images (sixel, kitty, emoji)
pub fn create_rgba_texture(device: &Device, width: u32, height: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("RGBA Image"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Upload data to a texture
pub fn upload_texture_data(
    queue: &Queue,
    texture: &Texture,
    data: &[u8],
    width: u32,
    height: u32,
    bytes_per_pixel: u32,
) {
    queue.write_texture(
        TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * bytes_per_pixel),
            rows_per_image: Some(height),
        },
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// Create vertex buffer with initial capacity
pub fn create_vertex_buffer(device: &Device, size: u64) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some("Vertex Buffer"),
        size,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Create index buffer with initial data
pub fn create_index_buffer(device: &Device, indices: &[u16]) -> Buffer {
    device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("Index Buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: BufferUsages::INDEX,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_new() {
        let instance = Instance::new(
            [0.0, 0.0],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
        );

        assert_eq!(instance.cell_pos, [0.0, 0.0]);
        assert_eq!(instance.fg_color[0], 1.0);
        assert_eq!(instance.bg_color[3], 1.0);
    }

    #[test]
    fn instance_size() {
        // 2 + 4 + 4 + 4 = 14 floats = 56 bytes
        assert_eq!(std::mem::size_of::<Instance>(), 56);
    }

    #[test]
    fn pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.vs_entry, "vs_main");
        assert_eq!(config.fs_entry, "fs_main");
        assert!(config.blend);
    }
}
