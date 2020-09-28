extern crate freetype as ft;
extern crate harfbuzz_rs as hb;

pub use std::fmt::Debug;

pub use ft::{Error as FontError, FtResult as FontResult};

pub type FaceIndex = u32;

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
pub struct Font {
    hb_font: hb::Owned<hb::Font<'static>>,
}

impl Font {
    pub fn new(face: &Face) -> Self {
        let hb_font = hb::Font::new(face.hb_face.clone());
        Self { hb_font }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_library() {
        let _lib = FontLibrary::new().unwrap();
    }

    #[test]
    fn create_face() {
        let lib = FontLibrary::new().unwrap();
        let _face = Face::from_file(&lib, "src/data/Roboto-Regular.ttf", 0).unwrap();
    }

    #[test]
    fn create_font() {
        let lib = FontLibrary::new().unwrap();
        let face = Face::from_file(&lib, "src/data/Roboto-Regular.ttf", 0).unwrap();
        let _font = Font::new(&face);
    }
}
