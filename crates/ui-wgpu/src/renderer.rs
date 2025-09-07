use anyhow::Result;
use wgpu::{Instance, Surface, Device, Queue, SurfaceConfiguration};
use winit::window::Window;
use std::sync::Arc;
use cosmic_text::{FontSystem, SwashCache, Buffer as TextBuffer, Metrics, Attrs, Shaping};
use glyphon::{
    TextRenderer as GlyphonRenderer, TextAtlas, TextArea, TextBounds,
    Resolution
};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    position: [f32; 2],
    color: [f32; 4],
}

pub struct Renderer {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'static>,
    pub config: SurfaceConfiguration,
    // Text rendering
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_renderer: GlyphonRenderer,
    text_atlas: TextAtlas,
    text_buffer: TextBuffer,
    text_buffer_selected: TextBuffer,  // Separate buffer for selected text
    pending_text: String,
    font_size: f32,
    pub cell_width: f32,
    pub cell_height: f32,
    // Selection (for visual highlighting)
    pub selection: Option<((usize, usize), (usize, usize))>,
    // Quad rendering for selection background
    quad_pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });
        
        let surface = instance.create_surface(window.clone())?;
        
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("Failed to find suitable adapter"))?;
        
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("The-Dev-Terminal Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;
        
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        
        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![surface_format],
            desired_maximum_frame_latency: 2,
        };
        
        surface.configure(&device, &config);
        
        // Initialize text rendering
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let mut text_atlas = TextAtlas::new(&device, &queue, surface_format);
        let text_renderer = GlyphonRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        
        let font_size = 14.0;
        let cell_width = font_size * 0.6;
        let cell_height = font_size * 1.25;
        
        let mut text_buffer = TextBuffer::new(&mut font_system, Metrics::new(font_size, cell_height));
        text_buffer.set_size(&mut font_system, size.width as f32, size.height as f32);
        
        let mut text_buffer_selected = TextBuffer::new(&mut font_system, Metrics::new(font_size, cell_height));
        text_buffer_selected.set_size(&mut font_system, size.width as f32, size.height as f32);
        
        let pending_text = "Hello from The Dev Terminal\n(type will show once PTY is wired)".to_string();
        
        // Create quad pipeline for selection background
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });
        
        let quad_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        
        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Quad Pipeline"),
            layout: Some(&quad_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        
        // Create vertex buffer for quads (will be updated each frame)
        let quad_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Quad Vertex Buffer"),
            size: 1024 * std::mem::size_of::<QuadVertex>() as u64, // Space for many quads
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        Ok(Self {
            device,
            queue,
            surface,
            config,
            font_system,
            swash_cache,
            text_renderer,
            text_atlas,
            text_buffer,
            text_buffer_selected,
            pending_text,
            font_size,
            cell_width,
            cell_height,
            selection: None,
            quad_pipeline,
            quad_vertex_buffer,
        })
    }
    
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            
            // Update text buffer sizes
            self.text_buffer.set_size(
                &mut self.font_system,
                new_size.width as f32,
                new_size.height as f32
            );
            self.text_buffer_selected.set_size(
                &mut self.font_system,
                new_size.width as f32,
                new_size.height as f32
            );
        }
    }
    
    pub fn set_text(&mut self, s: impl Into<String>) {
        self.pending_text = s.into();
    }
    
    pub fn font_size(&self) -> f32 {
        self.font_size
    }
    
    pub fn set_font_size(&mut self, pt: f32) {
        const MIN_PT: f32 = 8.0;
        const MAX_PT: f32 = 48.0;
        
        let pt = pt.clamp(MIN_PT, MAX_PT);
        self.font_size = pt;
        self.cell_width = pt * 0.6;
        self.cell_height = pt * 1.25;
        
        // Update glyphon buffer metrics
        self.text_buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(self.font_size, self.cell_height)
        );
        self.text_buffer_selected.set_metrics(
            &mut self.font_system,
            Metrics::new(self.font_size, self.cell_height)
        );
        
        // Recompute buffer size to the window
        self.text_buffer.set_size(
            &mut self.font_system,
            self.config.width as f32,
            self.config.height as f32
        );
        self.text_buffer_selected.set_size(
            &mut self.font_system,
            self.config.width as f32,
            self.config.height as f32
        );
    }
    
    pub fn render_frame(&mut self) -> Result<()> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });
        
        // Clear to dark gray
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.06,
                            g: 0.06,
                            b: 0.07,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        
        // Render selection background if there's a selection
        if let Some(((x0, y0), (x1, y1))) = self.selection {
            let min_x = x0.min(x1);
            let max_x = x0.max(x1);
            let min_y = y0.min(y1);
            let max_y = y0.max(y1);
            
            // Build quads for selected cells
            let mut vertices = Vec::new();
            
            for row in min_y..=max_y {
                let start_col = if row == min_y { min_x } else { 0 };
                let end_col = if row == max_y { max_x } else { self.config.width as usize / self.cell_width as usize };
                
                // Create a quad for this row's selection
                let left = 12.0 + (start_col as f32 * self.cell_width);
                let right = 12.0 + ((end_col + 1) as f32 * self.cell_width);
                let top = 12.0 + (row as f32 * self.cell_height);
                let bottom = top + self.cell_height;
                
                // Convert to NDC coordinates (-1 to 1)
                let left_ndc = (left / self.config.width as f32) * 2.0 - 1.0;
                let right_ndc = (right / self.config.width as f32) * 2.0 - 1.0;
                let top_ndc = 1.0 - (top / self.config.height as f32) * 2.0;
                let bottom_ndc = 1.0 - (bottom / self.config.height as f32) * 2.0;
                
                // Semi-transparent blue background
                let color = [0.2, 0.4, 0.8, 0.3]; // Blue with 30% opacity
                
                // Two triangles for a quad
                vertices.extend_from_slice(&[
                    QuadVertex { position: [left_ndc, top_ndc], color },
                    QuadVertex { position: [right_ndc, top_ndc], color },
                    QuadVertex { position: [left_ndc, bottom_ndc], color },
                    
                    QuadVertex { position: [right_ndc, top_ndc], color },
                    QuadVertex { position: [right_ndc, bottom_ndc], color },
                    QuadVertex { position: [left_ndc, bottom_ndc], color },
                ]);
            }
            
            if !vertices.is_empty() {
                // Update vertex buffer
                self.queue.write_buffer(
                    &self.quad_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&vertices),
                );
                
                // Render selection background
                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Selection Background Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });
                    
                    render_pass.set_pipeline(&self.quad_pipeline);
                    render_pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
                    render_pass.draw(0..vertices.len() as u32, 0..1);
                }
            }
        }
        
        // Prepare text - split into selected and non-selected
        let lines: Vec<&str> = self.pending_text.lines().collect();
        let mut normal_text = String::new();
        let mut selected_text = String::new();
        
        if let Some(((x0, y0), (x1, y1))) = self.selection {
            let min_x = x0.min(x1);
            let max_x = x0.max(x1);
            let min_y = y0.min(y1);
            let max_y = y0.max(y1);
            
            for (row, line) in lines.iter().enumerate() {
                let chars: Vec<char> = line.chars().collect();
                for (col, ch) in chars.iter().enumerate() {
                    let is_selected = row >= min_y && row <= max_y &&
                        ((row == min_y && col >= min_x) || row > min_y) &&
                        ((row == max_y && col <= max_x) || row < max_y);
                    
                    if is_selected {
                        selected_text.push(*ch);
                        normal_text.push(' '); // Space placeholder
                    } else {
                        normal_text.push(*ch);
                        selected_text.push(' '); // Space placeholder
                    }
                }
                normal_text.push('\n');
                selected_text.push('\n');
            }
        } else {
            normal_text = self.pending_text.clone();
            selected_text = self.pending_text.chars().map(|c| if c == '\n' { '\n' } else { ' ' }).collect();
        }
        
        // Update text buffers
        self.text_buffer.set_text(
            &mut self.font_system,
            &normal_text,
            Attrs::new().family(cosmic_text::Family::Monospace),
            Shaping::Advanced,
        );
        
        self.text_buffer_selected.set_text(
            &mut self.font_system,
            &selected_text,
            Attrs::new().family(cosmic_text::Family::Monospace),
            Shaping::Advanced,
        );
        
        // Prepare text areas
        let mut text_areas = vec![
            TextArea {
                buffer: &self.text_buffer,
                left: 12.0,
                top: 12.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: self.config.width as i32,
                    bottom: self.config.height as i32,
                },
                default_color: glyphon::Color::rgb(255, 255, 255), // White for normal text
            }
        ];
        
        // Add selected text overlay if there's a selection
        if self.selection.is_some() {
            text_areas.push(TextArea {
                buffer: &self.text_buffer_selected,
                left: 12.0,
                top: 12.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: self.config.width as i32,
                    bottom: self.config.height as i32,
                },
                default_color: glyphon::Color::rgb(100, 150, 255), // Light blue for selected text
            });
        }
        
        self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
            text_areas,
            &mut self.swash_cache,
        )?;
        
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Text Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            
            self.text_renderer.render(&self.text_atlas, &mut render_pass)?;
        }
        
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
    }
}