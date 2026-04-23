use std::sync::Arc;

use wgpu::SurfaceError;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowBuilder};

use crate::app::{App, EmberConfig};
use crate::input::{key_from_winit, InputState};
use crate::render::{DrawCommand, DrawQueue, Renderer2D};

pub fn run(config: EmberConfig, app: App) {
    env_logger::try_init().ok();
    if let Err(error) = pollster::block_on(run_async(config, app)) {
        eprintln!("ember-runtime error: {error}");
    }
}

async fn run_async(config: EmberConfig, mut app: App) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(config.title.clone())
            .with_inner_size(PhysicalSize::new(config.width, config.height))
            .build(&event_loop)
            .map_err(|error| error.to_string())?,
    );
    let mut gpu = GpuContext::new(Arc::clone(&window), &config).await?;
    let clear_color = wgpu::Color {
        r: config.background[0],
        g: config.background[1],
        b: config.background[2],
        a: config.background[3],
    };

    event_loop
        .run(move |event, target| {
            target.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => target.exit(),
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == ElementState::Pressed
                                && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape))
                            {
                                target.exit();
                            }
                            if let PhysicalKey::Code(code) = event.physical_key {
                                if let Some(key) = key_from_winit(code) {
                                    if let Some(input) = app.world.get_resource_mut::<InputState>()
                                    {
                                        match event.state {
                                            ElementState::Pressed => input.press(key),
                                            ElementState::Released => input.release(key),
                                        }
                                    }
                                }
                            }
                        }
                        WindowEvent::Resized(size) => gpu.resize(size),
                        WindowEvent::RedrawRequested => {
                            app.tick();
                            let commands = app
                                .world
                                .get_resource_mut::<DrawQueue>()
                                .map(DrawQueue::take)
                                .unwrap_or_default();
                            match gpu.render(clear_color, &commands) {
                                Ok(()) => {}
                                Err(SurfaceError::Lost | SurfaceError::Outdated) => {
                                    gpu.resize(gpu.size);
                                }
                                Err(SurfaceError::OutOfMemory) => target.exit(),
                                Err(SurfaceError::Timeout) => {}
                            }
                            if let Some(input) = app.world.get_resource_mut::<InputState>() {
                                input.end_frame();
                            }
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => window.request_redraw(),
                _ => {}
            }
        })
        .map_err(|error| error.to_string())
}

struct GpuContext {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    renderer: Renderer2D,
}

impl GpuContext {
    async fn new(window: Arc<Window>, config: &EmberConfig) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|error| error.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "failed to find a suitable GPU adapter".to_string())?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("ember-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let present_mode = if capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            wgpu::PresentMode::Fifo
        } else {
            capabilities.present_modes[0]
        };
        let alpha_mode = capabilities.alpha_modes[0];
        let width = size.width.max(1).max(config.width);
        let height = size.height.max(1).max(config.height);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);
        let renderer = Renderer2D::new(&device, format);

        Ok(Self {
            surface,
            device,
            queue,
            config: surface_config,
            size: PhysicalSize::new(width, height),
            renderer,
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render(
        &mut self,
        clear_color: wgpu::Color,
        commands: &[DrawCommand],
    ) -> Result<(), SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ember-clear-encoder"),
            });
        self.renderer.prepare(
            &self.device,
            &self.queue,
            self.config.width as f32,
            self.config.height as f32,
            commands,
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ember-clear-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.renderer.draw(&mut pass);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
