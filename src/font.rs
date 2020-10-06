extern crate freetype as ft;
extern crate harfbuzz_rs as hb;

use std::{collections::HashMap, fmt::Debug};

use num_traits::Zero;

use rae_gfx::core as gfx;
use rae_math::{conversion::ToHomogeneous3, geometry2, geometry3};

pub use ft::{Error as FontError, FtResult as FontResult};

pub type FaceIndex = u32;
pub type FontSize = u32;

pub struct FontLibrary {
    ft_lib: ft::Library,
}

impl FontLibrary {
    pub fn new() -> FontResult<Self> {
        let ft_lib = ft::Library::init()?;
        Ok(Self { ft_lib })
    }
}

impl Debug for FontLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "FontLibrary {{ }}")
    }
}

#[derive(Debug)]
pub struct Face {
    ft_face: ft::Face,
    hb_face: hb::Shared<hb::Face<'static>>,
}

impl Face {
    pub fn from_file<P: AsRef<std::path::Path>>(
        lib: &FontLibrary,
        path: P,
        face_index: FaceIndex,
    ) -> FontResult<Self> {
        let ft_face = lib
            .ft_lib
            .new_face(path.as_ref().as_os_str(), face_index as isize)?;
        let hb_face = hb::Face::from_file(path, face_index).unwrap().to_shared();
        Ok(Self { ft_face, hb_face })
    }
}

#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Vertex {
    position: [f32; 2],
    texture_coordinates: [f32; 3],
}

impl Vertex {
    pub fn new(position: [f32; 2], texture_coordinates: [f32; 3]) -> Self {
        Self {
            position,
            texture_coordinates,
        }
    }
}

unsafe impl bytemuck::Zeroable for Vertex {
    fn zeroed() -> Self {
        Self::new([0., 0.], [0., 0., 0.])
    }
}

unsafe impl bytemuck::Pod for Vertex {}

type MeshIndexRange = gfx::MeshIndexRange;
type MeshIndex = gfx::MeshIndex;
type Mesh = gfx::IndexedMesh<Vertex>;

#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PushConstants {
    transform: geometry3::HomogeneousMatrix<f32>,
    color: gfx::ColorF32,
}

impl PushConstants {
    pub fn new(transform: &geometry2::Transform<f32>, color: gfx::ColorF32) -> Self {
        Self {
            transform: transform.to_homogeneous3(),
            color,
        }
    }

    fn as_slice(&self) -> &[u32] {
        let pc: *const PushConstants = self;
        let pc: *const u8 = pc as *const u8;
        let data = unsafe { std::slice::from_raw_parts(pc, std::mem::size_of::<PushConstants>()) };
        bytemuck::cast_slice(&data)
    }
}

unsafe impl bytemuck::Zeroable for PushConstants {
    fn zeroed() -> Self {
        Self {
            transform: geometry3::HomogeneousMatrix::zero(),
            color: gfx::ColorF32::default(),
        }
    }
}

unsafe impl bytemuck::Pod for PushConstants {}

fn bind_group_layout(instance: &gfx::Instance) -> gfx::BindGroupLayout {
    gfx::BindGroupLayout::new(
        instance,
        &gfx::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                gfx::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: gfx::ShaderStage::FRAGMENT,
                    ty: gfx::BindingType::SampledTexture {
                        multisampled: false,
                        component_type: gfx::TextureComponentType::Float,
                        dimension: gfx::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                gfx::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: gfx::ShaderStage::FRAGMENT,
                    ty: gfx::BindingType::Sampler { comparison: false },
                    count: None,
                },
            ],
        },
    )
}

#[derive(Debug)]
pub struct UniformConstants {
    bind_group: gfx::BindGroup,
}

impl UniformConstants {
    pub fn new(
        instance: &gfx::Instance,
        texture: &gfx::TextureView,
        sampler: &gfx::Sampler,
    ) -> Self {
        let layout = bind_group_layout(instance);
        let bind_group = gfx::BindGroup::new(
            instance,
            &gfx::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    gfx::BindGroupEntry {
                        binding: 0,
                        resource: gfx::BindingResource::TextureView(texture),
                    },
                    gfx::BindGroupEntry {
                        binding: 1,
                        resource: gfx::BindingResource::Sampler(sampler),
                    },
                ],
            },
        );
        Self { bind_group }
    }
}

#[derive(Debug, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderPipelineDescriptor {
    pub color_blend: gfx::BlendDescriptor,
    pub alpha_blend: gfx::BlendDescriptor,
    pub write_mask: gfx::ColorWrite,
    pub color_buffer_format: gfx::CanvasColorBufferFormat,
    pub sample_count: gfx::SampleCount,
}

impl Default for RenderPipelineDescriptor {
    fn default() -> Self {
        Self {
            color_blend: gfx::BlendDescriptor {
                src_factor: gfx::BlendFactor::SrcAlpha,
                dst_factor: gfx::BlendFactor::OneMinusSrcAlpha,
                operation: gfx::BlendOperation::Add,
            },
            alpha_blend: gfx::BlendDescriptor {
                src_factor: gfx::BlendFactor::One,
                dst_factor: gfx::BlendFactor::One,
                operation: gfx::BlendOperation::Max,
            },
            write_mask: gfx::ColorWrite::ALL,
            color_buffer_format: gfx::CanvasColorBufferFormat::default(),
            sample_count: 1,
        }
    }
}

#[derive(Debug)]
pub struct RenderPipeline {
    pipeline: gfx::RenderPipeline,
    bind_group_layout: gfx::BindGroupLayout,
    sample_count: gfx::SampleCount,
    color_buffer_format: gfx::CanvasColorBufferFormat,
}

impl RenderPipeline {
    pub fn new(instance: &gfx::Instance, desc: &RenderPipelineDescriptor) -> Self {
        let bind_group_layout = bind_group_layout(instance);
        let pipeline_layout = gfx::PipelineLayout::new(
            instance,
            &gfx::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[gfx::PushConstantRange {
                    stages: gfx::ShaderStage::VERTEX,
                    range: 0..std::mem::size_of::<PushConstants>() as u32,
                }],
            },
        );
        let vs_module = gfx::ShaderModule::new(
            instance,
            gfx::include_spirv!("shaders/gen/spirv/text.vert.spv"),
        );
        let fs_module = gfx::ShaderModule::new(
            instance,
            gfx::include_spirv!("shaders/gen/spirv/text.frag.spv"),
        );
        let pipeline = gfx::RenderPipeline::new(
            instance,
            &gfx::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex_stage: gfx::ProgrammableStageDescriptor {
                    module: &vs_module,
                    entry_point: "main",
                },
                fragment_stage: Some(gfx::ProgrammableStageDescriptor {
                    module: &fs_module,
                    entry_point: "main",
                }),
                rasterization_state: Some(gfx::RasterizationStateDescriptor {
                    front_face: gfx::FrontFace::Ccw,
                    cull_mode: gfx::CullMode::Back,
                    ..Default::default()
                }),
                primitive_topology: gfx::PrimitiveTopology::TriangleList,
                color_states: &[gfx::ColorStateDescriptor {
                    format: gfx::TextureFormat::from(desc.color_buffer_format),
                    color_blend: desc.color_blend.clone(),
                    alpha_blend: desc.alpha_blend.clone(),
                    write_mask: desc.write_mask,
                }],
                depth_stencil_state: None,
                vertex_state: gfx::VertexStateDescriptor {
                    index_format: gfx::IndexFormat::Uint16,
                    vertex_buffers: &[gfx::VertexBufferDescriptor {
                        stride: std::mem::size_of::<Vertex>() as gfx::BufferAddress,
                        step_mode: gfx::InputStepMode::Vertex,
                        attributes: &[
                            gfx::VertexAttributeDescriptor {
                                format: gfx::VertexFormat::Float2,
                                offset: 0,
                                shader_location: 0,
                            },
                            gfx::VertexAttributeDescriptor {
                                format: gfx::VertexFormat::Float3,
                                offset: 8,
                                shader_location: 1,
                            },
                        ],
                    }],
                },
                sample_count: desc.sample_count,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            },
        );
        Self {
            pipeline,
            bind_group_layout,
            sample_count: desc.sample_count,
            color_buffer_format: desc.color_buffer_format,
        }
    }

    pub fn render_pass_requirements(&self) -> gfx::RenderPassRequirements {
        gfx::RenderPassRequirements {
            sample_count: self.sample_count,
            color_buffer_formats: vec![self.color_buffer_format],
            depth_stencil_buffer_format: None,
        }
    }
}

pub trait Renderer<'a> {
    fn draw_text(
        &mut self,
        pipeline: &'a RenderPipeline,
        font: &'a Font,
        text: &str,
        transform: geometry2::Transform<f32>,
    );
}

impl<'a> Renderer<'a> for gfx::RenderPass<'a> {
    fn draw_text(
        &mut self,
        pipeline: &'a RenderPipeline,
        font: &'a Font,
        text: &str,
        transform: geometry2::Transform<f32>,
    ) {
        let buffer = hb::UnicodeBuffer::new().add_str(text);
        let output = hb::shape(&font.hb_font, buffer, &[]);
        let positions = output.get_glyph_positions();
        let infos = output.get_glyph_infos();

        self.set_pipeline(&pipeline.pipeline);
        self.set_bind_group(0, &font.uniform_constants().bind_group, &[]);
        self.set_index_buffer(font.glyph_atlas_mesh.index_buffer().slice(..));
        self.set_vertex_buffer(0, font.glyph_atlas_mesh.vertex_buffer().slice(..));

        let mut cursor_pos = geometry2::Vector::new(0., 0.);
        for (position, info) in positions.iter().zip(infos) {
            let transform = transform
                * geometry2::Translation::from(
                    cursor_pos
                        + geometry2::Vector::new(
                            position.x_offset as f32 / 64.,
                            position.y_offset as f32 / 64.,
                        ),
                );
            let pc = PushConstants::new(&transform, gfx::ColorF32::WHITE);
            let range = font.glyph_map[&info.codepoint].clone();

            self.set_push_constants(gfx::ShaderStage::VERTEX, 0, pc.as_slice());
            self.draw_indexed(range, 0, 0..1);

            // TODO: we must check how to convert from the harfbuzz to the glsl scalar
            // values.
            // TODO: what should we use info.cluster for?
            cursor_pos.x = cursor_pos.x + position.x_advance as f32 / 64.;
            cursor_pos.y = cursor_pos.y + position.y_advance as f32 / 64.;
        }
    }
}

#[derive(Debug)]
pub struct Font {
    size: FontSize,
    hb_font: hb::Owned<hb::Font<'static>>,
    glyph_atlas: gfx::TextureView,
    glyph_atlas_sampler: gfx::Sampler,
    glyph_atlas_uniform: UniformConstants,
    glyph_atlas_mesh: Mesh,
    glyph_map: HashMap<u32, MeshIndexRange>,
}

impl Font {
    const RESOLUTION: u32 = 300;

    // TODO: generic iterator interface for characters.
    // TODO: make sure that the size computation is appropriate.
    // TODO: replace unwrap calls.
    // TODO: why is bytes per row proportional to the height rather than the width?
    pub fn new(instance: &gfx::Instance, face: &Face, size: FontSize, characters: &[char]) -> Self {
        // Setup harfbuzz font for future shaping.
        let mut hb_font = hb::Font::new(face.hb_face.clone());
        let ppem = size * Self::RESOLUTION / 72;
        hb_font.set_ppem(ppem, ppem);

        // Load glyphs.
        face.ft_face
            .set_char_size(0, size as isize, 0, Self::RESOLUTION)
            .unwrap();
        let mut glyphs = Vec::with_capacity(characters.len());
        for c in characters {
            face.ft_face
                .load_char(*c as usize, ft::face::LoadFlag::RENDER)
                .unwrap();
            glyphs.push((*c as u32, face.ft_face.glyph().bitmap()));
        }

        // Create the glyph atlas.
        let glyph_atlas_width = glyphs.iter().map(|x| x.1.width()).max().unwrap() as u32;
        let glyph_atlas_height = glyphs.iter().map(|x| x.1.rows()).max().unwrap() as u32;
        let glyph_atlas_depth = characters.len() as u32;
        let glyph_atlas_extent = gfx::Extent3d {
            width: glyph_atlas_width,
            height: glyph_atlas_height,
            depth: glyph_atlas_depth,
        };
        let glyph_atlas_slice_byte_count = (glyph_atlas_width * glyph_atlas_height) as usize;
        let glyph_atlas_byte_count = glyph_atlas_slice_byte_count * glyph_atlas_depth as usize;

        let mut glyph_atlas_buffer = vec![0; glyph_atlas_byte_count];
        let mut glyph_atlas_vertices = Vec::with_capacity(characters.len() * 4);
        let mut glyph_atlas_indices = Vec::with_capacity(characters.len() * 6);
        let mut glyph_map = HashMap::new();
        for (i, (c, g)) in glyphs.into_iter().enumerate() {
            let range_begin = i * glyph_atlas_slice_byte_count;
            let range_end = range_begin + (g.width() * g.rows()) as usize;
            glyph_atlas_buffer[range_begin..range_end].copy_from_slice(g.buffer());

            let gw = g.width() as f32;
            let gh = g.rows() as f32;
            let tw = gw / glyph_atlas_width as f32;
            let th = gh / glyph_atlas_height as f32;
            let idx = i as f32;
            glyph_atlas_vertices.extend_from_slice(&[
                Vertex::new([0., 0.], [0., 0., idx]),
                Vertex::new([0., gh], [0., th, idx]),
                Vertex::new([gw, gh], [tw, th, idx]),
                Vertex::new([gw, 0.], [tw, 0., idx]),
            ]);

            let vertices_begin = (i * 4) as MeshIndex;
            glyph_atlas_indices.extend_from_slice(&[
                vertices_begin,
                vertices_begin + 1,
                vertices_begin + 3,
                vertices_begin + 3,
                vertices_begin + 1,
                vertices_begin + 2,
            ]);

            let indices_begin = (i * 6) as u32;
            let indices_end = indices_begin + 6;
            glyph_map.insert(c, indices_begin..indices_end);
        }

        let glyph_atlas = gfx::Texture::new(
            instance,
            &gfx::TextureDescriptor {
                label: None,
                size: glyph_atlas_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: gfx::TextureDimension::D2,
                format: gfx::TextureFormat::R8Unorm,
                usage: gfx::TextureUsage::SAMPLED | gfx::TextureUsage::COPY_DST,
            },
        );
        glyph_atlas.write(
            instance,
            0,
            gfx::Origin3d::ZERO,
            glyph_atlas_buffer.as_slice(),
            gfx::TextureDataLayout {
                offset: 0,
                bytes_per_row: glyph_atlas_width,
                rows_per_image: glyph_atlas_height,
            },
            glyph_atlas_extent,
        );

        let glyph_atlas = glyph_atlas.create_view(&gfx::TextureViewDescriptor::default());
        let glyph_atlas_sampler = gfx::Sampler::new(instance, &gfx::SamplerDescriptor::default());
        let glyph_atlas_uniform =
            UniformConstants::new(instance, &glyph_atlas, &glyph_atlas_sampler);
        let glyph_atlas_mesh = Mesh::new(instance, &glyph_atlas_vertices, &glyph_atlas_indices);

        Self {
            size,
            hb_font,
            glyph_atlas,
            glyph_atlas_sampler,
            glyph_atlas_uniform,
            glyph_atlas_mesh,
            glyph_map,
        }
    }

    pub fn size(&self) -> FontSize {
        self.size
    }

    pub fn uniform_constants(&self) -> &UniformConstants {
        &self.glyph_atlas_uniform
    }
}

pub struct CharacterSet {}

impl CharacterSet {
    pub fn english() -> Vec<char> {
        (0x0000u32..0x007fu32)
            .map(|x| std::char::from_u32(x).expect("Invalid Unicode codepoint"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FONT_PATH: &str = "src/data/Roboto-Regular.ttf";

    #[test]
    fn create_library() {
        let _lib = FontLibrary::new().unwrap();
    }

    #[test]
    fn create_face() {
        let lib = FontLibrary::new().unwrap();
        let _face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
    }

    #[test]
    fn create_font() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let _font = Font::new(&instance, &face, 12, &['a', 'Z', '2', '#']);
    }

    #[test]
    fn create_english_font() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let _font = Font::new(&instance, &face, 12, CharacterSet::english().as_slice());
    }
}
