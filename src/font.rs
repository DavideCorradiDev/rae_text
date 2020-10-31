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

// TODO: hide the library?

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
        let c = c as usize;
        face.ft_face
            .load_char(c, ft::face::LoadFlag::RENDER)
            .unwrap();
        let char_index = face.ft_face.get_char_index(c);
        let glyph = face.ft_face.glyph();
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
pub struct Font {
    size: FontSize,
    hb_font: hb::Owned<hb::Font<'static>>,
    glyph_atlas_texture: gfx::TextureView,
    glyph_atlas_sampler: gfx::Sampler,
    glyph_atlas_uniform: UniformConstants,
    glyph_atlas_mesh: Mesh,
    glyph_atlas_map: HashMap<u32, (MeshIndexRange, geometry2::Vector<f32>)>,
}

impl Font {
    const RESOLUTION: FontResolution = 300;

    // TODO: make sure that the size computation is appropriate.
    // TODO: replace unwrap calls.
    // TODO: why is bytes per row proportional to the height rather than the width?
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
        let size_ppem = i26dot6_to_ppem(fsize_to_i26dot6(size), Self::RESOLUTION);
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

    fn create_glyph_atlas_map(
        glyph_set: &GlyphSet,
    ) -> HashMap<u32, (MeshIndexRange, geometry2::Vector<f32>)> {
        let mut glyph_atlas_map = HashMap::new();
        for (i, g) in glyph_set.glyphs.iter().enumerate() {
            let indices_begin = (i * 6) as u32;
            let indices_end = indices_begin + 6;
            glyph_atlas_map.insert(
                g.char_index,
                (
                    indices_begin..indices_end,
                    geometry2::Vector::new(g.left as f32, -g.top as f32),
                ),
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

    pub fn glyph_info(&self, char_index: &CharIndex) -> &(MeshIndexRange, geometry2::Vector<f32>) {
        &self.glyph_atlas_map[char_index]
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
    fn create_english_font() {
        let instance = gfx::Instance::new(&gfx::InstanceDescriptor::default()).unwrap();
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, TEST_FONT_PATH, 0).unwrap();
        let _font = Font::new(&instance, &face, 12., CharacterSet::english().as_slice());
    }
}
