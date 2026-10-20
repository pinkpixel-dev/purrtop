//! What kind of file something is, going by its extension.

/// A broad file kind, used for colors and the Types tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Video,
    Image,
    Audio,
    Archive,
    DiskImage,
    Document,
    Code,
    Binary,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Video => "Video",
            Kind::Image => "Image",
            Kind::Audio => "Audio",
            Kind::Archive => "Archive",
            Kind::DiskImage => "Disk image",
            Kind::Document => "Document",
            Kind::Code => "Code",
            Kind::Binary => "Binary",
            Kind::Other => "Other",
        }
    }

    /// The kind for a lowercase extension without the dot.
    pub fn of(extension: &str) -> Kind {
        match extension {
            "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "wmv" | "flv" | "mpg" | "mpeg" | "m2ts" | "3gp" => Kind::Video,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "heic" | "heif" | "bmp" | "tif" | "tiff" | "svg" | "ico" | "psd" | "xcf" | "kra" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "exr" | "hdr" => {
                Kind::Image
            }
            "mp3" | "flac" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "wma" | "aiff" | "alac" | "mid" | "midi" => Kind::Audio,
            "zip" | "tar" | "gz" | "tgz" | "xz" | "txz" | "zst" | "bz2" | "7z" | "rar" | "lz4" | "lzma" | "deb" | "rpm" | "apk" | "jar" | "whl" | "crate" => Kind::Archive,
            "iso" | "img" | "qcow2" | "vdi" | "vmdk" | "vhd" | "vhdx" | "appimage" | "flatpak" | "snap" | "squashfs" => Kind::DiskImage,
            "pdf" | "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "epub" | "xls" | "xlsx" | "ods" | "csv" | "ppt" | "pptx" | "odp" | "pages" | "tex" => Kind::Document,
            "rs" | "c" | "h" | "cpp" | "hpp" | "cc" | "py" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "go" | "java" | "kt" | "swift" | "rb" | "php" | "lua" | "sh" | "fish" | "zsh"
            | "html" | "css" | "scss" | "json" | "toml" | "yaml" | "yml" | "xml" | "sql" | "wasm" | "map" | "rlib" | "rmeta" | "o" | "pyc" => Kind::Code,
            "so" | "a" | "dll" | "dylib" | "node" | "bin" | "exe" | "pak" | "dat" | "db" | "sqlite" | "gguf" | "safetensors" | "onnx" | "pt" | "pth" | "ckpt" => Kind::Binary,
            _ => Kind::Other,
        }
    }
}

/// The lowercase extension of a file name, or `""` when it has none.
/// Dotfiles like `.bashrc` have no extension.
pub fn extension(name: &str) -> String {
    match name.rfind('.') {
        Some(0) | None => String::new(),
        Some(dot) if dot + 1 == name.len() => String::new(),
        Some(dot) => name[dot + 1..].to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_extensions() {
        assert_eq!(extension("movie.MKV"), "mkv");
        assert_eq!(extension("archive.tar.gz"), "gz");
        assert_eq!(extension(".bashrc"), "");
        assert_eq!(extension("Makefile"), "");
        assert_eq!(extension("odd."), "");
    }

    #[test]
    fn sorts_extensions_into_kinds() {
        assert_eq!(Kind::of("mkv"), Kind::Video);
        assert_eq!(Kind::of("jpeg"), Kind::Image);
        assert_eq!(Kind::of("iso"), Kind::DiskImage);
        assert_eq!(Kind::of("gguf"), Kind::Binary);
        assert_eq!(Kind::of(""), Kind::Other);
    }
}
