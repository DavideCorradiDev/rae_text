extern crate freetype as ft;
extern crate harfbuzz_rs as hb;

use std::{collections::HashMap, fmt::Debug};

use rae_gfx::core as gfx;
use rae_math::geometry2;

use super::{Mesh, MeshIndex, MeshIndexRange, UniformConstants, Vertex};

pub use ft::{Error as FontError, FtResult as FontResult};
pub use hb::GlyphBuffer as TextShapingInfo;

pub type FaceIndex = u32;
pub type CharIndex = u32;

pub type FontSize = f32;
pub type FontResolution = u32;
pub type I26Dot6Size = i32;
pub type PpemSize = i32;

pub fn i26dot6_to_fsize(x: I26Dot6Size) -> FontSize {
    x as FontSize / 64.
}

pub fn fsize_to_i26dot6(x: FontSize) -> I26Dot6Size {
    (x * 64.) as I26Dot6Size
}

pub fn i26dot6_to_ppem(x: I26Dot6Size, res: FontResolution) -> PpemSize {
    x * res as I26Dot6Size / 72
}

pub fn ppem_to_i26dot6(x: PpemSize, res: FontResolution) -> I26Dot6Size {
    x * 72 / res as I26Dot6Size
}

pub fn fsize_to_ppem(x: FontSize, res: FontResolution) -> PpemSize {
    i26dot6_to_ppem(fsize_to_i26dot6(x), res)
}

pub fn ppem_to_fsize(x: PpemSize, res: FontResolution) -> FontSize {
    i26dot6_to_fsize(ppem_to_i26dot6(x, res))
}

// TODO: hide the library?
// TODO: replace unwrap calls with proper error handling.
// TODO: add error case tests.
// TODO: open ticket for better font size typing.
// TODO: improve the rendering interface.
// TODO: test different font sizes.
// TODO: test with non-latin font.
// TODO: right-left / top-down text.
// TODO: formatted text rendering.
// TODO: newline, text breaks.
// TODO: unknown character placeholder rendering.

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

    pub fn load_char(&self, c: char) -> &ft::GlyphSlot {
        self.ft_face
            .load_char(c as usize, ft::face::LoadFlag::RENDER)
            .unwrap();
        self.ft_face.glyph()
    }

    pub fn char_index(&self, c: char) -> CharIndex {
        self.ft_face.get_char_index(c as usize)
    }
}

#[derive(Debug)]
struct Glyph {
    char_index: CharIndex,
    pixels: Vec<u8>,
    left: i32,
    top: i32,
    width: i32,
    rows: i32,
}

impl Glyph {
    fn new(face: &Face, c: char) -> Self {
        let char_index = face.char_index(c);
        let glyph = face.load_char(c);
        let bitmap = glyph.bitmap();
        Glyph {
            char_index,
            pixels: Vec::from(bitmap.buffer()),
            left: glyph.bitmap_left(),
            top: glyph.bitmap_top(),
            width: bitmap.width(),
            rows: bitmap.rows(),
        }
    }
}

#[derive(Debug)]
struct GlyphSet {
    glyphs: Vec<Glyph>,
    extent: gfx::Extent3d,
}

impl GlyphSet {
    fn new(face: &Face, characters: &[char], size: FontSize, resolution: FontResolution) -> Self {
        face.ft_face
            .set_char_size(0, fsize_to_i26dot6(size) as isize, 0, resolution)
            .unwrap();
        let mut glyphs = Vec::with_capacity(characters.len());
        for c in characters {
            glyphs.push(Glyph::new(face, *c));
        }
        let extent = gfx::Extent3d {
            width: glyphs.iter().map(|x| x.width).max().unwrap() as u32,
            height: glyphs.iter().map(|x| x.rows).max().unwrap() as u32,
            depth: characters.len() as u32,
        };
        Self { glyphs, extent }
    }
}

#[derive(Debug)]
pub struct GlyphRenderingInfo {
    pub index_range: MeshIndexRange,
    pub bearing: geometry2::Vector<f32>,
}

#[derive(Debug)]
pub struct Font {
    size: FontSize,
    hb_font: hb::Owned<hb::Font<'static>>,
    glyph_atlas_texture: gfx::TextureView,
    glyph_atlas_sampler: gfx::Sampler,
    glyph_atlas_uniform: UniformConstants,
    glyph_atlas_mesh: Mesh,
    glyph_atlas_map: HashMap<CharIndex, GlyphRenderingInfo>,
}

impl Font {
    const RESOLUTION: FontResolution = 300;

    pub fn new(instance: &gfx::Instance, face: &Face, size: FontSize, characters: &[char]) -> Self {
        assert!(!characters.is_empty());
        assert!(size > 0.);

        let hb_font = Self::create_shaper(face, size);
        let glyph_set = GlyphSet::new(face, characters, size, Self::RESOLUTION);
        let glyph_atlas_texture = Self::create_glyph_atlas_texture(instance, &glyph_set);
        let glyph_atlas_sampler = gfx::Sampler::new(instance, &gfx::SamplerDescriptor::default());
        let glyph_atlas_uniform =
            UniformConstants::new(instance, &glyph_atlas_texture, &glyph_atlas_sampler);
        let glyph_atlas_mesh = Self::create_glyph_atlas_mesh(instance, &glyph_set);
        let glyph_atlas_map = Self::create_glyph_atlas_map(&glyph_set);

        Self {
            size,
            hb_font,
            glyph_atlas_texture,
            glyph_atlas_sampler,
            glyph_atlas_uniform,
            glyph_atlas_mesh,
            glyph_atlas_map,
        }
    }

    fn create_shaper(face: &Face, size: FontSize) -> hb::Owned<hb::Font<'static>> {
        let mut hb_font = hb::Font::new(face.hb_face.clone());
        let size_ppem = fsize_to_ppem(size, Self::RESOLUTION);
        hb_font.set_scale(size_ppem, size_ppem);
        hb_font
    }

    fn create_glyph_atlas_texture(
        instance: &gfx::Instance,
        glyph_set: &GlyphSet,
    ) -> gfx::TextureView {
        let glyph_atlas_row_byte_count = glyph_set.extent.width as usize;
        let glyph_atlas_slice_byte_count =
            (glyph_set.extent.width * glyph_set.extent.height) as usize;
        let glyph_atlas_byte_count = glyph_atlas_slice_byte_count * glyph_set.extent.depth as usize;

        let mut glyph_atlas_buffer = vec![0; glyph_atlas_byte_count];
        for (i, g) in glyph_set.glyphs.iter().enumerate() {
            let slice_begin = i * glyph_atlas_slice_byte_count;
            for row in 0..g.rows {
                let image_begin = slice_begin + row as usize * glyph_atlas_row_byte_count;
                let image_end = image_begin + g.width as usize;
                let pixels_begin = (row * g.width) as usize;
                let pixels_end = pixels_begin + g.width as usize;
                glyph_atlas_buffer[image_begin..image_end]
                    .copy_from_slice(&g.pixels[pixels_begin..pixels_end]);
            }
        }

        let glyph_atlas_texture = gfx::Texture::new(
            instance,
            &gfx::TextureDescriptor {
                label: None,
                size: glyph_set.extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: gfx::TextureDimension::D2,
                format: gfx::TextureFormat::R8Unorm,
                usage: gfx::TextureUsage::SAMPLED | gfx::TextureUsage::COPY_DST,
            },
        );
        glyph_atlas_texture.write(
            instance,
            0,
            gfx::Origin3d::ZERO,
            glyph_atlas_buffer.as_slice(),
            gfx::TextureDataLayout {
                offset: 0,
                bytes_per_row: glyph_set.extent.width,
                rows_per_image: glyph_set.extent.height,
            },
            glyph_set.extent,
        );
        glyph_atlas_texture.create_view(&gfx::TextureViewDescriptor::default())
    }

    fn create_glyph_atlas_mesh(instance: &gfx::Instance, glyph_set: &GlyphSet) -> Mesh {
        let mut glyph_atlas_vertices = Vec::with_capacity(glyph_set.glyphs.len() * 4);
        let mut glyph_atlas_indices = Vec::with_capacity(glyph_set.glyphs.len() * 6);
        for (i, g) in glyph_set.glyphs.iter().enumerate() {
            let gw = g.width as f32;
            let gh = g.rows as f32;
            let tw = gw / glyph_set.extent.width as f32;
            let th = gh / glyph_set.extent.height as f32;
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
        }
        Mesh::new(instance, &glyph_atlas_vertices, &glyph_atlas_indices)
    }

    fn create_glyph_atlas_map(glyph_set: &GlyphSet) -> HashMap<CharIndex, GlyphRenderingInfo> {
        let mut glyph_atlas_map = HashMap::new();
        for (i, g) in glyph_set.glyphs.iter().enumerate() {
            let indices_begin = (i * 6) as u32;
            let indices_end = indices_begin + 6 as u32;
            glyph_atlas_map.insert(
                g.char_index,
                GlyphRenderingInfo {
                    index_range: indices_begin..indices_end,
                    bearing: geometry2::Vector::new(g.left as f32, -g.top as f32),
                },
            );
        }
        glyph_atlas_map
    }

    pub fn size(&self) -> FontSize {
        self.size
    }

    pub fn shape_text(&self, text: &str) -> TextShapingInfo {
        let buffer = hb::UnicodeBuffer::new().add_str(text);
        hb::shape(&self.hb_font, buffer, &[])
    }

    pub fn glyph_rendering_info(&self, char_index: CharIndex) -> &GlyphRenderingInfo {
        &self.glyph_atlas_map[&char_index]
    }

    pub fn index_buffer(&self) -> &gfx::Buffer {
        self.glyph_atlas_mesh.index_buffer()
    }

    pub fn vertex_buffer(&self) -> &gfx::Buffer {
        self.glyph_atlas_mesh.vertex_buffer()
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
    use galvanic_assert::{matchers::*, *};

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
        let _font = Font::new(&instance, &face, 12., &['a', 'Z', '2', '#']);
    }

    #[test]
    fn font_size() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let font = Font::new(&instance, &face, 12., &['a', 'Z', '2', '#']);
        expect_that!(&font.size(), eq(12.));
    }

    #[test]
    fn font_shape_text() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let font = Font::new(&instance, &face, 12., &['a', 'Z', '2', '#']);
        let shaping = font.shape_text("aZzz");
        let positions = shaping.get_glyph_positions();
        let infos = shaping.get_glyph_infos();

        let expected_x_advance = [1740, 1915, 1585, 1585];
        let expected_y_advance = [0, 0, 0, 0];
        let expected_x_offset = [0, 0, 0, 0];
        let expected_y_offset = [0, 0, 0, 0];
        let expected_codepoint = [69, 62, 94, 94];

        for i in 0..positions.len() {
            expect_that!(&positions[i].x_advance, eq(expected_x_advance[i]));
            expect_that!(&positions[i].y_advance, eq(expected_y_advance[i]));
            expect_that!(&positions[i].x_offset, eq(expected_x_offset[i]));
            expect_that!(&positions[i].y_offset, eq(expected_y_offset[i]));
            expect_that!(&infos[i].codepoint, eq(expected_codepoint[i]));
        }
    }

    #[test]
    fn font_glyph_rendering_info() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let font = Font::new(&instance, &face, 12., &['a', 'Z', '2', 'p']);

        {
            let gri = font.glyph_rendering_info(face.char_index('a'));
            expect_that!(&gri.index_range, eq(0..6));
            expect_that!(&gri.bearing.x, close_to(2., 1e-6));
            expect_that!(&gri.bearing.y, close_to(-27., 1e-6));
        }
        {
            let gri = font.glyph_rendering_info(face.char_index('Z'));
            expect_that!(&gri.index_range, eq(6..12));
            expect_that!(&gri.bearing.x, close_to(2., 1e-6));
            expect_that!(&gri.bearing.y, close_to(-36., 1e-6));
        }
        {
            let gri = font.glyph_rendering_info(face.char_index('2'));
            expect_that!(&gri.index_range, eq(12..18));
            expect_that!(&gri.bearing.x, close_to(2., 1e-6));
            expect_that!(&gri.bearing.y, close_to(-36., 1e-6));
        }
        {
            let gri = font.glyph_rendering_info(face.char_index('p'));
            expect_that!(&gri.index_range, eq(18..24));
            expect_that!(&gri.bearing.x, close_to(3., 1e-6));
            expect_that!(&gri.bearing.y, close_to(-27., 1e-6));
        }
    }

    #[test]
    fn create_english_font() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let _font = Font::new(&instance, &face, 12., CharacterSet::english().as_slice());
    }
}
