extern crate freetype as ft;
extern crate harfbuzz_rs as hb;

use std::fmt::Debug;

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

#[derive(Debug)]
pub struct Font {
    size: FontSize,
    hb_font: hb::Owned<hb::Font<'static>>,
    glyph_atlas: gfx::Texture,
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
        let max_glyph_width = glyphs.iter().map(|x| x.width()).max().unwrap() as u32;
        let max_glyph_height = glyphs.iter().map(|x| x.rows()).max().unwrap() as u32;
        let glyph_count = characters.len() as u32;
        let glyph_atlas_extent = gfx::Extent3d {
            width: max_glyph_width,
            height: max_glyph_height,
            depth: glyph_count,
        };

        let max_glyph_byte_count = (max_glyph_width * max_glyph_height) as usize;
        let byte_count = max_glyph_byte_count * glyph_count as usize;
        let mut glyph_atlas_buffer = vec![0; byte_count as usize];
        for (i, g) in glyphs.iter().enumerate() {
            let range_begin = i * max_glyph_byte_count;
            let range_end = range_begin + (g.width() * g.rows()) as usize;
            glyph_atlas_buffer[range_begin..range_end].copy_from_slice(g.buffer());
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
                bytes_per_row: max_glyph_width,
                rows_per_image: max_glyph_height,
            },
            glyph_atlas_extent,
        );

        Self {
            size,
            hb_font,
            glyph_atlas,
        }
    }

    pub fn size(&self) -> FontSize {
        self.size
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
        let _font = Font::new(&instance, &face, 12, &['a', 'B', '1', '#']);
    }
}
