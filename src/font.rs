extern crate freetype as ft;
extern crate harfbuzz_rs as hb;

use std::{collections::HashMap, fmt::Debug};

use rae_gfx::core as gfx;

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
pub struct FontVertex {
    position: [f32; 2],
    texture_coordinates: [f32; 2],
    glyph_index: u32,
}

impl FontVertex {
    pub fn new(position: [f32; 2], texture_coordinates: [f32; 2], glyph_index: u32) -> Self {
        Self {
            position,
            texture_coordinates,
            glyph_index,
        }
    }
}

unsafe impl bytemuck::Zeroable for FontVertex {
    fn zeroed() -> Self {
        Self::new([0., 0.], [0., 0.], 0)
    }
}

unsafe impl bytemuck::Pod for FontVertex {}

type FontMeshIndex = gfx::MeshIndex;
type FontMeshIndexRange = gfx::MeshIndexRange;
type FontMesh = gfx::IndexedMesh<FontVertex>;

#[derive(Debug)]
pub struct Font {
    size: FontSize,
    hb_font: hb::Owned<hb::Font<'static>>,
    glyph_atlas: gfx::Texture,
    glyph_atlas_mesh: FontMesh,
    glyph_map: HashMap<char, FontMeshIndexRange>,
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
            glyphs.push(face.ft_face.glyph().bitmap());
        }

        // Create the glyph atlas.
        let glyph_atlas_width = glyphs.iter().map(|x| x.width()).max().unwrap() as u32;
        let glyph_atlas_height = glyphs.iter().map(|x| x.rows()).max().unwrap() as u32;
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
        for (i, g) in glyphs.iter().enumerate() {
            let range_begin = i * glyph_atlas_slice_byte_count;
            let range_end = range_begin + (g.width() * g.rows()) as usize;
            glyph_atlas_buffer[range_begin..range_end].copy_from_slice(g.buffer());

            let gw = g.width() as f32;
            let gh = g.rows() as f32;
            let tw = gw / glyph_atlas_width as f32;
            let th = gh / glyph_atlas_height as f32;
            let idx = i as u32;
            glyph_atlas_vertices.extend_from_slice(&[
                FontVertex::new([0., 0.], [0., 0.], idx),
                FontVertex::new([0., gh], [0., th], idx),
                FontVertex::new([gw, gh], [tw, th], idx),
                FontVertex::new([gw, 0.], [tw, 0.], idx),
            ]);

            let vertices_begin = (i * 4) as FontMeshIndex;
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
            glyph_map.insert(characters[i], indices_begin..indices_end);
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

        let glyph_atlas_mesh = FontMesh::new(instance, &glyph_atlas_vertices, &glyph_atlas_indices);

        Self {
            size,
            hb_font,
            glyph_atlas,
            glyph_atlas_mesh,
            glyph_map,
        }
    }

    pub fn size(&self) -> FontSize {
        self.size
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
        let _font = Font::new(&instance, &face, 12, CharacterSet::english().as_slice());
    }
}
