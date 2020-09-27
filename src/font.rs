pub use std::fmt::Debug;

pub use freetype::{Error as FontError, FtResult as FontResult};

pub type FaceIndex = isize;

pub struct FontLibrary {
    ft_lib: freetype::Library,
}

impl FontLibrary {
    pub fn new() -> FontResult<Self> {
        let ft_lib = freetype::Library::init()?;
        Ok(Self { ft_lib })
    }
}

impl Debug for FontLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "FontLibrary {{ }}")
    }
}

#[derive(Debug)]
pub struct FontFace {
    ft_face: freetype::Face,
}

impl FontFace {
    pub fn from_file<P: AsRef<std::ffi::OsStr>>(
        lib: &FontLibrary,
        path: P,
        face_index: FaceIndex,
    ) -> FontResult<Self> {
        let ft_face = lib.ft_lib.new_face(path, face_index)?;
        Ok(Self { ft_face })
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
    fn create_font_face() {
        let lib = FontLibrary::new().unwrap();
        let _font_face = FontFace::from_file(&lib, "src/data/Roboto-Regular.ttf", 0).unwrap();
    }
}
