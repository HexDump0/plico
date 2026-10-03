mod archive;
mod bindings;
mod compression;
mod crop;
mod documents;
mod flatten;
mod images;
mod redact;
mod security;
mod stamps;
#[cfg(test)]
mod tests;

pub use archive::{PdfALevel, StandardFont, convert_to_pdfa_bytes, standard_fonts_for_pdfa};
pub use bindings::{
    add_page_numbers, add_watermark, compress_pdf, convert_to_pdfa, crop_pdf, images_to_pdf,
    merge_pdfs, organize_pdfs, pdf_protection, pdfa_standard_fonts, protect_pdf, redact_pdf,
    redaction_text, sign_pdf, split_pdf_every, split_pdf_ranges, unlock_pdf,
};
pub use compression::{CompressOptions, compress_pdf_bytes, compress_pdf_bytes_with_password};
pub use crop::{PageCrop, crop_pdf_bytes};
pub use documents::{
    OrganizeItem, SplitMode, merge_pdf_bytes, merge_pdf_bytes_with_options,
    merge_pdf_bytes_with_passwords, organize_pdf_bytes, organize_pdf_items, organize_pdfs_bytes,
    organize_pdfs_bytes_with_passwords, split_pdf_bytes, split_pdf_bytes_with_password,
};
pub use flatten::{FlattenScope, Flattened, flatten_pdf_bytes};
pub use images::{ImagePdfOptions, PageOrientation, images_to_pdf_bytes};
pub use redact::{
    PageImage, PageText, RedactOptions, Redacted, Redaction, Unremovable, page_texts,
    redact_pdf_bytes,
};
pub use security::{
    ProtectOptions, Protection, protect_pdf_bytes, protection_of, unlock_pdf_bytes,
};
pub use stamps::{
    FontFamily, PageNumberOptions, Position, SignaturePlacement, TextStyle, WatermarkContent,
    WatermarkOptions, add_page_numbers_bytes, add_signature_bytes, add_watermark_bytes,
};
