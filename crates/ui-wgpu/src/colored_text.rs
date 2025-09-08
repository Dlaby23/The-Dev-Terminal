use wgpu::util::DeviceExt;
use the_dev_terminal_core::grid::Cell;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TextVertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
    color: [f32; 4],
}

pub struct ColoredTextRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    glyph_texture: wgpu::Texture,
    glyph_view: wgpu::TextureView,
    vertices: Vec<TextVertex>,
    indices: Vec<u16>,
}

impl ColoredTextRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // Create a simple glyph texture (we'll generate ASCII glyphs)
        let glyph_size = 256;
        let glyph_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Texture"),
            size: wgpu::Extent3d {
                width: glyph_size,
                height: glyph_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        
        // Generate ASCII glyphs (simplified - just fill with white for now)
        let glyph_data = vec![255u8; (glyph_size * glyph_size) as usize];
        
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &glyph_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &glyph_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(glyph_size),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: glyph_size,
                height: glyph_size,
                depth_or_array_layers: 1,
            },
        );
        
        let glyph_view = glyph_texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        
        // Uniform buffer for screen size
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Text Uniform Buffer"),
            contents: bytemuck::cast_slice(&[800.0f32, 600.0f32]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Text Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&glyph_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Colored Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("colored_text.wgsl").into()),
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Colored Text Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TextVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Text Vertex Buffer"),
            size: 65536 * std::mem::size_of::<TextVertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Text Index Buffer"),
            size: 98304 * std::mem::size_of::<u16>() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group,
            glyph_texture,
            glyph_view,
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }
    
    pub fn update_screen_size(&self, queue: &wgpu::Queue, width: f32, height: f32) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[width, height]));
    }
    
    pub fn prepare_cells(
        &mut self,
        queue: &wgpu::Queue,
        cells: &[Cell],
        cols: usize,
        rows: usize,
        cell_width: f32,
        cell_height: f32,
        offset_x: f32,
        offset_y: f32,
    ) {
        self.vertices.clear();
        self.indices.clear();
        
        // For each visible cell, create a colored quad
        for row in 0..rows {
            for col in 0..cols {
                let idx = row * cols + col;
                if idx >= cells.len() {
                    break;
                }
                
                let cell = &cells[idx];
                if cell.ch == '\0' || cell.ch == ' ' {
                    continue;
                }
                
                let x = offset_x + col as f32 * cell_width;
                let y = offset_y + row as f32 * cell_height;
                
                let color = [
                    cell.fg.r as f32 / 255.0,
                    cell.fg.g as f32 / 255.0,
                    cell.fg.b as f32 / 255.0,
                    1.0,
                ];
                
                // Create a simple colored rectangle for each character
                // In a real implementation, we'd use actual glyph texture coordinates
                let vertex_base = self.vertices.len() as u16;
                
                // Top-left
                self.vertices.push(TextVertex {
                    position: [x, y],
                    tex_coords: [0.0, 0.0],
                    color,
                });
                // Top-right
                self.vertices.push(TextVertex {
                    position: [x + cell_width * 0.8, y],
                    tex_coords: [1.0, 0.0],
                    color,
                });
                // Bottom-right
                self.vertices.push(TextVertex {
                    position: [x + cell_width * 0.8, y + cell_height],
                    tex_coords: [1.0, 1.0],
                    color,
                });
                // Bottom-left
                self.vertices.push(TextVertex {
                    position: [x, y + cell_height],
                    tex_coords: [0.0, 1.0],
                    color,
                });
                
                // Two triangles
                self.indices.push(vertex_base);
                self.indices.push(vertex_base + 1);
                self.indices.push(vertex_base + 2);
                self.indices.push(vertex_base);
                self.indices.push(vertex_base + 2);
                self.indices.push(vertex_base + 3);
            }
        }
        
        // Upload data
        if !self.vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&self.vertices));
            queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&self.indices));
        }
    }
    
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.indices.is_empty() {
            return;
        }
        
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
    }
}