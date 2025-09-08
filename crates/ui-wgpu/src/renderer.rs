use anyhow::Result;
use wgpu::*;
use wgpu::util::DeviceExt;
use winit::window::Window;
use std::sync::Arc;
use cosmic_text::{FontSystem, SwashCache, Buffer as TextBuffer, Metrics, Attrs, Shaping};
use glyphon::{
    TextRenderer as GlyphonRenderer, TextAtlas, TextArea, TextBounds,
    Resolution
};
use crate::colored_text::ColoredTextRenderer;
use the_dev_terminal_core::grid::Cell;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    pos: [f32; 2],   // pixel coords
    color: [f32; 4], // rgba
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUbo { 
    size: [f32; 2] 
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
    pending_text: String,
    pending_cells: Vec<Cell>,
    pending_cols: usize,
    pending_rows: usize,
    colored_text_renderer: ColoredTextRenderer,
    font_size: f32,
    pub cell_width: f32,
    pub cell_height: f32,
    // Selection (for visual highlighting)
    pub selection: Option<((usize, usize), (usize, usize))>,
    // Selection pipeline state
    sel_pipeline: RenderPipeline,
    sel_bindgroup: BindGroup,
    _sel_bind_layout: BindGroupLayout,
    sel_screen_ubo: Buffer,
    sel_vbuf: Buffer,
    sel_vertices: Vec<QuadVertex>,
    // Viewport controls for smooth scrolling
    pub viewport_top_row: usize,
    pub y_offset_px: f32,
    // Cursor position
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub cursor_visible: bool,
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
        
        let font_size = 18.0;
        let cell_width = font_size * 0.6;
        let cell_height = font_size * 1.25;
        
        let mut text_buffer = TextBuffer::new(&mut font_system, Metrics::new(font_size, cell_height));
        text_buffer.set_size(&mut font_system, size.width as f32, size.height as f32);
        
        let pending_text = "Hello from The Dev Terminal\n(type will show once PTY is wired)".to_string();
        
        // --- selection pipeline setup ---
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("selection.wgsl"),
            source: ShaderSource::Wgsl(include_str!("shaders/selection.wgsl").into()),
        });

        // uniform: screen size
        let screen_init = ScreenUbo { size: [config.width as f32, config.height as f32] };

        let sel_screen_ubo = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("sel.screen.ubo"),
            contents: bytemuck::bytes_of(&screen_init),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let sel_bind_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("sel.bindlayout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let sel_bindgroup = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sel.bindgroup"),
            layout: &sel_bind_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: sel_screen_ubo.as_entire_binding(),
            }],
        });

        // vertex buffer layout
        let vbuf_layout = VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                // location 0: pos (vec2<f32>)
                VertexAttribute { offset: 0, shader_location: 0, format: VertexFormat::Float32x2 },
                // location 1: color (vec4<f32>)
                VertexAttribute { offset: 8, shader_location: 1, format: VertexFormat::Float32x4 },
            ],
        };

        // pipeline
        let sel_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("sel.pipeline.layout"),
            bind_group_layouts: &[&sel_bind_layout],
            push_constant_ranges: &[],
        });

        let sel_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("sel.pipeline"),
            layout: Some(&sel_pipeline_layout),
            vertex: VertexState { 
                module: &shader, 
                entry_point: "vs_main", 
                buffers: &[vbuf_layout],
            },
            fragment: Some(FragmentState {
                module: &shader, 
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
        });

        // dynamic vertex buffer (we'll rebuild each frame as needed)
        let sel_vbuf = device.create_buffer(&BufferDescriptor {
            label: Some("sel.vbuf"),
            size: (std::mem::size_of::<QuadVertex>() * 6 * 32768) as BufferAddress, // up to 32k rects for large terminals
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        // Create the colored text renderer
        let colored_text_renderer = ColoredTextRenderer::new(&device, &queue, config.format);
        
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
            pending_text,
            pending_cells: Vec::new(),
            pending_cols: 0,
            pending_rows: 0,
            colored_text_renderer,
            font_size,
            cell_width,
            cell_height,
            selection: None,
            sel_pipeline,
            _sel_bind_layout: sel_bind_layout,
            sel_bindgroup,
            sel_screen_ubo,
            sel_vbuf,
            sel_vertices: Vec::with_capacity(6 * 4096),
            viewport_top_row: 0,
            y_offset_px: 0.0,
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
        })
    }
    
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            
            // Update text buffer size
            self.text_buffer.set_size(
                &mut self.font_system,
                new_size.width as f32,
                new_size.height as f32
            );
            
            // Update screen UBO for selection shader
            let screen_data = [new_size.width as f32, new_size.height as f32];
            self.queue.write_buffer(&self.sel_screen_ubo, 0, bytemuck::cast_slice(&screen_data));
        }
    }
    
    pub fn set_text(&mut self, s: impl Into<String>) {
        self.pending_text = s.into();
    }
    
    pub fn set_cells(&mut self, cells: Vec<Cell>, cols: usize, rows: usize) {
        self.pending_cells = cells;
        self.pending_cols = cols;
        self.pending_rows = rows;
    }
    
    pub fn set_viewport(&mut self, top_row: usize, y_offset_px: f32) {
        self.viewport_top_row = top_row;
        self.y_offset_px = y_offset_px;
    }
    
    pub fn set_cursor(&mut self, x: usize, y: usize, visible: bool) {
        self.cursor_x = x;
        self.cursor_y = y;
        self.cursor_visible = visible;
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
        
        // Recompute buffer size to the window
        self.text_buffer.set_size(
            &mut self.font_system,
            self.config.width as f32,
            self.config.height as f32
        );
    }
    
    #[inline]
    pub fn push_rect(&mut self, x: f32, y: f32, w: f32, h: f32, rgba: [f32;4]) {
        // two triangles (6 vertices) in pixel coordinates
        let (x0, y0) = (x,     y);
        let (x1, y1) = (x + w, y + h);

        let v0 = QuadVertex { pos: [x0, y0], color: rgba };
        let v1 = QuadVertex { pos: [x1, y0], color: rgba };
        let v2 = QuadVertex { pos: [x0, y1], color: rgba };
        let v3 = QuadVertex { pos: [x1, y1], color: rgba };

        // tri 1: v0, v1, v2; tri 2: v2, v1, v3
        self.sel_vertices.extend_from_slice(&[v0, v1, v2, v2, v1, v3]);
    }

    fn flush_rects<'a>(&'a mut self, encoder: &mut CommandEncoder, view: &'a TextureView) {
        if self.sel_vertices.is_empty() { return; }
        
        // upload
        self.queue.write_buffer(&self.sel_vbuf, 0, bytemuck::cast_slice(&self.sel_vertices));
        
        // draw
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("selection.pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view, 
                resolve_target: None,
                ops: Operations { 
                    load: LoadOp::Load, 
                    store: StoreOp::Store 
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        
        pass.set_pipeline(&self.sel_pipeline);
        pass.set_bind_group(0, &self.sel_bindgroup, &[]);
        pass.set_vertex_buffer(0, self.sel_vbuf.slice(..));
        pass.draw(0..(self.sel_vertices.len() as u32), 0..1);
        drop(pass);
        
        self.sel_vertices.clear();
    }
    
    pub fn render_frame(&mut self) -> Result<()> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor { 
            label: Some("encoder") 
        });

        // 1) clear background
        {
            let _rp = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view, 
                    resolve_target: None,
                    ops: Operations { 
                        load: LoadOp::Clear(Color { r: 0.06, g: 0.06, b: 0.07, a: 1.0 }), 
                        store: StoreOp::Store 
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }

        // 2) Draw colored cell backgrounds
        if !self.pending_cells.is_empty() {
            let visible_rows = (self.config.height as f32 / self.cell_height) as usize + 2;
            let visible_cols = (self.config.width as f32 / self.cell_width) as usize + 2;
            
            for row in 0..visible_rows.min(self.pending_rows) {
                for col in 0..visible_cols.min(self.pending_cols) {
                    let idx = row * self.pending_cols + col;
                    if idx >= self.pending_cells.len() {
                        break;
                    }
                    
                    let cell = &self.pending_cells[idx];
                    // Only draw background if it's not the default black
                    if cell.bg.r != 0 || cell.bg.g != 0 || cell.bg.b != 0 {
                        let x = 12.0 + col as f32 * self.cell_width;
                        let y = 12.0 + row as f32 * self.cell_height + self.y_offset_px;
                        let color = [
                            cell.bg.r as f32 / 255.0,
                            cell.bg.g as f32 / 255.0,
                            cell.bg.b as f32 / 255.0,
                            1.0,
                        ];
                        self.push_rect(x, y, self.cell_width, self.cell_height, color);
                    }
                }
            }
        }
        
        // 3) Draw cursor if visible
        if self.cursor_visible {
            let cursor_x = 12.0 + self.cursor_x as f32 * self.cell_width;
            let cursor_y = 12.0 + self.cursor_y as f32 * self.cell_height + self.y_offset_px;
            // Draw cursor as a bright block
            self.push_rect(cursor_x, cursor_y, self.cell_width, self.cell_height, [0.9, 0.9, 0.9, 0.8]);
        }
        
        // 4) push selection rects (with viewport offset)
        if let Some(((x0, y0), (x1, y1))) = self.selection {
            let minx = x0.min(x1);
            let maxx = x0.max(x1);
            let miny = y0.min(y1);
            let maxy = y0.max(y1);
            
            for row in miny..=maxy {
                let start_col = if row == miny { minx } else { 0 };
                let end_col = if row == maxy { maxx } else { 
                    (self.config.width as f32 / self.cell_width) as usize - 1 
                };
                
                for col in start_col..=end_col {
                    let x = 12.0 + col as f32 * self.cell_width;
                    // Apply y_offset_px for smooth scrolling
                    let y = 12.0 + row as f32 * self.cell_height + self.y_offset_px;
                    // Semi-transparent blue selection background
                    self.push_rect(x, y, self.cell_width, self.cell_height, [0.2, 0.4, 0.8, 0.3]);
                }
            }
        }
        
        // Flush selection and cursor rectangles
        self.flush_rects(&mut encoder, &view);

        // 5) draw text on top
        // For now, use glyphon for text rendering until we implement proper glyph atlas
        self.text_buffer.set_text(
            &mut self.font_system,
            &self.pending_text,
            Attrs::new().family(cosmic_text::Family::Monospace),
            Shaping::Advanced,
        );
        
        let text_areas = vec![TextArea {
            buffer: &self.text_buffer,
            left: 12.0,
            top: 12.0 + self.y_offset_px,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: self.config.width as i32,
                bottom: self.config.height as i32,
            },
            default_color: glyphon::Color::rgb(255, 255, 255),
        }];
        
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
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Text Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            
            self.text_renderer.render(&self.text_atlas, &mut render_pass)?;
        }

        // 4) submit
        self.queue.submit([encoder.finish()]);
        output.present();
        
        Ok(())
    }
}