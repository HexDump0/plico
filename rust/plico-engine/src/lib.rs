mod bindings;
mod compression;
mod documents;
mod images;
mod security;
mod stamps;
#[cfg(test)]
mod tests;

pub use bindings::{
    add_page_numbers, add_watermark, compress_pdf, images_to_pdf, merge_pdfs, organize_pdfs,
    pdf_protection, protect_pdf, split_pdf_every, split_pdf_ranges, unlock_pdf,
};
pub use compression::{CompressOptions, compress_pdf_bytes, compress_pdf_bytes_with_password};
pub use documents::{
    OrganizeItem, SplitMode, merge_pdf_bytes, merge_pdf_bytes_with_options,
    merge_pdf_bytes_with_passwords, organize_pdf_bytes, organize_pdf_items, organize_pdfs_bytes,
    organize_pdfs_bytes_with_passwords, split_pdf_bytes, split_pdf_bytes_with_password,
};
pub use images::{ImagePdfOptions, PageOrientation, images_to_pdf_bytes};
pub use security::{
    ProtectOptions, Protection, protect_pdf_bytes, protection_of, unlock_pdf_bytes,
};
pub use stamps::{
    FontFamily, PageNumberOptions, Position, TextStyle, WatermarkContent, WatermarkOptions,
    add_page_numbers_bytes, add_watermark_bytes,
};
