/// The Unix mode for private directories: owner only.
pub const DIRECTORY_PERMISSION: u32 = 0o700;
/// The Unix mode for written files: owner read and write, everyone else read.
pub const FILE_PERMISSION: u32 = 0o644;

/// The kind of a file-system entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// The kind of a written file, with an uppercase label from
/// [`FileType::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// An image of any format.
    Image,
    /// A JSON document.
    Json,
    /// A PNG image.
    Png,
    /// A thumbnail image.
    Thumbnail,
}

impl FileType {
    /// Returns the uppercase label, such as `"IMAGE"` or `"PNG"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "IMAGE",
            Self::Json => "JSON",
            Self::Png => "PNG",
            Self::Thumbnail => "THUMBNAIL",
        }
    }
}
