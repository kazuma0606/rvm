use std::sync::OnceLock;

use ab_glyph::{point, Font, FontArc, ScaleFont};
use bytemuck::{Pod, Zeroable};

use crate::ecs::World;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn white() -> Self {
        Self::rgb(1.0, 1.0, 1.0)
    }

    pub const fn black() -> Self {
        Self::rgb(0.0, 0.0, 0.0)
    }

    pub const fn red() -> Self {
        Self::rgb(1.0, 0.0, 0.0)
    }

    pub const fn green() -> Self {
        Self::rgb(0.0, 1.0, 0.0)
    }

    pub const fn cyan() -> Self {
        Self::rgb(0.0, 1.0, 1.0)
    }

    pub const fn yellow() -> Self {
        Self::rgb(1.0, 1.0, 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

impl Position {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub w: f32,
    pub h: f32,
    pub color: Color,
}

impl Rect {
    pub const fn new(w: f32, h: f32, color: Color) -> Self {
        Self { w, h, color }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub radius: f32,
    pub color: Color,
}

impl Circle {
    pub const fn new(radius: f32, color: Color) -> Self {
        Self { radius, color }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub content: String,
    pub size: f32,
    pub color: Color,
}

impl Text {
    pub fn new(content: impl Into<String>, size: f32, color: Color) -> Self {
        Self {
            content: content.into(),
            size,
            color,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    },
    Circle {
        x: f32,
        y: f32,
        radius: f32,
        color: Color,
    },
    Text {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        color: Color,
    },
}

#[derive(Debug, Default)]
pub struct DrawQueue {
    commands: Vec<DrawCommand>,
}

impl DrawQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.commands.push(DrawCommand::Rect { x, y, w, h, color });
    }

    pub fn draw_circle(&mut self, x: f32, y: f32, radius: f32, color: Color) {
        self.commands.push(DrawCommand::Circle {
            x,
            y,
            radius,
            color,
        });
    }

    pub fn draw_text(&mut self, text: impl Into<String>, x: f32, y: f32, size: f32, color: Color) {
        self.commands.push(DrawCommand::Text {
            text: text.into(),
            x,
            y,
            size,
            color,
        });
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn take(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.commands)
    }
}

pub fn draw_rects(world: &mut World) {
    let commands = world
        .query2::<Position, Rect>()
        .into_iter()
        .map(|(_, position, rect)| DrawCommand::Rect {
            x: position.x,
            y: position.y,
            w: rect.w,
            h: rect.h,
            color: rect.color,
        })
        .collect::<Vec<_>>();
    let queue = world.resource_mut::<DrawQueue>();
    for command in commands {
        if let DrawCommand::Rect { x, y, w, h, color } = command {
            queue.draw_rect(x, y, w, h, color);
        }
    }
}

pub fn draw_circles(world: &mut World) {
    let commands = world
        .query2::<Position, Circle>()
        .into_iter()
        .map(|(_, position, circle)| DrawCommand::Circle {
            x: position.x,
            y: position.y,
            radius: circle.radius,
            color: circle.color,
        })
        .collect::<Vec<_>>();
    let queue = world.resource_mut::<DrawQueue>();
    for command in commands {
        if let DrawCommand::Circle {
            x,
            y,
            radius,
            color,
        } = command
        {
            queue.draw_circle(x, y, radius, color);
        }
    }
}

pub fn draw_texts(world: &mut World) {
    let commands = world
        .query2::<Position, Text>()
        .into_iter()
        .map(|(_, position, text)| DrawCommand::Text {
            text: text.content.clone(),
            x: position.x,
            y: position.y,
            size: text.size,
            color: text.color,
        })
        .collect::<Vec<_>>();
    let queue = world.resource_mut::<DrawQueue>();
    for command in commands {
        if let DrawCommand::Text {
            text,
            x,
            y,
            size,
            color,
        } = command
        {
            queue.draw_text(text, x, y, size, color);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub struct Renderer2D {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    vertex_capacity: usize,
    index_capacity: usize,
    index_count: u32,
}

impl Renderer2D {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ember-2d-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ember-2d-pipeline-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ember-2d-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Self {
            pipeline,
            vertex_buffer: None,
            index_buffer: None,
            vertex_capacity: 0,
            index_capacity: 0,
            index_count: 0,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: f32,
        height: f32,
        commands: &[DrawCommand],
    ) {
        let (vertices, indices) = build_geometry(commands, width, height);
        if vertices.is_empty() || indices.is_empty() {
            self.index_count = 0;
            return;
        }

        if self.vertex_capacity < vertices.len() {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ember-2d-vertices"),
                size: (self.vertex_capacity * std::mem::size_of::<Vertex>()) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.index_capacity < indices.len() {
            self.index_capacity = indices.len().next_power_of_two();
            self.index_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ember-2d-indices"),
                size: (self.index_capacity * std::mem::size_of::<u32>()) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        let vertex_buffer = self.vertex_buffer.as_ref().expect("vertex buffer");
        let index_buffer = self.index_buffer.as_ref().expect("index buffer");
        queue.write_buffer(vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        queue.write_buffer(index_buffer, 0, bytemuck::cast_slice(&indices));
        self.index_count = indices.len() as u32;
    }

    pub fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.index_count == 0 {
            return;
        }
        let Some(vertex_buffer) = self.vertex_buffer.as_ref() else {
            return;
        };
        let Some(index_buffer) = self.index_buffer.as_ref() else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

fn build_geometry(commands: &[DrawCommand], width: f32, height: f32) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for command in commands {
        match command {
            &DrawCommand::Rect { x, y, w, h, color } => {
                push_rect(
                    &mut vertices,
                    &mut indices,
                    width,
                    height,
                    x,
                    y,
                    w,
                    h,
                    color,
                );
            }
            &DrawCommand::Circle {
                x,
                y,
                radius,
                color,
            } => {
                push_circle(
                    &mut vertices,
                    &mut indices,
                    width,
                    height,
                    x,
                    y,
                    radius,
                    color,
                );
            }
            DrawCommand::Text {
                text,
                x,
                y,
                size,
                color,
            } => {
                push_text(
                    &mut vertices,
                    &mut indices,
                    width,
                    height,
                    text,
                    *x,
                    *y,
                    *size,
                    *color,
                );
            }
        }
    }
    (vertices, indices)
}

fn push_rect(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Color,
) {
    let base = vertices.len() as u32;
    vertices.extend([
        vertex(x, y, width, height, color),
        vertex(x + w, y, width, height, color),
        vertex(x + w, y + h, width, height, color),
        vertex(x, y + h, width, height, color),
    ]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_circle(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    radius: f32,
    color: Color,
) {
    const SEGMENTS: u32 = 32;
    let center = vertices.len() as u32;
    vertices.push(vertex(x, y, width, height, color));
    for i in 0..SEGMENTS {
        let angle = (i as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        vertices.push(vertex(
            x + radius * angle.cos(),
            y + radius * angle.sin(),
            width,
            height,
            color,
        ));
    }
    for i in 0..SEGMENTS {
        indices.extend([center, center + 1 + i, center + 1 + ((i + 1) % SEGMENTS)]);
    }
}

fn push_text(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    width: f32,
    height: f32,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
) {
    if size <= 0.0 || text.is_empty() {
        return;
    }
    if let Some(font) = default_font() {
        push_ab_glyph_text(
            vertices, indices, width, height, text, x, y, size, color, font,
        );
    } else {
        push_bitmap_text(vertices, indices, width, height, text, x, y, size, color);
    }
}

fn push_ab_glyph_text(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    width: f32,
    height: f32,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
    font: &FontArc,
) {
    let scaled = font.as_scaled(size);
    let mut cursor = x;
    let mut baseline = y + scaled.ascent();
    for ch in text.chars() {
        if ch == '\n' {
            cursor = x;
            baseline += scaled.height();
            continue;
        }
        let glyph_id = scaled.glyph_id(ch);
        let glyph = glyph_id.with_scale_and_position(size, point(cursor, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|px, py, coverage| {
                if coverage <= 0.0 {
                    return;
                }
                push_rect(
                    vertices,
                    indices,
                    width,
                    height,
                    bounds.min.x + px as f32,
                    bounds.min.y + py as f32,
                    1.0,
                    1.0,
                    Color::rgba(color.r, color.g, color.b, color.a * coverage),
                );
            });
        }
        cursor += scaled.h_advance(glyph_id);
    }
}

fn push_bitmap_text(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    width: f32,
    height: f32,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: Color,
) {
    let scale = (size / 8.0).max(1.0);
    let mut cursor_x = x;
    let mut cursor_y = y;
    for ch in text.chars() {
        if ch == '\n' {
            cursor_x = x;
            cursor_y += 8.0 * scale;
            continue;
        }
        for (row, bits) in bitmap_glyph(ch).iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) == 0 {
                    continue;
                }
                push_rect(
                    vertices,
                    indices,
                    width,
                    height,
                    cursor_x + col as f32 * scale,
                    cursor_y + row as f32 * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
        cursor_x += 6.0 * scale;
    }
}

fn default_font() -> Option<&'static FontArc> {
    static FONT: OnceLock<Option<FontArc>> = OnceLock::new();
    FONT.get_or_init(load_default_font).as_ref()
}

#[cfg(not(target_arch = "wasm32"))]
fn load_default_font() -> Option<FontArc> {
    [
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/calibri.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    ]
    .iter()
    .find_map(|path| {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| FontArc::try_from_vec(bytes).ok())
    })
}

#[cfg(target_arch = "wasm32")]
fn load_default_font() -> Option<FontArc> {
    None
}

fn bitmap_glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        '0' => [
            0b11110, 0b10011, 0b10101, 0b11001, 0b10001, 0b10001, 0b11110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b11110, 0b00001, 0b00001, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b10010, 0b10010, 0b10010, 0b11111, 0b00010, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b01010, 0b10001,
        ],
        'Y' => [
            0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '!' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        ' ' => [0; 7],
        _ => [
            0b11111, 0b10001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
    }
}

fn vertex(x: f32, y: f32, width: f32, height: f32, color: Color) -> Vertex {
    Vertex {
        position: [(x / width) * 2.0 - 1.0, 1.0 - (y / height) * 2.0],
        color: [color.r, color.g, color.b, color.a],
    }
}

const SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_queue_collects_commands() {
        let mut queue = DrawQueue::new();
        queue.draw_rect(1.0, 2.0, 3.0, 4.0, Color::red());
        queue.draw_circle(5.0, 6.0, 7.0, Color::cyan());
        queue.draw_text("SCORE: 10", 8.0, 9.0, 16.0, Color::white());

        assert_eq!(queue.commands().len(), 3);
        assert_eq!(queue.take().len(), 3);
        assert!(queue.commands().is_empty());
    }

    #[test]
    fn rect_and_circle_geometry_use_pixel_coordinates() {
        let commands = [
            DrawCommand::Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0,
                color: Color::white(),
            },
            DrawCommand::Circle {
                x: 50.0,
                y: 25.0,
                radius: 10.0,
                color: Color::yellow(),
            },
        ];

        let (vertices, indices) = build_geometry(&commands, 100.0, 50.0);
        assert_eq!(vertices[0].position, [-1.0, 1.0]);
        assert_eq!(vertices[2].position, [1.0, -1.0]);
        assert_eq!(vertices.len(), 37);
        assert_eq!(indices.len(), 102);
    }

    #[test]
    fn draw_systems_enqueue_matching_components() {
        let mut world = World::new();
        world.insert_resource(DrawQueue::new());
        world
            .spawn()
            .with(Position::new(10.0, 20.0))
            .with(Rect::new(30.0, 40.0, Color::green()))
            .with(Circle::new(8.0, Color::cyan()))
            .with(Text::new("OK", 12.0, Color::white()))
            .build();

        draw_rects(&mut world);
        draw_circles(&mut world);
        draw_texts(&mut world);

        let commands = world.resource::<DrawQueue>().commands();
        assert_eq!(commands.len(), 3);
        assert!(matches!(commands[0], DrawCommand::Rect { .. }));
        assert!(matches!(commands[1], DrawCommand::Circle { .. }));
        assert!(matches!(commands[2], DrawCommand::Text { .. }));
    }

    #[test]
    fn text_geometry_rasterizes_to_rectangles() {
        let commands = [DrawCommand::Text {
            text: "A1".to_string(),
            x: 0.0,
            y: 0.0,
            size: 12.0,
            color: Color::white(),
        }];

        let (vertices, indices) = build_geometry(&commands, 200.0, 100.0);
        assert!(!vertices.is_empty());
        assert!(!indices.is_empty());
        assert_eq!(vertices.len() % 4, 0);
        assert_eq!(indices.len() % 6, 0);
    }
}
