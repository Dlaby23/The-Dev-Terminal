use anyhow::Result;
use wgpu::{Instance, Surface, Device, Queue, SurfaceConfiguration};
use winit::window::Window;
use std::sync::Arc;
use cosmic_text::{FontSystem, SwashCache, Buffer as TextBuffer, Metrics, Attrs, Shaping};
use glyphon::{
    TextRenderer as GlyphonRenderer, TextAtlas, TextArea, TextBounds,
    Resolution
};

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
    font_size: f32,
    pub cell_width: f32,
    pub cell_height: f32,
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
        
        let pending_text = "Hello from The Dev Terminal\n(type will show once PTY is wired)".to_string();
        
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
            font_size,
            cell_width,
            cell_height,
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
        
        // Recompute buffer size to the window
        self.text_buffer.set_size(
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
        
        // Update and render text
        self.text_buffer.set_text(
            &mut self.font_system,
            &self.pending_text,
            Attrs::new().family(cosmic_text::Family::Monospace),
            Shaping::Advanced,
        );
        
        let text_areas = vec![TextArea {
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