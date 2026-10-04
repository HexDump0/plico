use std::collections::{BTreeMap, BTreeSet};

use lopdf::content::Content;
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, ObjectId, Permissions,
    Stream, dictionary,
};

use super::{
    CompressOptions, ImagePdfOptions, OrganizeItem, PageOrientation, SplitMode, compress_pdf_bytes,
    compress_pdf_bytes_with_password, images_to_pdf_bytes, merge_pdf_bytes,
    merge_pdf_bytes_with_options, merge_pdf_bytes_with_passwords, organize_pdf_bytes,
    organize_pdf_items, organize_pdfs_bytes, organize_pdfs_bytes_with_passwords, split_pdf_bytes,
    split_pdf_bytes_with_password, unlock_pdf_bytes,
};
use crate::archive::xmp_date;
use crate::compression::{deflate_best, filter_matches, image_transcode::is_jpeg_image};
use crate::documents::parse_version;
use crate::images::jpeg_orientation;
use crate::{
    FlattenScope, FontFamily, PageCrop, PageNumberOptions, PdfALevel, Position, ProtectOptions,
    Protection, SignaturePlacement, StandardFont, TextStyle, WatermarkContent, WatermarkOptions,
    add_page_numbers_bytes, add_signature_bytes, add_watermark_bytes, convert_to_pdfa_bytes,
    crop_pdf_bytes, flatten_pdf_bytes, protect_pdf_bytes, protection_of, standard_fonts_for_pdfa,
};
use crate::{OcrPage, OcrWord, add_text_layer_bytes, page_texts as glyph_texts};

fn image_pdf_options() -> ImagePdfOptions {
    ImagePdfOptions {
        page_width: 595.0,
        page_height: 842.0,
        margin: 18.0,
        orientation: PageOrientation::Auto,
    }
}

fn rgba_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[255, 0, 0, 255, 0, 0, 255, 0])
            .unwrap();
    }
    bytes
}

#[test]
fn image_pdf_keeps_jpeg_bytes_and_input_order() {
    let first = jpeg_bytes(3, 2, 90);
    let second = jpeg_bytes(2, 3, 90);
    let output = images_to_pdf_bytes(&[&first, &second], image_pdf_options()).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let pages = document.get_pages();
    assert_eq!(pages.len(), 2);
    for (page, original) in pages.into_values().zip([first, second]) {
        let xobjects = document.get_page_resources(page).unwrap().0.unwrap();
        let id = xobjects
            .get(b"XObject")
            .and_then(Object::as_dict)
            .unwrap()
            .get(b"Im0")
            .and_then(Object::as_reference)
            .unwrap();
        let image = document.get_object(id).and_then(Object::as_stream).unwrap();
        assert_eq!(image.content, original);
        assert_eq!(
            image.dict.get(b"Filter").and_then(Object::as_name).unwrap(),
            b"DCTDecode"
        );
    }
}

#[test]
fn image_pdf_preserves_png_alpha_as_soft_mask() {
    let png = rgba_png();
    let output = images_to_pdf_bytes(&[&png], image_pdf_options()).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let image = document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .find(|stream| stream.dict.get(b"SMask").is_ok())
        .unwrap();
    assert_eq!(
        image.decompressed_content().unwrap(),
        [255, 0, 0, 0, 0, 255]
    );
    let mask_id = image
        .dict
        .get(b"SMask")
        .and_then(Object::as_reference)
        .unwrap();
    let mask = document
        .get_object(mask_id)
        .and_then(Object::as_stream)
        .unwrap();
    assert_eq!(mask.decompressed_content().unwrap(), [255, 0]);
}

#[test]
fn image_pdf_honors_exif_orientation() {
    let jpeg = jpeg_bytes(3, 2, 90);
    let mut oriented = jpeg[..2].to_vec();
    oriented.extend_from_slice(&[
        0xff, 0xe1, 0, 34, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12,
        0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ]);
    oriented.extend_from_slice(&jpeg[2..]);
    assert_eq!(jpeg_orientation(&oriented), 6);
    let output = images_to_pdf_bytes(&[&oriented], image_pdf_options()).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let page = *document.get_pages().values().next().unwrap();
    let media_box = document
        .get_dictionary(page)
        .unwrap()
        .get(b"MediaBox")
        .and_then(Object::as_array)
        .unwrap();
    assert_eq!(media_box[2].as_float().unwrap(), 595.0);
    let content = String::from_utf8(document.get_page_content(page)).unwrap();
    assert!(content.contains("0.0000 -"));
}

fn one_page_pdf(label: &str) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, label.as_bytes().to_vec()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

/// Puts /MediaBox and /Resources on the page tree node rather than on the
/// page, which is legal and is what most report generators emit.
fn inheriting_pdf(label: &str, width: i64) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let content_id = document.add_object(Stream::new(dictionary! {}, label.as_bytes().to_vec()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), 800.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn numbered_pdf(count: u32) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let mut kids = Vec::new();
    for number in 1..=count {
        let content_id =
            document.add_object(Stream::new(dictionary! {}, number.to_string().into_bytes()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        kids.push(Object::Reference(page_id));
    }
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => count,
            "MediaBox" => vec![0.into(), 0.into(), 400.into(), 600.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn page_contents(bytes: &[u8]) -> Vec<String> {
    let document = Document::load_mem(bytes).unwrap();
    document
        .get_pages()
        .into_values()
        .map(|page| {
            String::from_utf8_lossy(&document.get_page_content(page))
                .trim()
                .to_owned()
        })
        .collect()
}

fn page_widths(document: &Document) -> Vec<i64> {
    document
        .get_pages()
        .into_values()
        .map(|id| {
            document
                .get_dictionary(id)
                .and_then(|page| page.get(b"MediaBox"))
                .and_then(Object::as_array)
                .map(|media_box| media_box[2].as_i64().unwrap_or(-1))
                .unwrap_or(-1)
        })
        .collect()
}

#[test]
fn merges_pages_in_input_order() {
    let first = one_page_pdf("first");
    let second = one_page_pdf("second");
    let merged = merge_pdf_bytes(&[&first, &second]).unwrap();
    let document = Document::load_mem(&merged).unwrap();
    let pages = document.get_pages();

    assert_eq!(pages.len(), 2);
    let content = pages
        .into_values()
        .map(|page| document.get_page_content(page))
        .collect::<Vec<_>>();
    assert_eq!(content, [b"first\n".to_vec(), b"second\n".to_vec()]);
}

#[test]
fn rejects_invalid_input() {
    let error = merge_pdf_bytes(&[b"not a pdf", b"also not a pdf"]).unwrap_err();
    assert!(error.starts_with("PDF 1 could not be read:"));
}

#[test]
fn requires_two_documents() {
    let pdf = one_page_pdf("only");
    assert_eq!(
        merge_pdf_bytes(&[&pdf]).unwrap_err(),
        "Choose at least two PDFs to merge."
    );
}

#[test]
fn keeps_each_page_geometry_when_inherited() {
    let narrow = inheriting_pdf("narrow", 200);
    let wide = inheriting_pdf("wide", 600);
    let merged = merge_pdf_bytes(&[&narrow, &wide]).unwrap();
    let document = Document::load_mem(&merged).unwrap();

    assert_eq!(page_widths(&document), vec![200, 600]);
}

#[test]
fn keeps_inherited_resources() {
    let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &inheriting_pdf("b", 300)]).unwrap();
    let document = Document::load_mem(&merged).unwrap();

    let second = document.get_pages()[&2];
    let resources = document
        .get_dictionary(second)
        .and_then(|page| page.get_deref(b"Resources", &document))
        .and_then(Object::as_dict)
        .expect("appended page kept the resources it inherited");
    assert!(resources.has(b"Font"));
}

/// /MediaBox is required. The merged root carries no default, so every page
/// has to resolve one on its own.
#[test]
fn every_page_defines_its_own_mediabox() {
    let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &inheriting_pdf("b", 612)]).unwrap();
    let document = Document::load_mem(&merged).unwrap();

    assert_eq!(page_widths(&document), vec![200, 612]);
}

#[test]
fn drops_unreachable_objects() {
    let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &one_page_pdf("b")]).unwrap();
    let mut document = Document::load_mem(&merged).unwrap();

    let types = document
        .objects
        .iter()
        .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
        .collect::<BTreeMap<_, _>>();
    // startxref reaches the cross-reference stream, not the object graph, and
    // the reader keeps the object streams it unpacked, so those always look
    // unreferenced.
    let leaked = document
        .prune_objects()
        .into_iter()
        .filter(|id| !matches!(types[id].as_slice(), b"XRef" | b"ObjStm"))
        .collect::<Vec<_>>();

    assert!(leaked.is_empty(), "unreachable objects kept: {leaked:?}");
}

/// Page order has to come from the page tree, not from object numbering.
#[test]
fn keeps_page_order_when_ids_run_backwards() {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    // Allocate the second page first, so it takes the lower object number.
    let second_content = document.add_object(Stream::new(dictionary! {}, b"second".to_vec()));
    let second = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => second_content,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
    });
    let first_content = document.add_object(Stream::new(dictionary! {}, b"first".to_vec()));
    let first = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => first_content,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 300.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![first.into(), second.into()],
            "Count" => 2,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut reversed = Vec::new();
    document.save_to(&mut reversed).unwrap();

    let merged = merge_pdf_bytes(&[&reversed, &one_page_pdf("third")]).unwrap();
    let document = Document::load_mem(&merged).unwrap();
    let content = document
        .get_pages()
        .into_values()
        .map(|page| {
            String::from_utf8_lossy(&document.get_page_content(page))
                .trim()
                .to_owned()
        })
        .collect::<Vec<_>>();

    assert_eq!(content, ["first", "second", "third"]);
}

/// A page tree listing a kid that was never written. Compacting object ids
/// must not let that reference drift onto a real object: pointed at the page
/// tree itself it forms a cycle, and every page in the document disappears.
#[test]
fn survives_a_page_tree_kid_that_does_not_exist() {
    let mut document = Document::with_version("1.4");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, b"real".to_vec()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            // (9, 0) is never written.
            "Kids" => vec![Object::Reference((9, 0)), page_id.into()],
            "Count" => 2,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut broken = Vec::new();
    document.save_to(&mut broken).unwrap();

    let merged = merge_pdf_bytes(&[&broken, &one_page_pdf("second")]).unwrap();
    let document = Document::load_mem(&merged).unwrap();

    assert_eq!(document.get_pages().len(), 2);
}

#[test]
fn merges_more_than_two_documents() {
    let files = [
        one_page_pdf("a"),
        one_page_pdf("b"),
        one_page_pdf("c"),
        one_page_pdf("d"),
    ];
    let refs = files.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let merged = merge_pdf_bytes(&refs).unwrap();

    assert_eq!(Document::load_mem(&merged).unwrap().get_pages().len(), 4);
}

#[test]
fn organizes_pages_across_documents_in_requested_order() {
    let first = numbered_pdf(2);
    let second = numbered_pdf(2);
    let output =
        organize_pdfs_bytes(&[&first, &second], &[(1, 2, 0), (0, 1, 90), (1, 1, 180)]).unwrap();

    assert_eq!(page_contents(&output), ["2", "1", "1"]);

    let document = Document::load_mem(&output).unwrap();
    let rotations = document
        .get_pages()
        .into_values()
        .map(|id| {
            document
                .get_dictionary(id)
                .and_then(|page| page.get(b"Rotate"))
                .and_then(Object::as_i64)
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    assert_eq!(rotations, [0, 90, 180]);
}

#[test]
fn keeps_rotation_of_a_page_that_already_had_one() {
    let mut document = Document::load_mem(&numbered_pdf(1)).unwrap();
    let page = *document.get_pages().values().next().unwrap();
    document.get_dictionary_mut(page).unwrap().set("Rotate", 90);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();

    let output = organize_pdf_bytes(&bytes, &[(1, 90)]).unwrap();
    let result = Document::load_mem(&output).unwrap();
    let page = *result.get_pages().values().next().unwrap();
    assert_eq!(
        result
            .get_dictionary(page)
            .and_then(|page| page.get(b"Rotate"))
            .and_then(Object::as_i64)
            .unwrap(),
        180
    );
}

#[test]
fn organize_rejects_duplicate_pages_bad_sources_and_turns() {
    let input = numbered_pdf(2);
    assert!(organize_pdf_bytes(&input, &[(1, 0), (1, 90)]).is_err());
    assert!(organize_pdf_bytes(&input, &[(3, 0)]).is_err());
    assert!(organize_pdf_bytes(&input, &[]).is_err());
    assert!(organize_pdfs_bytes(&[&input], &[(1, 0, 45)]).is_err());
    assert!(organize_pdfs_bytes(&[], &[]).is_err());
    assert!(organize_pdfs_bytes(&[&input], &[(0, 1, 0), (1, 1, 0)]).is_err());
}

#[test]
fn splits_ranges_into_separate_pdfs() {
    let outputs = split_pdf_bytes(
        &numbered_pdf(5),
        SplitMode::Ranges(&[(2, 3), (5, 5)], false),
    )
    .unwrap();

    assert_eq!(outputs.len(), 2);
    assert_eq!(page_contents(&outputs[0]), ["2", "3"]);
    assert_eq!(page_contents(&outputs[1]), ["5"]);

    let document = Document::load_mem(&outputs[0]).unwrap();
    assert_eq!(page_widths(&document), vec![400, 400]);
    let first = document.get_pages()[&1];
    let resources = document
        .get_dictionary(first)
        .and_then(|page| page.get_deref(b"Resources", &document))
        .and_then(Object::as_dict)
        .unwrap();
    assert!(resources.has(b"Font"));
}

#[test]
fn combined_ranges_keep_order_and_include_overlaps_once() {
    let outputs =
        split_pdf_bytes(&numbered_pdf(5), SplitMode::Ranges(&[(3, 4), (1, 3)], true)).unwrap();

    assert_eq!(outputs.len(), 1);
    assert_eq!(page_contents(&outputs[0]), ["3", "4", "1", "2"]);
}

#[test]
fn splits_every_n_pages_with_a_short_final_part() {
    let outputs = split_pdf_bytes(&numbered_pdf(5), SplitMode::Every(2)).unwrap();

    assert_eq!(outputs.len(), 3);
    assert_eq!(page_contents(&outputs[0]), ["1", "2"]);
    assert_eq!(page_contents(&outputs[1]), ["3", "4"]);
    assert_eq!(page_contents(&outputs[2]), ["5"]);
}

#[test]
fn split_rejects_invalid_ranges_and_intervals() {
    let input = numbered_pdf(3);
    assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[], false)).is_err());
    assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[(0, 2)], false)).is_err());
    assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[(3, 2)], false)).is_err());
    assert!(split_pdf_bytes(&input, SplitMode::Ranges(&[(1, 4)], false)).is_err());
    assert!(split_pdf_bytes(&input, SplitMode::Every(0)).is_err());
}

#[test]
fn split_output_drops_unreachable_pages_and_objects() {
    let outputs = split_pdf_bytes(&numbered_pdf(5), SplitMode::Ranges(&[(3, 3)], false)).unwrap();
    let mut document = Document::load_mem(&outputs[0]).unwrap();
    let types = document
        .objects
        .iter()
        .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
        .collect::<BTreeMap<_, _>>();
    let leaked = document
        .prune_objects()
        .into_iter()
        .filter(|id| !matches!(types[id].as_slice(), b"XRef" | b"ObjStm"))
        .collect::<Vec<_>>();

    assert_eq!(page_contents(&outputs[0]), ["3"]);
    assert!(leaked.is_empty(), "unreachable objects kept: {leaked:?}");
}

fn jpeg_bytes(width: u16, height: u16, quality: u8) -> Vec<u8> {
    // A smooth gradient encodes compactly; scale it to spread the data.
    let components = 3usize;
    let mut pixels = Vec::with_capacity(width as usize * height as usize * components);
    for y in 0..height {
        for x in 0..width {
            pixels.push((x as u32 * 3 % 256) as u8);
            pixels.push((y as u32 * 7 % 256) as u8);
            pixels.push((x as u32 + y as u32) as u8);
        }
    }
    let mut encoded = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut encoded, quality);
    encoder.set_chroma_subsampling_method(jpeg_encoder::ChromaSubsamplingMethod::Nearest);
    encoder
        .encode(&pixels, width, height, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    encoded
}

/// A page whose only content is one image XObject holding the given JPEG.
fn jpeg_pdf(jpeg: &[u8], width: i64, height: i64) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width,
            "Height" => height,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg.to_vec(),
    ));
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        format!("q {width} 0 0 {height} 0 0 cm /Im1 Do Q").into_bytes(),
    ));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn compress_options(
    image_quality: u8,
    max_image_dimension: u32,
    remove_metadata: bool,
    remove_thumbnails: bool,
) -> CompressOptions {
    CompressOptions {
        reflate: true,
        image_quality,
        max_image_dimension,
        remove_metadata,
        remove_thumbnails,
    }
}

/// The stream holding the first page's decoded content after a compress.
fn image_stream(document: &Document) -> (i64, i64, &[u8]) {
    let page = document.get_pages()[&1];
    let image = document
        .get_dictionary(page)
        .and_then(|page| page.get_deref(b"Resources", document))
        .and_then(|resources| resources.as_dict())
        .and_then(|resources| resources.get_deref(b"XObject", document))
        .and_then(|xobjects| xobjects.as_dict())
        .and_then(|xobjects| xobjects.get_deref(b"Im1", document))
        .and_then(|object| object.as_stream())
        .unwrap();
    let width = image.dict.get(b"Width").and_then(Object::as_i64).unwrap();
    let height = image.dict.get(b"Height").and_then(Object::as_i64).unwrap();
    (width, height, &image.content)
}
#[test]
fn compress_packs_uncompressed_streams() {
    // lopdf only packs a stream when it saves meaningfully, so give the
    // content some bulk.
    let input = one_page_pdf(&"sample".repeat(400));
    let compressed = compress_pdf_bytes(&input, compress_options(0, 0, false, false)).unwrap();
    let document = Document::load_mem(&compressed).unwrap();
    let pages = document.get_pages();

    assert_eq!(pages.len(), 1);
    assert_eq!(
        document.get_page_content(pages[&1]),
        format!("{}\n", "sample".repeat(400)).into_bytes()
    );
    let has_flate = document.objects.values().any(|object| match object {
        Object::Stream(stream) => filter_is_flate(stream),
        _ => false,
    });
    assert!(
        has_flate,
        "content stream did not gain a FlateDecode filter"
    );
    assert!(compressed.len() <= input.len(), "a bulky PDF inflated");
}

fn filter_is_flate(stream: &Stream) -> bool {
    matches!(stream.dict.get(b"Filter"), Ok(Object::Name(filter)) if filter == b"FlateDecode")
}

#[test]
fn compress_strips_metadata_and_thumbnails() {
    let mut document = Document::load_mem(&one_page_pdf("body")).unwrap();
    let metadata_id = document.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x/>".to_vec(),
    ));
    let thumb_id = document.add_object(Stream::new(dictionary! {}, vec![0u8; 512]));
    document.catalog_mut().unwrap().set("Metadata", metadata_id);
    let page_id = document.get_pages()[&1];
    document
        .get_dictionary_mut(page_id)
        .unwrap()
        .set("Thumb", thumb_id);
    let mut with_extras = Vec::new();
    document.save_to(&mut with_extras).unwrap();

    let compressed = compress_pdf_bytes(&with_extras, compress_options(0, 0, true, true)).unwrap();
    let mut document = Document::load_mem(&compressed).unwrap();

    assert!(document.catalog().unwrap().get(b"Metadata").is_err());
    assert!(document.trailer.get(b"Info").is_err());
    let page = document.get_pages()[&1];
    assert!(
        document
            .get_dictionary(page)
            .unwrap()
            .get(b"Thumb")
            .is_err()
    );

    let types = document
        .objects
        .iter()
        .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
        .collect::<BTreeMap<_, _>>();
    // startxref reaches the cross-reference stream, not the object graph,
    // so the reader always leaves the output's own XRef and ObjStm looking
    // unreferenced.
    let leaked = document
        .prune_objects()
        .into_iter()
        .filter(|id| types[id] != b"XRef" && types[id] != b"ObjStm")
        .collect::<Vec<_>>();
    assert!(leaked.is_empty(), "stripped streams kept: {leaked:?}");
}

#[test]
fn compress_transcodes_jpeg_images() {
    let original = jpeg_bytes(600, 400, 100);
    let input = jpeg_pdf(&original, 600, 400);
    let compressed = compress_pdf_bytes(&input, compress_options(60, 0, false, false)).unwrap();
    let document = Document::load_mem(&compressed).unwrap();

    let (width, height, content) = image_stream(&document);
    assert_eq!((width, height), (600, 400));
    assert!(
        content.len() < original.len(),
        "re-encoded JPEG not smaller"
    );
    // The replacement still decodes as a JPEG with the same dimensions.
    let mut decoder = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(content));
    decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    assert_eq!((info.width, info.height), (600, 400));
}

#[test]
fn compress_downscales_oversized_images() {
    let original = jpeg_bytes(3000, 800, 95);
    let input = jpeg_pdf(&original, 3000, 800);
    let compressed = compress_pdf_bytes(&input, compress_options(80, 2000, false, false)).unwrap();
    let document = Document::load_mem(&compressed).unwrap();

    let (width, height, content) = image_stream(&document);
    assert_eq!((width, height), (2000, 533));
    let mut decoder = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(content));
    let pixels = decoder.decode().unwrap();
    assert_eq!(pixels.len(), 2000 * 533 * 3);
    assert_eq!(document.get_pages().len(), 1);
}

#[test]
fn compress_downscales_through_a_reduced_decode_keeping_colours() {
    // Four flat quadrants; a reduced decode that mixed up channels or rows
    // would show in their centres.
    let (width, height) = (2400u16, 1600u16);
    let quadrant = |x: u16, y: u16| match (x < width / 2, y < height / 2) {
        (true, true) => [220, 40, 40],
        (false, true) => [40, 200, 60],
        (true, false) => [30, 60, 210],
        (false, false) => [240, 230, 30],
    };
    let pixels: Vec<u8> = (0..height)
        .flat_map(|y| (0..width).flat_map(move |x| quadrant(x, y)))
        .collect();
    for progressive in [false, true] {
        let mut jpeg = Vec::new();
        let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg, 100);
        encoder.set_progressive(progressive);
        encoder
            .encode(&pixels, width, height, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let input = jpeg_pdf(&jpeg, width.into(), height.into());
        let output = compress_pdf_bytes(&input, compress_options(90, 500, false, false)).unwrap();
        let document = Document::load_mem(&output).unwrap();
        let (new_width, new_height, content) = image_stream(&document);
        assert_eq!(
            (new_width, new_height),
            (500, 333),
            "progressive: {progressive}"
        );
        let decoded = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(content))
            .decode()
            .unwrap();
        for (x, y) in [(125u16, 83u16), (375, 83), (125, 250), (375, 250)] {
            let at = (usize::from(y) * 500 + usize::from(x)) * 3;
            let expected = quadrant(x * 24 / 5, y * 24 / 5);
            for (got, want) in decoded[at..at + 3].iter().zip(expected) {
                assert!(
                    got.abs_diff(want) < 12,
                    "progressive: {progressive}, at ({x}, {y}): {:?} for {expected:?}",
                    &decoded[at..at + 3]
                );
            }
        }
    }
}

#[test]
fn compress_downscales_grayscale_jpeg_without_panicking() {
    let width = 1500u16;
    let height = 1000u16;
    let pixels = (0..usize::from(width) * usize::from(height))
        .map(|index| ((index / usize::from(width) + index) % 256) as u8)
        .collect::<Vec<_>>();
    let mut jpeg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpeg, 95)
        .encode(&pixels, width, height, jpeg_encoder::ColorType::Luma)
        .unwrap();
    let mut source = Document::load_mem(&jpeg_pdf(&jpeg, width.into(), height.into())).unwrap();
    let image_id = source
        .objects
        .iter()
        .find_map(|(id, object)| {
            object
                .as_stream()
                .ok()
                .filter(|stream| is_jpeg_image(&stream.dict))
                .map(|_| *id)
        })
        .unwrap();
    source
        .get_object_mut(image_id)
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .dict
        .set("ColorSpace", "DeviceGray");
    let mut input = Vec::new();
    source.save_to(&mut input).unwrap();

    let output = compress_pdf_bytes(&input, compress_options(60, 750, false, false)).unwrap();
    let result = Document::load_mem(&output).unwrap();
    assert_eq!(image_stream(&result).0, 750);
    assert_eq!(image_stream(&result).1, 500);
}

#[test]
fn compress_converts_large_flate_photo_to_jpeg() {
    let jpeg = jpeg_bytes(600, 400, 100);
    let mut decoder = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&jpeg));
    let pixels = decoder.decode().unwrap();
    let mut source = Document::load_mem(&jpeg_pdf(&jpeg, 600, 400)).unwrap();
    let image = source
        .objects
        .values_mut()
        .find_map(|object| {
            object
                .as_stream_mut()
                .ok()
                .filter(|stream| is_jpeg_image(&stream.dict))
        })
        .unwrap();
    image.dict.remove(b"Filter");
    image.set_content(pixels);
    image.compress().unwrap();
    let original_stream_size = image.content.len();
    let mut input = Vec::new();
    source.save_to(&mut input).unwrap();

    let output = compress_pdf_bytes(&input, compress_options(60, 0, false, false)).unwrap();
    let result = Document::load_mem(&output).unwrap();
    let (_, _, content) = image_stream(&result);
    assert!(content.len() < original_stream_size);
    assert!(result.objects.values().any(|object| {
        object.as_stream().ok().is_some_and(|stream| {
            stream
                .dict
                .get(b"Subtype")
                .is_ok_and(|value| value.as_name().is_ok_and(|name| name == b"Image"))
                && filter_matches(&stream.dict, b"DCTDecode")
        })
    }));
}

#[test]
fn compress_output_uses_object_streams() {
    // Enough dictionaries that packing them into object streams wins over
    // writing each one directly.
    let compressed =
        compress_pdf_bytes(&numbered_pdf(8), compress_options(0, 0, false, false)).unwrap();
    let document = Document::load_mem(&compressed).unwrap();

    assert!(
        document
            .objects
            .values()
            .any(|object| object.type_name().is_ok_and(|name| name == b"ObjStm")),
        "no object stream in output"
    );
    let (major, minor) = parse_version(&document.version);
    assert!((major, minor) >= (1, 5));
}
#[test]
fn compress_never_inflates_a_small_document() {
    let input = one_page_pdf("small");
    let compressed = compress_pdf_bytes(&input, compress_options(0, 0, false, false)).unwrap();
    assert!(compressed.len() <= input.len());
}

#[test]
fn compress_rejects_unreadable_input() {
    let error = compress_pdf_bytes(b"not a pdf", compress_options(0, 0, false, false)).unwrap_err();
    assert!(error.starts_with("PDF 1 could not be read:"));
}

#[test]
fn compress_deduplicates_identical_streams() {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();

    let stream_bytes = b"q /F1 12 Tf (Duplicate watermarked logo text) Tj Q ".repeat(20);
    let stream1_id = document.add_object(Stream::new(
        dictionary! { "Filter" => "FlateDecode" },
        deflate_best(&stream_bytes).unwrap(),
    ));
    let stream2_id = document.add_object(Stream::new(
        dictionary! { "Filter" => "FlateDecode" },
        deflate_best(&stream_bytes).unwrap(),
    ));
    assert_ne!(stream1_id, stream2_id);

    let page1_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => stream1_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });
    let page2_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => stream2_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    });

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page1_id.into(), page2_id.into()],
            "Count" => 2,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    let compressed = compress_pdf_bytes(&input, compress_options(0, 0, false, false)).unwrap();
    let output = Document::load_mem(&compressed).unwrap();
    let pages = output.get_pages();
    assert_eq!(pages.len(), 2);
    let expected_content = format!("{}\n", String::from_utf8_lossy(&stream_bytes)).into_bytes();
    let p1_content = output.get_page_content(pages[&1]);
    let p2_content = output.get_page_content(pages[&2]);
    assert_eq!(p1_content, expected_content);
    assert_eq!(p2_content, expected_content);

    // Both pages must point to the same content stream now
    let p1_contents_ref = output
        .get_dictionary(pages[&1])
        .unwrap()
        .get(b"Contents")
        .and_then(Object::as_reference)
        .unwrap();
    let p2_contents_ref = output
        .get_dictionary(pages[&2])
        .unwrap()
        .get(b"Contents")
        .and_then(Object::as_reference)
        .unwrap();
    assert_eq!(p1_contents_ref, p2_contents_ref);
}

#[test]
fn compress_recompresses_flate_stream_with_png_predictor() {
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    // Raw predictor stream: 1 byte filter prefix (0 = None) + 10 pixels of grayscale
    let raw_predictor_data = [0u8, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100].repeat(500);
    // Encode loosely with fast compression (level 1)
    let mut fast_encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    fast_encoder.write_all(&raw_predictor_data).unwrap();
    let fast_compressed = fast_encoder.finish().unwrap();

    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 10,
            "Height" => 500,
            "ColorSpace" => "DeviceGray",
            "BitsPerComponent" => 8,
            "Filter" => "FlateDecode",
            "DecodeParms" => dictionary! {
                "Predictor" => 15,
                "Columns" => 10,
                "Colors" => 1,
                "BitsPerComponent" => 8,
            },
        },
        fast_compressed.clone(),
    ));
    let content_id = document.add_object(Stream::new(
        dictionary! {},
        b"q 100 0 0 100 0 0 cm /Im0 Do Q".to_vec(),
    ));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    // Compress with lossless reflate
    let compressed = compress_pdf_bytes(&input, compress_options(0, 0, false, false)).unwrap();
    let result = Document::load_mem(&compressed).unwrap();
    let page = result.get_pages()[&1];
    let result_image = result
        .get_dictionary(page)
        .and_then(|p| p.get_deref(b"Resources", &result))
        .and_then(|r| r.as_dict())
        .and_then(|r| r.get_deref(b"XObject", &result))
        .and_then(|x| x.as_dict())
        .and_then(|x| x.get_deref(b"Im0", &result))
        .and_then(|o| o.as_stream())
        .unwrap();

    assert!(
        result_image.content.len() < fast_compressed.len(),
        "flate stream with predictor did not shrink"
    );
    assert!(result_image.dict.has(b"DecodeParms"));
    // The decompressed pixels must still be bit-for-bit identical
    let pixels = result_image.decompressed_content().unwrap();
    assert_eq!(pixels.len(), 10 * 500);
    assert_eq!(&pixels[..10], &[10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
}

#[test]
fn orders_versions_numerically() {
    assert!(parse_version("1.10") > parse_version("1.5"));
    assert!(parse_version("2.0") > parse_version("1.7"));
    assert_eq!(parse_version("nonsense"), (1, 0));
}

/// RC4 128-bit (revision 3), the most common legacy handler. An empty `user`
/// is the permissions-only case: it opens without asking, as viewers do.
fn locked_pdf(bytes: &[u8], user: &str) -> Vec<u8> {
    lock_with(bytes, |document| {
        EncryptionState::try_from(EncryptionVersion::V2 {
            document,
            owner_password: "owner",
            user_password: user,
            key_length: 128,
            permissions: Permissions::all(),
        })
        .unwrap()
    })
}

fn lock_with(bytes: &[u8], state: impl FnOnce(&Document) -> EncryptionState) -> Vec<u8> {
    let mut document = Document::load_mem(bytes).unwrap();
    document.trailer.set(
        "ID",
        vec![
            Object::string_literal(b"plico-fixture".to_vec()),
            Object::string_literal(b"plico-fixture".to_vec()),
        ],
    );
    let state = state(&document);
    document.encrypt(&state).unwrap();
    let mut output = Vec::new();
    document.save_to(&mut output).unwrap();
    output
}

#[test]
fn merges_a_locked_pdf_with_its_password_and_writes_it_unlocked() {
    let locked = locked_pdf(&numbered_pdf(3), "secret");
    let plain = one_page_pdf("first");
    let merged = merge_pdf_bytes_with_passwords(&[&locked, &plain], &["secret"]).unwrap();

    let document = Document::load_mem(&merged).unwrap();
    assert!(!document.is_encrypted());
    assert!(document.trailer.get(b"Encrypt").is_err());
    assert_eq!(page_contents(&merged), ["1", "2", "3", "first"]);
}

#[test]
fn names_the_locked_pdf_when_its_password_is_missing_or_wrong() {
    let locked = locked_pdf(&numbered_pdf(2), "secret");
    let plain = one_page_pdf("first");

    let missing = merge_pdf_bytes(&[&plain, &locked]).unwrap_err();
    assert!(missing.contains("PDF 2 is password protected"), "{missing}");

    let wrong = merge_pdf_bytes_with_passwords(&[&locked, &plain], &["nope"]).unwrap_err();
    assert!(
        wrong.contains("PDF 1 could not be unlocked with that password"),
        "{wrong}"
    );
}

#[test]
fn opens_a_permissions_only_pdf_without_asking() {
    let restricted = locked_pdf(&numbered_pdf(2), "");
    let plain = one_page_pdf("first");
    let merged = merge_pdf_bytes(&[&restricted, &plain]).unwrap();
    assert_eq!(page_contents(&merged), ["1", "2", "first"]);
}

#[test]
fn accepts_the_owner_password_for_split_and_organize() {
    let locked = locked_pdf(&numbered_pdf(3), "secret");

    let parts =
        split_pdf_bytes_with_password(&locked, "owner", SplitMode::Ranges(&[(2, 3)], false))
            .unwrap();
    assert_eq!(page_contents(&parts[0]), ["2", "3"]);

    let organized =
        organize_pdfs_bytes_with_passwords(&[&locked], &["secret"], &[(0, 3, 0), (0, 1, 0)])
            .unwrap();
    assert_eq!(page_contents(&organized), ["3", "1"]);
}

#[test]
fn compressing_a_locked_pdf_never_returns_the_locked_original() {
    let locked = locked_pdf(&numbered_pdf(1), "secret");
    let output = compress_pdf_bytes_with_password(
        &locked,
        "secret",
        CompressOptions {
            reflate: true,
            image_quality: 0,
            max_image_dimension: 0,
            remove_metadata: false,
            remove_thumbnails: false,
        },
    )
    .unwrap();
    assert!(!Document::load_mem(&output).unwrap().is_encrypted());
    assert_eq!(page_contents(&output), ["1"]);
}

/// Revision 2 recovers the user password with one RC4 pass instead of twenty.
#[test]
fn accepts_the_owner_password_of_a_revision_2_pdf() {
    let locked = lock_with(&numbered_pdf(2), |document| {
        EncryptionState::try_from(EncryptionVersion::V1 {
            document,
            owner_password: "owner",
            user_password: "secret",
            permissions: Permissions::all(),
        })
        .unwrap()
    });
    let parts = split_pdf_bytes_with_password(&locked, "owner", SplitMode::Every(1)).unwrap();
    assert_eq!(page_contents(&parts[1]), ["2"]);
}

#[test]
fn unlocking_keeps_every_page_and_drops_only_the_encryption() {
    let locked = locked_pdf(&numbered_pdf(3), "secret");
    let unlocked = unlock_pdf_bytes(&locked, "secret").unwrap();
    let document = Document::load_mem(&unlocked).unwrap();
    assert!(document.trailer.get(b"Encrypt").is_err());
    assert_eq!(page_contents(&unlocked), ["1", "2", "3"]);
    assert!(unlock_pdf_bytes(&locked, "").is_err());
}

fn media_box(document: &Document, page: lopdf::ObjectId) -> Vec<f32> {
    document
        .get_dictionary(page)
        .and_then(|page| page.get(b"MediaBox"))
        .and_then(Object::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_float().unwrap())
        .collect()
}

#[test]
fn merge_bookmarks_open_each_file_at_its_first_page() {
    let first = numbered_pdf(2);
    let second = one_page_pdf("second");
    let merged =
        merge_pdf_bytes_with_options(&[&first, &second], &[], Some(&["Report", "Été"])).unwrap();
    let document = Document::load_mem(&merged).unwrap();
    let pages = document.get_pages();
    let catalog = document.catalog().unwrap();
    assert_eq!(
        catalog.get(b"PageMode").and_then(Object::as_name).unwrap(),
        b"UseOutlines"
    );
    let outlines = document
        .get_dictionary(
            catalog
                .get(b"Outlines")
                .and_then(Object::as_reference)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(outlines.get(b"Count").and_then(Object::as_i64).unwrap(), 2);

    let mut item_id = outlines
        .get(b"First")
        .and_then(Object::as_reference)
        .unwrap();
    let mut seen = Vec::new();
    loop {
        let item = document.get_dictionary(item_id).unwrap();
        let target = item.get(b"Dest").and_then(Object::as_array).unwrap()[0]
            .as_reference()
            .unwrap();
        let number = pages.iter().find(|(_, id)| **id == target).map(|(n, _)| *n);
        let title = lopdf::decode_text_string(item.get(b"Title").unwrap()).unwrap();
        seen.push((title, number));
        match item.get(b"Next").and_then(Object::as_reference) {
            Ok(next) => item_id = next,
            Err(_) => break,
        }
    }
    assert_eq!(
        seen,
        [("Report".to_owned(), Some(1)), ("Été".to_owned(), Some(3))]
    );
}

#[test]
fn merges_without_an_outline_unless_asked() {
    let merged = merge_pdf_bytes(&[&one_page_pdf("a"), &one_page_pdf("b")]).unwrap();
    let document = Document::load_mem(&merged).unwrap();
    assert!(document.catalog().unwrap().get(b"Outlines").is_err());
}

#[test]
fn organize_inserts_blank_pages_where_asked() {
    let source = numbered_pdf(2);
    let output = organize_pdf_items(
        &[&source],
        &[],
        &[
            OrganizeItem::Blank {
                width: 300.0,
                height: 500.0,
                turn: 90,
            },
            OrganizeItem::Page {
                source: 0,
                number: 2,
                turn: 0,
            },
            OrganizeItem::Blank {
                width: 300.0,
                height: 500.0,
                turn: 0,
            },
            OrganizeItem::Page {
                source: 0,
                number: 1,
                turn: 0,
            },
        ],
    )
    .unwrap();
    assert_eq!(page_contents(&output), ["", "2", "", "1"]);
    let document = Document::load_mem(&output).unwrap();
    let pages = document.get_pages();
    assert_eq!(media_box(&document, pages[&1]), [0.0, 0.0, 300.0, 500.0]);
    let rotate = |number| {
        document
            .get_dictionary(pages[&number])
            .and_then(|page| page.get(b"Rotate"))
            .and_then(Object::as_i64)
            .unwrap_or(0)
    };
    assert_eq!((rotate(1), rotate(3)), (90, 0));

    let bad = organize_pdf_items(
        &[&source],
        &[],
        &[OrganizeItem::Blank {
            width: 0.0,
            height: 500.0,
            turn: 0,
        }],
    );
    assert!(bad.is_err());
}

#[test]
fn fits_each_page_to_its_image() {
    let wide = jpeg_bytes(40, 20, 90);
    let tall = jpeg_bytes(20, 40, 90);
    let output = images_to_pdf_bytes(
        &[&wide, &tall],
        ImagePdfOptions {
            page_width: 0.0,
            page_height: 0.0,
            margin: 10.0,
            orientation: PageOrientation::Auto,
        },
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let pages = document.get_pages();
    assert_eq!(media_box(&document, pages[&1]), [0.0, 0.0, 50.0, 35.0]);
    assert_eq!(media_box(&document, pages[&2]), [0.0, 0.0, 35.0, 50.0]);
}

#[test]
fn forced_orientation_applies_to_every_image() {
    let wide = jpeg_bytes(40, 20, 90);
    let tall = jpeg_bytes(20, 40, 90);
    for (orientation, expected) in [
        (PageOrientation::Portrait, [595.0, 842.0]),
        (PageOrientation::Landscape, [842.0, 595.0]),
    ] {
        let output = images_to_pdf_bytes(
            &[&wide, &tall],
            ImagePdfOptions {
                orientation,
                ..image_pdf_options()
            },
        )
        .unwrap();
        let document = Document::load_mem(&output).unwrap();
        for page in document.get_pages().into_values() {
            assert_eq!(media_box(&document, page)[2..], expected);
        }
    }
}

fn protect(bytes: &[u8], user: &str, owner: &str, print: bool, copy: bool, edit: bool) -> Vec<u8> {
    protect_pdf_bytes(
        bytes,
        "",
        ProtectOptions {
            user_password: user,
            owner_password: owner,
            allow_printing: print,
            allow_copying: copy,
            allow_editing: edit,
        },
    )
    .unwrap()
}

fn encrypt_entry(bytes: &[u8], key: &[u8]) -> i64 {
    let document = Document::load_mem(bytes).unwrap();
    document
        .get_encrypted()
        .unwrap()
        .get(key)
        .and_then(Object::as_i64)
        .unwrap()
}

#[test]
fn protects_with_aes_256_and_opens_only_with_a_password() {
    let source = numbered_pdf(3);
    let locked = protect(&source, "open", "owner", true, true, true);

    assert_eq!(encrypt_entry(&locked, b"V"), 5);
    assert_eq!(encrypt_entry(&locked, b"R"), 6);
    // poppler misreads AES-256 files without these (see security.rs).
    assert_eq!(encrypt_entry(&locked, b"Length"), 256);
    assert_eq!(protection_of(&locked).unwrap(), Protection::Password);
    assert!(split_pdf_bytes(&locked, SplitMode::Every(10)).is_err());
    for password in ["open", "owner"] {
        let parts = split_pdf_bytes_with_password(&locked, password, SplitMode::Every(10)).unwrap();
        assert_eq!(page_contents(&parts[0]), ["1", "2", "3"]);
    }
}

#[test]
fn records_the_permissions_it_was_asked_for() {
    let locked = protect(&numbered_pdf(1), "open", "", false, true, false);
    let bits = encrypt_entry(&locked, b"P") as u32;
    let has = |flag: Permissions| bits & flag.bits() as u32 != 0;
    assert!(!has(Permissions::PRINTABLE));
    assert!(has(Permissions::COPYABLE));
    assert!(!has(Permissions::MODIFIABLE));
    assert!(has(Permissions::COPYABLE_FOR_ACCESSIBILITY));
}

#[test]
fn restrictions_alone_open_without_a_password() {
    let restricted = protect(&numbered_pdf(2), "", "", false, false, false);
    assert_eq!(protection_of(&restricted).unwrap(), Protection::Restricted);
    let parts = split_pdf_bytes(&restricted, SplitMode::Every(10)).unwrap();
    assert_eq!(page_contents(&parts[0]), ["1", "2"]);
}

#[test]
fn unlocking_round_trips_and_refuses_unprotected_files() {
    let source = numbered_pdf(3);
    assert_eq!(protection_of(&source).unwrap(), Protection::None);
    assert!(unlock_pdf_bytes(&source, "").is_err());

    let locked = protect(&source, "open", "", true, true, true);
    let unlocked = unlock_pdf_bytes(&locked, "open").unwrap();
    assert_eq!(protection_of(&unlocked).unwrap(), Protection::None);
    assert_eq!(page_contents(&unlocked), ["1", "2", "3"]);

    let restricted = protect(&source, "", "", false, true, true);
    let freed = unlock_pdf_bytes(&restricted, "").unwrap();
    assert_eq!(protection_of(&freed).unwrap(), Protection::None);
}

#[test]
fn reprotecting_changes_the_password() {
    let first = protect(&numbered_pdf(1), "old", "", true, true, true);
    let second = protect_pdf_bytes(
        &first,
        "old",
        ProtectOptions {
            user_password: "new",
            owner_password: "",
            allow_printing: true,
            allow_copying: true,
            allow_editing: true,
        },
    )
    .unwrap();
    assert!(split_pdf_bytes_with_password(&second, "old", SplitMode::Every(1)).is_err());
    let parts = split_pdf_bytes_with_password(&second, "new", SplitMode::Every(1)).unwrap();
    assert_eq!(page_contents(&parts[0]), ["1"]);
}

#[test]
fn protect_refuses_options_that_protect_nothing() {
    let source = numbered_pdf(1);
    let attempt = |user: &str, owner: &str, print: bool| {
        protect_pdf_bytes(
            &source,
            "",
            ProtectOptions {
                user_password: user,
                owner_password: owner,
                allow_printing: print,
                allow_copying: true,
                allow_editing: true,
            },
        )
    };
    assert!(attempt("", "", true).is_err());
    assert!(attempt("same", "same", false).is_err());
    assert!(attempt(&"x".repeat(128), "", true).is_err());
}

fn helvetica(size: f32) -> TextStyle {
    TextStyle {
        family: FontFamily::Helvetica,
        bold: false,
        size,
        color: [0.0, 0.0, 0.0],
    }
}

fn number_options(template: &str) -> PageNumberOptions<'_> {
    PageNumberOptions {
        template,
        first_number: 1,
        pages: &[],
        position: Position::Bottom,
        margin: 20.0,
        style: helvetica(10.0),
        opacity: 1.0,
    }
}

fn watermark_options(text: &str) -> WatermarkOptions<'_> {
    WatermarkOptions {
        content: WatermarkContent::Text {
            text,
            style: helvetica(20.0),
        },
        pages: &[],
        position: Position::Center,
        margin: 20.0,
        rotation: 0.0,
        opacity: 1.0,
        behind: false,
        tile: false,
    }
}

/// One page per entry, each with its content and any other page entries.
/// `tree` adds entries to the single page tree node.
fn pdf_with_pages(pages: &[(&str, Dictionary)], tree: Dictionary) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let kids = pages
        .iter()
        .map(|(content, entries)| {
            let content_id =
                document.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
            let mut page = entries.clone();
            page.set("Type", "Page");
            page.set("Parent", pages_id);
            page.set("Contents", content_id);
            Object::Reference(document.add_object(page))
        })
        .collect::<Vec<_>>();
    let mut node = tree;
    node.set("Type", "Pages");
    node.set("Count", kids.len() as i64);
    node.set("Kids", kids);
    document.objects.insert(pages_id, Object::Dictionary(node));
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn square_page(size: i64) -> Dictionary {
    dictionary! { "MediaBox" => vec![0.into(), 0.into(), size.into(), size.into()] }
}

/// The ids of the forms a page draws, in drawing order.
fn stamp_forms(document: &Document, page: lopdf::ObjectId) -> Vec<lopdf::ObjectId> {
    let content = Content::decode(&document.get_page_content(page)).unwrap();
    let xobjects = document
        .get_page_resources(page)
        .unwrap()
        .0
        .and_then(|resources| resources.get(b"XObject").ok())
        .and_then(|xobjects| match xobjects {
            Object::Reference(id) => document.get_dictionary(*id).ok(),
            xobjects => xobjects.as_dict().ok(),
        });
    content
        .operations
        .iter()
        .filter(|operation| operation.operator == "Do")
        .filter_map(|operation| {
            let name = operation.operands[0].as_name().ok()?;
            xobjects?.get(name).and_then(Object::as_reference).ok()
        })
        .collect()
}

/// Applies `first`, then `second`.
fn then(first: [f32; 6], second: [f32; 6]) -> [f32; 6] {
    let [a, b, c, d, e, f] = first;
    let [a2, b2, c2, d2, e2, f2] = second;
    [
        a * a2 + b * c2,
        a * b2 + b * d2,
        c * a2 + d * c2,
        c * b2 + d * d2,
        e * a2 + f * c2 + e2,
        e * b2 + f * d2 + f2,
    ]
}

/// Every string a page's stamps show, with where it starts in the page's own
/// coordinates.
fn stamped_text(document: &Document, page: lopdf::ObjectId) -> Vec<(String, f32, f32)> {
    let mut shown = Vec::new();
    for form in stamp_forms(document, page) {
        let stream = document
            .get_object(form)
            .and_then(Object::as_stream)
            .unwrap();
        let content = Content::decode(&stream.decompressed_content().unwrap()).unwrap();
        let mut transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut origin = (0.0, 0.0);
        for operation in content.operations {
            let numbers = || {
                operation
                    .operands
                    .iter()
                    .map(|operand| operand.as_float().unwrap())
                    .collect::<Vec<_>>()
            };
            match operation.operator.as_str() {
                "cm" => transform = then(numbers().try_into().unwrap(), transform),
                "Tm" => origin = (numbers()[4], numbers()[5]),
                "Tj" => {
                    let text = operation.operands[0]
                        .as_str()
                        .unwrap()
                        .iter()
                        .map(|&byte| char::from(byte))
                        .collect();
                    let [a, b, c, d, e, f] = transform;
                    shown.push((
                        text,
                        a * origin.0 + c * origin.1 + e,
                        b * origin.0 + d * origin.1 + f,
                    ));
                }
                _ => {}
            }
        }
    }
    shown
}

fn page_texts(bytes: &[u8]) -> Vec<Vec<String>> {
    let document = Document::load_mem(bytes).unwrap();
    document
        .get_pages()
        .into_values()
        .map(|page| {
            stamped_text(&document, page)
                .into_iter()
                .map(|(text, _, _)| text)
                .collect()
        })
        .collect()
}

fn first_stamp_origin(bytes: &[u8]) -> (f32, f32) {
    let document = Document::load_mem(bytes).unwrap();
    let page = document.get_pages()[&1];
    let (_, x, y) = stamped_text(&document, page).remove(0);
    (x, y)
}

fn assert_near((x, y): (f32, f32), (expected_x, expected_y): (f32, f32)) {
    assert!(
        (x - expected_x).abs() < 0.01 && (y - expected_y).abs() < 0.01,
        "({x}, {y}) is not ({expected_x}, {expected_y})"
    );
}

#[test]
fn numbers_every_page_centred_at_the_bottom() {
    let output =
        add_page_numbers_bytes(&numbered_pdf(3), "", number_options("Page {n} of {total}"))
            .unwrap();
    assert_eq!(
        page_texts(&output),
        [["Page 1 of 3"], ["Page 2 of 3"], ["Page 3 of 3"]]
    );
    // "Page 1 of 3" is 511.5 thousandths of an em wide in Helvetica; the
    // baseline sits on the margin.
    assert_near(first_stamp_origin(&output), (200.0 - 51.15 / 2.0, 20.0));
    for (content, original) in page_contents(&output).iter().zip(["1", "2", "3"]) {
        assert!(content.contains(original), "{content}");
    }
}

#[test]
fn numbers_chosen_pages_counting_the_ones_skipped() {
    let output = add_page_numbers_bytes(
        &numbered_pdf(4),
        "",
        PageNumberOptions {
            pages: &[4, 2],
            first_number: 1,
            ..number_options("{n}/{total}")
        },
    )
    .unwrap();
    let texts = page_texts(&output);
    assert_eq!(texts[0], Vec::<String>::new());
    assert_eq!(texts[1], ["1/3"]);
    assert_eq!(texts[2], Vec::<String>::new());
    assert_eq!(texts[3], ["3/3"]);
    assert_eq!(page_contents(&output)[0], "1");
}

#[test]
fn places_numbers_at_the_bottom_the_reader_sees_on_a_turned_page() {
    // "7" is 5.56 points wide at 10 points; the page is 200 by 400.
    for (rotation, expected) in [
        (0, (100.0 - 2.78, 20.0)),
        (90, (180.0, 200.0 - 2.78)),
        (180, (100.0 + 2.78, 380.0)),
        (270, (20.0, 200.0 + 2.78)),
    ] {
        let input = pdf_with_pages(
            &[(
                "",
                dictionary! {
                    "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()],
                    "Rotate" => rotation,
                },
            )],
            dictionary! {},
        );
        let output = add_page_numbers_bytes(
            &input,
            "",
            PageNumberOptions {
                first_number: 7,
                ..number_options("{n}")
            },
        )
        .unwrap();
        assert_near(first_stamp_origin(&output), expected);
    }
}

#[test]
fn keeps_numbers_inside_the_crop_box_and_honours_user_units() {
    let cropped = pdf_with_pages(
        &[(
            "",
            dictionary! {
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "CropBox" => vec![50.into(), 60.into(), 150.into(), 160.into()],
            },
        )],
        dictionary! {},
    );
    let options = PageNumberOptions {
        position: Position::BottomLeft,
        margin: 10.0,
        ..number_options("{n}")
    };
    let output = add_page_numbers_bytes(&cropped, "", options).unwrap();
    assert_near(first_stamp_origin(&output), (60.0, 70.0));

    // Each unit is two points, so a 10 point margin is 5 units.
    let mut large = square_page(200);
    large.set("UserUnit", 2);
    let options = PageNumberOptions {
        position: Position::BottomLeft,
        margin: 10.0,
        ..number_options("{n}")
    };
    let output =
        add_page_numbers_bytes(&pdf_with_pages(&[("", large)], dictionary! {}), "", options)
            .unwrap();
    assert_near(first_stamp_origin(&output), (5.0, 5.0));
}

#[test]
fn measures_text_with_the_font_it_draws_with() {
    // "Hello" is 2278 thousandths wide in Helvetica, 2222 in Times, and 3000
    // in Courier; cap heights are 718, 662 and 562.
    for (family, width, cap) in [
        (FontFamily::Helvetica, 22.78, 7.18),
        (FontFamily::Times, 22.22, 6.62),
        (FontFamily::Courier, 30.0, 5.62),
    ] {
        let input = pdf_with_pages(&[("", square_page(200))], dictionary! {});
        let output = add_page_numbers_bytes(
            &input,
            "",
            PageNumberOptions {
                position: Position::TopRight,
                margin: 10.0,
                style: TextStyle {
                    family,
                    ..helvetica(10.0)
                },
                ..number_options("Hello")
            },
        )
        .unwrap();
        assert_near(first_stamp_origin(&output), (190.0 - width, 190.0 - cap));
    }
}

#[test]
fn closes_states_the_page_left_open_before_drawing() {
    let input = pdf_with_pages(
        &[("q 2 0 0 2 0 0 cm q 1 0 0 1 5 5 cm 0 0 m", square_page(200))],
        dictionary! {},
    );
    let output = add_page_numbers_bytes(&input, "", number_options("{n}")).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let content = Content::decode(&document.get_page_content(document.get_pages()[&1])).unwrap();
    let mut depth = 0usize;
    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => depth += 1,
            "Q" => depth = depth.saturating_sub(1),
            "Do" => {
                assert_eq!(depth, 0, "the stamp inherits the page's transform");
                return;
            }
            _ => {}
        }
    }
    panic!("the stamp is never drawn");
}

#[test]
fn keeps_resources_the_page_inherited() {
    let output = add_page_numbers_bytes(
        &inheriting_pdf("BT /F1 12 Tf ET", 300),
        "",
        number_options("{n}"),
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let page = document.get_pages()[&1];
    let resources = document.get_page_resources(page).unwrap().0.unwrap();
    let fonts = match resources.get(b"Font").unwrap() {
        Object::Reference(id) => document.get_dictionary(*id).unwrap(),
        fonts => fonts.as_dict().unwrap(),
    };
    let font = fonts.get(b"F1").and_then(Object::as_reference).unwrap();
    assert_eq!(
        document
            .get_dictionary(font)
            .and_then(|font| font.get(b"BaseFont"))
            .and_then(Object::as_name)
            .unwrap(),
        b"Helvetica"
    );
    assert_eq!(stamp_forms(&document, page).len(), 1);
    // Centred on the 300 point width the page inherits.
    assert_near(first_stamp_origin(&output), (150.0 - 2.78, 20.0));
}

#[test]
fn leaves_resources_shared_with_unstamped_pages_alone() {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let shared = document.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font },
    });
    let kids = ["first", "second"]
        .into_iter()
        .map(|label| {
            let content = document.add_object(Stream::new(dictionary! {}, label.into()));
            Object::Reference(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content,
                "Resources" => shared,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            }))
        })
        .collect::<Vec<_>>();
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 2 }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    let output = add_page_numbers_bytes(
        &input,
        "",
        PageNumberOptions {
            pages: &[1],
            ..number_options("{n}")
        },
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let pages = document.get_pages();
    let resources_of = |page| {
        document
            .get_dictionary(pages[&page])
            .unwrap()
            .get(b"Resources")
    };

    let second = resources_of(2).and_then(Object::as_reference).unwrap();
    let second = document.get_dictionary(second).unwrap();
    assert!(!second.has(b"XObject"));
    // The first page's copy refers to the same font dictionary rather than
    // carrying its own.
    let first = resources_of(1).and_then(Object::as_dict).unwrap();
    assert_eq!(
        first.get(b"Font").and_then(Object::as_reference).unwrap(),
        second.get(b"Font").and_then(Object::as_reference).unwrap()
    );
    assert_eq!(page_texts(&output), [vec!["1"], vec![]]);
}

#[test]
fn draws_a_watermark_behind_the_page_when_asked() {
    let output = add_watermark_bytes(
        &numbered_pdf(1),
        "",
        WatermarkOptions {
            behind: true,
            ..watermark_options("DRAFT")
        },
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let content = Content::decode(&document.get_page_content(document.get_pages()[&1])).unwrap();
    assert_eq!(content.operations[0].operator, "Do");
}

#[test]
fn watermark_shares_one_form_between_pages_of_one_size() {
    let output = add_watermark_bytes(
        &numbered_pdf(3),
        "",
        WatermarkOptions {
            opacity: 0.25,
            rotation: 45.0,
            ..watermark_options("DRAFT")
        },
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let forms = document
        .get_pages()
        .into_values()
        .flat_map(|page| stamp_forms(&document, page))
        .collect::<BTreeSet<_>>();
    assert_eq!(forms.len(), 1);
    let form = document
        .get_object(*forms.first().unwrap())
        .and_then(Object::as_stream)
        .unwrap();
    let state = form
        .dict
        .get(b"Resources")
        .and_then(Object::as_dict)
        .and_then(|resources| resources.get(b"ExtGState"))
        .and_then(Object::as_dict)
        .and_then(|states| states.get(b"G0"))
        .and_then(Object::as_reference)
        .unwrap();
    let opacity = document
        .get_dictionary(state)
        .and_then(|state| state.get(b"ca"))
        .and_then(Object::as_float)
        .unwrap();
    assert!((opacity - 0.25).abs() < 1e-6);
}

#[test]
fn transparency_raises_an_old_version_to_1_4() {
    let mut document = Document::load_mem(&one_page_pdf("old")).unwrap();
    document.version = "1.3".into();
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();
    let output = add_watermark_bytes(
        &input,
        "",
        WatermarkOptions {
            opacity: 0.5,
            ..watermark_options("DRAFT")
        },
    )
    .unwrap();
    let version = Document::load_mem(&output).unwrap().version;
    assert!(parse_version(&version) >= (1, 4), "{version}");
}

#[test]
fn turns_a_watermark_about_its_centre() {
    // "X" is 13.34 points wide at 20 points and its capitals 14.36 tall, so
    // the unturned baseline starts 6.67 left of and 7.18 below the centre.
    let input = pdf_with_pages(&[("", square_page(200))], dictionary! {});
    let output = add_watermark_bytes(
        &input,
        "",
        WatermarkOptions {
            rotation: 90.0,
            ..watermark_options("X")
        },
    )
    .unwrap();
    assert_near(first_stamp_origin(&output), (107.18, 93.33));
}

#[test]
fn tiles_a_watermark_but_not_into_dust() {
    let input = pdf_with_pages(&[("", square_page(200))], dictionary! {});
    let output = add_watermark_bytes(
        &input,
        "",
        WatermarkOptions {
            tile: true,
            rotation: 30.0,
            ..watermark_options("X")
        },
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let shown = stamped_text(&document, document.get_pages()[&1]);
    assert!(shown.len() > 4, "{} tiles", shown.len());

    let dust = add_watermark_bytes(
        &input,
        "",
        WatermarkOptions {
            tile: true,
            margin: 0.0,
            content: WatermarkContent::Text {
                text: ".",
                style: helvetica(1.0),
            },
            ..watermark_options("")
        },
    )
    .unwrap_err();
    assert!(dust.contains("too small"), "{dust}");
}

#[test]
fn image_watermark_keeps_png_transparency() {
    let png = rgba_png();
    let input = pdf_with_pages(&[("", square_page(200))], dictionary! {});
    let output = add_watermark_bytes(
        &input,
        "",
        WatermarkOptions {
            content: WatermarkContent::Image {
                bytes: &png,
                width: 0.5,
            },
            ..watermark_options("")
        },
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let form = stamp_forms(&document, document.get_pages()[&1])[0];
    let form = document
        .get_object(form)
        .and_then(Object::as_stream)
        .unwrap();
    let image = form
        .dict
        .get(b"Resources")
        .and_then(Object::as_dict)
        .and_then(|resources| resources.get(b"XObject"))
        .and_then(Object::as_dict)
        .and_then(|images| images.get(b"I0"))
        .and_then(Object::as_reference)
        .unwrap();
    assert!(
        document
            .get_object(image)
            .and_then(Object::as_stream)
            .unwrap()
            .dict
            .has(b"SMask")
    );
    // A 2 by 1 image at half the page width, centred.
    let content = String::from_utf8(form.decompressed_content().unwrap()).unwrap();
    assert!(
        content.contains("1 0 0 1 50 75 cm\n100 0 0 50 0 0 cm\n/I0 Do"),
        "{content}"
    );
}

#[test]
fn refuses_what_it_cannot_draw() {
    let input = numbered_pdf(4);
    let error = add_watermark_bytes(&input, "", watermark_options("日本")).unwrap_err();
    assert!(error.contains("cannot be drawn"), "{error}");
    let error = add_page_numbers_bytes(
        &input,
        "",
        PageNumberOptions {
            pages: &[5],
            ..number_options("{n}")
        },
    )
    .unwrap_err();
    assert!(error.contains("between 1 and 4"), "{error}");
    assert!(add_page_numbers_bytes(&input, "", number_options(" ")).is_err());
    assert!(add_watermark_bytes(&input, "", watermark_options(" \n ")).is_err());
    for options in [
        WatermarkOptions {
            opacity: 1.5,
            ..watermark_options("A")
        },
        WatermarkOptions {
            margin: f32::NAN,
            ..watermark_options("A")
        },
        WatermarkOptions {
            rotation: f32::INFINITY,
            ..watermark_options("A")
        },
        WatermarkOptions {
            content: WatermarkContent::Text {
                text: "A",
                style: helvetica(0.0),
            },
            ..watermark_options("A")
        },
        WatermarkOptions {
            content: WatermarkContent::Image {
                bytes: b"not an image",
                width: 0.5,
            },
            ..watermark_options("")
        },
        WatermarkOptions {
            content: WatermarkContent::Image {
                bytes: &rgba_png(),
                width: 0.0,
            },
            ..watermark_options("")
        },
    ] {
        assert!(add_watermark_bytes(&input, "", options).is_err());
    }
}

#[test]
fn stamps_a_locked_pdf_with_its_password_and_writes_it_unlocked() {
    let locked = locked_pdf(&numbered_pdf(2), "secret");
    let missing = add_page_numbers_bytes(&locked, "", number_options("{n}")).unwrap_err();
    assert!(missing.contains("password protected"), "{missing}");

    let output = add_page_numbers_bytes(&locked, "secret", number_options("{n}")).unwrap();
    let document = Document::load_mem(&output).unwrap();
    assert!(document.trailer.get(b"Encrypt").is_err());
    assert_eq!(page_texts(&output), [["1"], ["2"]]);
}

#[test]
fn refuses_to_stamp_a_page_tree_listing_a_page_it_cannot_read() {
    let mut document = Document::with_version("1.4");
    let pages_id = document.new_object_id();
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            // (9, 0) is never written.
            "Kids" => vec![Object::Reference((9, 0)), page_id.into()],
            "Count" => 2,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    let mut broken = Vec::new();
    document.save_to(&mut broken).unwrap();

    let error = add_page_numbers_bytes(&broken, "", number_options("{n}")).unwrap_err();
    assert!(error.contains("damaged"), "{error}");
    assert!(add_watermark_bytes(&broken, "", watermark_options("DRAFT")).is_err());
}

/// A page whose /Annots names object 50, which is never written.
fn dangling_annotation_pdf() -> Vec<u8> {
    let mut page = square_page(200);
    page.set("Annots", vec![Object::Reference((50, 0))]);
    pdf_with_pages(&[("first", page)], dictionary! {})
}

#[test]
fn a_reference_to_a_missing_object_stays_dangling_when_written() {
    let broken = dangling_annotation_pdf();
    let other = numbered_pdf(3);
    let outputs = [
        merge_pdf_bytes(&[&broken, &other]).unwrap(),
        split_pdf_bytes(&broken, SplitMode::Every(1))
            .unwrap()
            .remove(0),
        organize_pdfs_bytes(&[&broken, &other], &[(1, 1, 0), (0, 1, 0)]).unwrap(),
    ];
    for output in outputs {
        let document = Document::load_mem(&output).unwrap();
        let annotated = document
            .get_pages()
            .into_values()
            .find(|page| document.get_dictionary(*page).unwrap().has(b"Annots"))
            .unwrap();
        let annotation = document
            .get_dictionary(annotated)
            .and_then(|page| page.get(b"Annots"))
            .and_then(Object::as_array)
            .unwrap()[0]
            .as_reference()
            .unwrap();
        assert!(
            document.get_object(annotation).is_err(),
            "the missing annotation now resolves to {:?}",
            document.get_object(annotation)
        );
        let producer = document
            .trailer
            .get(b"Info")
            .and_then(Object::as_reference)
            .and_then(|info| document.get_dictionary(info))
            .and_then(|info| info.get(b"Producer"))
            .and_then(Object::as_str)
            .unwrap();
        assert_eq!(producer, b"Plico");
    }
}

/// A one-page PDF whose page draws `content`; `build` adds whatever else the
/// test needs, given the page and catalog ids.
fn pdfa_fixture(content: &str, build: impl FnOnce(&mut Document, ObjectId, ObjectId)) -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        "Resources" => dictionary! {},
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    build(&mut document, page_id, catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

fn to_pdfa(bytes: &[u8], level: PdfALevel) -> Document {
    Document::load_mem(&convert_to_pdfa_bytes(bytes, "", level, &[]).unwrap()).unwrap()
}

fn catalog_of(document: &Document) -> &Dictionary {
    document.catalog().unwrap()
}

fn resolved<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap(),
        object => object,
    }
}

fn first_page(document: &Document) -> &Dictionary {
    let id = *document.get_pages().values().next().unwrap();
    document.get_dictionary(id).unwrap()
}

#[test]
fn pdfa_declares_its_conformance_and_keeps_the_page() {
    let input = pdfa_fixture("0 0 1 rg 0 0 10 10 re f", |_, _, _| {});
    let output = convert_to_pdfa_bytes(&input, "", PdfALevel::A2b, &[]).unwrap();
    // 6.1.2: a comment of at least four bytes above 127 follows the header.
    assert!(output.starts_with(b"%PDF-1."));
    let mark = output.split(|&byte| byte == b'\n').nth(1).unwrap();
    assert!(mark.len() >= 5 && mark[0] == b'%' && mark[1..5].iter().all(|&byte| byte > 127));

    let document = Document::load_mem(&output).unwrap();
    assert_eq!(page_contents(&output), vec!["0 0 1 rg 0 0 10 10 re f"]);
    let id = document
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .unwrap();
    assert_eq!(id.len(), 2);

    let catalog = catalog_of(&document);
    let metadata = resolved(&document, catalog.get(b"Metadata").unwrap())
        .as_stream()
        .unwrap();
    assert!(!metadata.dict.has(b"Filter"), "XMP is written uncompressed");
    let xmp = String::from_utf8(metadata.content.clone()).unwrap();
    assert!(xmp.contains("<pdfaid:part>2</pdfaid:part>"));
    assert!(xmp.contains("<pdfaid:conformance>B</pdfaid:conformance>"));

    let intents = resolved(&document, catalog.get(b"OutputIntents").unwrap())
        .as_array()
        .unwrap();
    assert_eq!(intents.len(), 1);
    let intent = resolved(&document, &intents[0]).as_dict().unwrap();
    assert_eq!(
        intent.get(b"S").and_then(Object::as_name).unwrap(),
        b"GTS_PDFA1"
    );
    let profile = resolved(&document, intent.get(b"DestOutputProfile").unwrap())
        .as_stream()
        .unwrap();
    assert_eq!(profile.dict.get(b"N").and_then(Object::as_i64).unwrap(), 3);
}

#[test]
fn pdfa_3_declares_part_3() {
    let input = pdfa_fixture("", |_, _, _| {});
    let document = to_pdfa(&input, PdfALevel::A3b);
    let metadata = resolved(&document, catalog_of(&document).get(b"Metadata").unwrap())
        .as_stream()
        .unwrap();
    assert!(String::from_utf8_lossy(&metadata.content).contains("<pdfaid:part>3</pdfaid:part>"));
}

#[test]
fn pdfa_removes_scripts_and_actions_that_change_the_document() {
    let input = pdfa_fixture("", |document, page_id, catalog_id| {
        let script = document.add_object(dictionary! {
            "S" => "JavaScript",
            "JS" => Object::string_literal("app.alert(1)"),
        });
        let launch = dictionary! { "S" => "Launch", "F" => Object::string_literal("calc.exe") };
        let link = |action: Dictionary| {
            dictionary! {
                "Type" => "Annot",
                "Subtype" => "Link",
                "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
                "A" => action,
            }
        };
        let web = link(dictionary! {
            "S" => "URI",
            "URI" => Object::string_literal("https://example.com"),
            "Next" => script,
        });
        let bad = document.add_object(link(launch));
        let good = document.add_object(web);
        let page = document.get_dictionary_mut(page_id).unwrap();
        page.set("Annots", vec![bad.into(), good.into()]);
        page.set("AA", dictionary! { "O" => script });
        let catalog = document.get_dictionary_mut(catalog_id).unwrap();
        catalog.set("OpenAction", script);
        catalog.set(
            "Names",
            dictionary! { "JavaScript" => dictionary! { "Names" => vec![Object::string_literal("x"), script.into()] } },
        );
    });
    let document = to_pdfa(&input, PdfALevel::A2b);
    let catalog = catalog_of(&document);
    assert!(!catalog.has(b"OpenAction"));
    let names = resolved(&document, catalog.get(b"Names").unwrap())
        .as_dict()
        .unwrap();
    assert!(!names.has(b"JavaScript"));
    let page = first_page(&document);
    assert!(!page.has(b"AA"));

    let annotations = resolved(&document, page.get(b"Annots").unwrap())
        .as_array()
        .unwrap();
    assert_eq!(annotations.len(), 2, "links stay, only their actions go");
    let action = |index: usize| {
        resolved(&document, &annotations[index])
            .as_dict()
            .unwrap()
            .get(b"A")
            .ok()
            .map(|action| resolved(&document, action).as_dict().unwrap().clone())
    };
    assert!(action(0).is_none());
    let web = action(1).unwrap();
    assert_eq!(web.get(b"S").and_then(Object::as_name).unwrap(), b"URI");
    assert!(!web.has(b"Next"));
}

#[test]
fn pdfa_refuses_fonts_without_a_program_but_not_form_defaults() {
    let drawn = pdfa_fixture("BT /F1 12 Tf (Hi) Tj ET", |document, page_id, _| {
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        document.get_dictionary_mut(page_id).unwrap().set(
            "Resources",
            dictionary! { "Font" => dictionary! { "F1" => font } },
        );
    });
    let error = convert_to_pdfa_bytes(&drawn, "", PdfALevel::A2b, &[]).unwrap_err();
    assert!(error.contains("“Helvetica”"), "{error}");

    let form_only = pdfa_fixture("", |document, _, catalog_id| {
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        document.get_dictionary_mut(catalog_id).unwrap().set(
            "AcroForm",
            dictionary! {
                "Fields" => vec![],
                "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
                "NeedAppearances" => true,
            },
        );
    });
    let document = to_pdfa(&form_only, PdfALevel::A2b);
    let form = resolved(&document, catalog_of(&document).get(b"AcroForm").unwrap())
        .as_dict()
        .unwrap();
    assert!(!form.has(b"NeedAppearances"));
}

fn attachment_fixture() -> Vec<u8> {
    pdfa_fixture("", |document, _, catalog_id| {
        let file = document.add_object(Stream::new(
            dictionary! { "Type" => "EmbeddedFile" },
            b"a,b".to_vec(),
        ));
        let specification = document.add_object(dictionary! {
            "Type" => "Filespec",
            "F" => Object::string_literal("data.csv"),
            "EF" => dictionary! { "F" => file },
        });
        document.get_dictionary_mut(catalog_id).unwrap().set(
            "Names",
            dictionary! {
                "EmbeddedFiles" => dictionary! {
                    "Names" => vec![Object::string_literal("data.csv"), specification.into()],
                },
            },
        );
    })
}

#[test]
fn pdfa_2_refuses_attachments_and_pdfa_3_associates_them() {
    let input = attachment_fixture();
    let error = convert_to_pdfa_bytes(&input, "", PdfALevel::A2b, &[]).unwrap_err();
    assert!(error.contains("PDF/A-3"), "{error}");

    let document = to_pdfa(&input, PdfALevel::A3b);
    let associated = resolved(&document, catalog_of(&document).get(b"AF").unwrap())
        .as_array()
        .unwrap();
    assert_eq!(associated.len(), 1);
    let specification = resolved(&document, &associated[0]).as_dict().unwrap();
    assert!(specification.has(b"UF"));
    assert_eq!(
        specification
            .get(b"AFRelationship")
            .and_then(Object::as_name)
            .unwrap(),
        b"Unspecified"
    );
    let files = specification.get(b"EF").and_then(Object::as_dict).unwrap();
    let file = resolved(&document, files.get(b"F").unwrap())
        .as_stream()
        .unwrap();
    assert_eq!(
        file.dict.get(b"Subtype").and_then(Object::as_name).unwrap(),
        b"application/octet-stream"
    );
}

fn default_cmyk(document: &Document) -> Option<i64> {
    let resources = resolved(document, first_page(document).get(b"Resources").ok()?)
        .as_dict()
        .ok()?;
    let spaces = resolved(document, resources.get(b"ColorSpace").ok()?)
        .as_dict()
        .ok()?;
    let space = resolved(document, spaces.get(b"DefaultCMYK").ok()?)
        .as_array()
        .ok()?;
    assert_eq!(space[0].as_name().unwrap(), b"ICCBased");
    resolved(document, &space[1])
        .as_stream()
        .ok()?
        .dict
        .get(b"N")
        .and_then(Object::as_i64)
        .ok()
}

#[test]
fn pdfa_gives_device_cmyk_a_profile_only_when_it_is_used() {
    let cmyk = pdfa_fixture("0 0 0 1 k 0 0 10 10 re f", |_, _, _| {});
    assert_eq!(default_cmyk(&to_pdfa(&cmyk, PdfALevel::A2b)), Some(4));
    let rgb = pdfa_fixture("1 0 0 rg 0 0 10 10 re f", |_, _, _| {});
    assert_eq!(default_cmyk(&to_pdfa(&rgb, PdfALevel::A2b)), None);
}

#[test]
fn pdfa_names_the_profile_on_cmyk_images_and_shadings() {
    let input = pdfa_fixture("/Im0 Do", |document, page_id, _| {
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 1,
                "Height" => 1,
                "BitsPerComponent" => 8,
                "ColorSpace" => "DeviceCMYK",
                "Interpolate" => true,
            },
            vec![0, 0, 0, 255],
        ));
        document.get_dictionary_mut(page_id).unwrap().set(
            "Resources",
            dictionary! { "XObject" => dictionary! { "Im0" => image } },
        );
    });
    let document = to_pdfa(&input, PdfALevel::A2b);
    let image = document
        .objects
        .values()
        .find_map(|object| {
            let stream = object.as_stream().ok()?;
            (stream.dict.get(b"Subtype").and_then(Object::as_name).ok()? == b"Image")
                .then_some(stream)
        })
        .unwrap();
    let space = image
        .dict
        .get(b"ColorSpace")
        .and_then(Object::as_array)
        .unwrap();
    assert_eq!(space[0].as_name().unwrap(), b"ICCBased");
    assert!(
        !image
            .dict
            .get(b"Interpolate")
            .and_then(Object::as_bool)
            .unwrap_or(false)
    );
}

#[test]
fn pdfa_keeps_hidden_annotations_invisible_and_refuses_ones_without_appearance() {
    let annotated = |flags: i64, appearance: bool| {
        pdfa_fixture("", move |document, page_id, _| {
            let mut annotation = dictionary! {
                "Type" => "Annot",
                "Subtype" => "Square",
                "Rect" => vec![10.into(), 10.into(), 50.into(), 30.into()],
                "F" => flags,
            };
            if appearance {
                let normal = document.add_object(Stream::new(
                    dictionary! { "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 20.into()] },
                    b"0 0 40 20 re S".to_vec(),
                ));
                annotation.set("AP", dictionary! { "N" => normal, "D" => normal });
            }
            let id = document.add_object(annotation);
            document
                .get_dictionary_mut(page_id)
                .unwrap()
                .set("Annots", vec![id.into()]);
        })
    };

    let document = to_pdfa(&annotated(2, true), PdfALevel::A2b);
    let page = first_page(&document);
    let annotations = resolved(&document, page.get(b"Annots").unwrap())
        .as_array()
        .unwrap();
    let annotation = resolved(&document, &annotations[0]).as_dict().unwrap();
    assert_eq!(annotation.get(b"F").and_then(Object::as_i64).unwrap(), 4);
    let appearance = annotation.get(b"AP").and_then(Object::as_dict).unwrap();
    assert!(!appearance.has(b"D"), "only the normal appearance may stay");
    let normal = resolved(&document, appearance.get(b"N").unwrap())
        .as_stream()
        .unwrap();
    assert!(
        normal.content.is_empty(),
        "a hidden annotation still draws nothing"
    );

    let visible = to_pdfa(&annotated(0, true), PdfALevel::A2b);
    let annotations = resolved(&visible, first_page(&visible).get(b"Annots").unwrap())
        .as_array()
        .unwrap();
    let annotation = resolved(&visible, &annotations[0]).as_dict().unwrap();
    assert_eq!(
        annotation.get(b"F").and_then(Object::as_i64).unwrap(),
        4,
        "printable"
    );

    let error = convert_to_pdfa_bytes(&annotated(0, false), "", PdfALevel::A2b, &[]).unwrap_err();
    assert!(error.contains("page 1"), "{error}");
}

#[test]
fn pdfa_lets_a_form_without_resources_borrow_its_page() {
    let input = pdfa_fixture("/Fm0 Do", |document, page_id, _| {
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            },
            b"/Fm0 Do".to_vec(),
        ));
        document.get_dictionary_mut(page_id).unwrap().set(
            "Resources",
            dictionary! { "XObject" => dictionary! { "Fm0" => form } },
        );
    });
    let document = to_pdfa(&input, PdfALevel::A2b);
    let form = document
        .objects
        .values()
        .find_map(|object| {
            let stream = object.as_stream().ok()?;
            (stream.dict.get(b"Subtype").and_then(Object::as_name).ok()? == b"Form")
                .then_some(stream)
        })
        .unwrap();
    assert!(form.dict.has(b"Resources"));
}

#[test]
fn pdfa_metadata_carries_the_document_information() {
    let input = pdfa_fixture("", |document, _, _| {
        let info = document.add_object(dictionary! {
            "Title" => Object::string_literal("Q&A <draft>"),
            "Author" => Object::string_literal("Plico"),
            "CreationDate" => Object::string_literal("D:20260101120000+02'00'"),
            "ModDate" => Object::string_literal("yesterday"),
        });
        document.trailer.set("Info", info);
        // An empty /ID counts as none.
        document.trailer.set("ID", vec![]);
    });
    let document = to_pdfa(&input, PdfALevel::A2b);
    let metadata = resolved(&document, catalog_of(&document).get(b"Metadata").unwrap())
        .as_stream()
        .unwrap();
    let xmp = String::from_utf8(metadata.content.clone()).unwrap();
    assert!(xmp.contains("Q&amp;A &lt;draft&gt;"));
    assert!(xmp.contains("<rdf:li>Plico</rdf:li>"));
    assert!(xmp.contains("<xmp:CreateDate>2026-01-01T12:00:00+02:00</xmp:CreateDate>"));
    assert!(!xmp.contains("ModifyDate"));
    let info = resolved(&document, document.trailer.get(b"Info").unwrap())
        .as_dict()
        .unwrap();
    assert!(
        !info.has(b"ModDate"),
        "a date XMP cannot hold is dropped from both"
    );
    assert_eq!(
        document
            .trailer
            .get(b"ID")
            .and_then(Object::as_array)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn reads_pdf_dates_with_any_precision() {
    assert_eq!(
        xmp_date("D:20260101120000+02'00'").as_deref(),
        Some("2026-01-01T12:00:00+02:00")
    );
    assert_eq!(
        xmp_date("D:20260101120000Z").as_deref(),
        Some("2026-01-01T12:00:00Z")
    );
    assert_eq!(xmp_date("D:2026").as_deref(), Some("2026-01-01T00:00:00"));
    assert_eq!(xmp_date("D:202613").as_deref(), None);
    assert_eq!(xmp_date("garbage"), None);
}

fn substitute(name: &str) -> (Vec<u8>, String) {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../static/pdfa-fonts");
    (
        std::fs::read(format!("{directory}/{name}.cff")).unwrap(),
        std::fs::read_to_string(format!("{directory}/{name}.txt")).unwrap(),
    )
}

fn font_fixture(font: Dictionary) -> Vec<u8> {
    pdfa_fixture("BT /F1 12 Tf (Hi) Tj ET", |document, page_id, _| {
        let font = document.add_object(font);
        document.get_dictionary_mut(page_id).unwrap().set(
            "Resources",
            dictionary! { "Font" => dictionary! { "F1" => font } },
        );
    })
}

fn embedded_font(document: &Document) -> &Dictionary {
    document
        .objects
        .values()
        .find_map(|object| {
            let dictionary = object.as_dict().ok()?;
            (dictionary.get(b"Type").and_then(Object::as_name).ok()? == b"Font")
                .then_some(dictionary)
        })
        .unwrap()
}

#[test]
fn pdfa_embeds_a_substitute_for_a_standard_font() {
    let input = font_fixture(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica-Bold",
        "Encoding" => "WinAnsiEncoding",
    });
    assert_eq!(
        standard_fonts_for_pdfa(&input, "").unwrap(),
        vec!["Helvetica-Bold"]
    );
    let (program, metrics) = substitute("Helvetica-Bold");
    let fonts = [StandardFont {
        name: "Helvetica-Bold",
        program: &program,
        metrics: &metrics,
    }];
    let output = convert_to_pdfa_bytes(&input, "", PdfALevel::A2b, &fonts).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let font = embedded_font(&document);
    assert_eq!(
        font.get(b"Subtype").and_then(Object::as_name).unwrap(),
        b"Type1"
    );
    // Adobe's Helvetica-Bold widths, which the substitute matches: space and A.
    let first = font.get(b"FirstChar").and_then(Object::as_i64).unwrap();
    let widths = resolved(&document, font.get(b"Widths").unwrap())
        .as_array()
        .unwrap();
    let width = |code: i64| widths[(code - first) as usize].as_i64().unwrap();
    assert_eq!((width(32), width(65)), (278, 722));
    let descriptor = resolved(&document, font.get(b"FontDescriptor").unwrap())
        .as_dict()
        .unwrap();
    let file = resolved(&document, descriptor.get(b"FontFile3").unwrap())
        .as_stream()
        .unwrap();
    assert_eq!(
        file.dict.get(b"Subtype").and_then(Object::as_name).unwrap(),
        b"Type1C"
    );
}

#[test]
fn pdfa_spells_out_the_symbol_encoding() {
    let input = font_fixture(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Symbol" },
    );
    let (program, metrics) = substitute("Symbol");
    let fonts = [StandardFont {
        name: "Symbol",
        program: &program,
        metrics: &metrics,
    }];
    let document =
        Document::load_mem(&convert_to_pdfa_bytes(&input, "", PdfALevel::A2b, &fonts).unwrap())
            .unwrap();
    let encoding = embedded_font(&document)
        .get(b"Encoding")
        .and_then(Object::as_dict)
        .unwrap();
    let differences = encoding
        .get(b"Differences")
        .and_then(Object::as_array)
        .unwrap();
    // Code 97 is alpha in the Symbol encoding.
    let mut code = 0;
    let mut alpha = None;
    for item in differences {
        match item {
            Object::Integer(start) => code = *start,
            Object::Name(name) => {
                if name == b"alpha" {
                    alpha = Some(code);
                }
                code += 1;
            }
            _ => {}
        }
    }
    assert_eq!(alpha, Some(97));
}

#[test]
fn pdfa_refuses_a_substitute_that_would_move_text() {
    // Widths a quarter wider than Helvetica's are some other font.
    let input = font_fixture(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "Arial,Bold",
        "FirstChar" => 65,
        "LastChar" => 65,
        "Widths" => vec![900.into()],
        "Encoding" => "WinAnsiEncoding",
    });
    let (program, metrics) = substitute("Helvetica-Bold");
    let fonts = [StandardFont {
        name: "Helvetica-Bold",
        program: &program,
        metrics: &metrics,
    }];
    let error = convert_to_pdfa_bytes(&input, "", PdfALevel::A2b, &fonts).unwrap_err();
    assert!(error.contains("would move"), "{error}");
}

#[test]
fn pdfa_recognises_standard_font_aliases_only() {
    let needed = |name: &str| {
        let input = font_fixture(
            dictionary! { "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => name },
        );
        standard_fonts_for_pdfa(&input, "").unwrap()
    };
    assert_eq!(needed("ArialMT"), vec!["Helvetica"]);
    assert_eq!(needed("Arial,BoldItalic"), vec!["Helvetica-BoldOblique"]);
    assert_eq!(needed("TimesNewRomanPS-BoldMT"), vec!["Times-Bold"]);
    assert_eq!(needed("ABCDEF+CourierNew,Italic"), vec!["Courier-Oblique"]);
    assert!(needed("ArialNarrow").is_empty());
    assert!(needed("Arial-Black").is_empty());
}

fn page_box(document: &Document, page: u32, key: &[u8]) -> Option<[f32; 4]> {
    let page = document.get_dictionary(document.get_pages()[&page]).ok()?;
    let values = page.get(key).ok()?.as_array().ok()?;
    let values = values
        .iter()
        .map(|value| value.as_float().unwrap())
        .collect::<Vec<_>>();
    values.try_into().ok()
}

fn assert_box(found: Option<[f32; 4]>, expected: [f32; 4]) {
    let found = found.expect("the box is missing");
    assert!(
        found
            .iter()
            .zip(expected)
            .all(|(value, expected)| (value - expected).abs() < 0.01),
        "{found:?} is not {expected:?}"
    );
}

fn crop(page: u32, area: [f32; 4]) -> PageCrop {
    PageCrop { page, area }
}

#[test]
fn crops_the_area_given_from_the_top_left() {
    let input = pdf_with_pages(
        &[(
            "0 0 m",
            dictionary! { "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()] },
        )],
        dictionary! {},
    );
    let output = crop_pdf_bytes(&input, "", &[crop(1, [0.1, 0.25, 0.6, 0.75])]).unwrap();
    let document = Document::load_mem(&output).unwrap();
    assert_box(
        page_box(&document, 1, b"CropBox"),
        [20.0, 100.0, 120.0, 300.0],
    );
    assert_box(
        page_box(&document, 1, b"MediaBox"),
        [20.0, 100.0, 120.0, 300.0],
    );
    assert_eq!(page_contents(&output)[0], "0 0 m");
}

#[test]
fn crops_the_half_the_reader_sees_on_a_turned_page() {
    // The reader's left half of a 200 by 400 page, at each rotation.
    for (rotation, expected) in [
        (0, [0.0, 0.0, 100.0, 400.0]),
        (90, [0.0, 0.0, 200.0, 200.0]),
        (180, [100.0, 0.0, 200.0, 400.0]),
        (270, [0.0, 200.0, 200.0, 400.0]),
    ] {
        let input = pdf_with_pages(
            &[(
                "",
                dictionary! {
                    "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()],
                    "Rotate" => rotation,
                },
            )],
            dictionary! {},
        );
        let output = crop_pdf_bytes(&input, "", &[crop(1, [0.0, 0.0, 0.5, 1.0])]).unwrap();
        let document = Document::load_mem(&output).unwrap();
        assert_box(page_box(&document, 1, b"CropBox"), expected);
    }
}

#[test]
fn crops_within_an_existing_crop_box_and_honours_user_units() {
    let cropped = pdf_with_pages(
        &[(
            "",
            dictionary! {
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "CropBox" => vec![50.into(), 60.into(), 150.into(), 160.into()],
            },
        )],
        dictionary! {},
    );
    let output = crop_pdf_bytes(&cropped, "", &[crop(1, [0.0, 0.0, 0.5, 0.5])]).unwrap();
    let document = Document::load_mem(&output).unwrap();
    assert_box(
        page_box(&document, 1, b"CropBox"),
        [50.0, 110.0, 100.0, 160.0],
    );

    let mut large = square_page(200);
    large.set("UserUnit", 2);
    let input = pdf_with_pages(&[("", large)], dictionary! {});
    let output = crop_pdf_bytes(&input, "", &[crop(1, [0.0, 0.0, 0.5, 0.5])]).unwrap();
    let document = Document::load_mem(&output).unwrap();
    assert_box(
        page_box(&document, 1, b"CropBox"),
        [0.0, 100.0, 100.0, 200.0],
    );
}

#[test]
fn crops_only_the_pages_given_and_leaves_inherited_boxes_alone() {
    let input = pdf_with_pages(
        &[
            ("1", dictionary! {}),
            ("2", dictionary! {}),
            ("3", dictionary! {}),
        ],
        dictionary! { "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()] },
    );
    let output = crop_pdf_bytes(
        &input,
        "",
        &[crop(2, [0.5, 0.0, 1.0, 0.5]), crop(3, [0.0, 0.0, 1.0, 1.0])],
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    assert_eq!(page_box(&document, 1, b"MediaBox"), None);
    assert_eq!(page_box(&document, 1, b"CropBox"), None);
    assert_box(
        page_box(&document, 2, b"CropBox"),
        [50.0, 50.0, 100.0, 100.0],
    );
    // An area covering the whole page changes nothing.
    assert_eq!(page_box(&document, 3, b"CropBox"), None);
    assert_eq!(page_contents(&output), ["1", "2", "3"]);
}

#[test]
fn clips_print_boxes_and_drops_thumbnails() {
    let mut document = Document::load_mem(&pdf_with_pages(
        &[(
            "",
            dictionary! {
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
                "TrimBox" => vec![10.into(), 10.into(), 190.into(), 190.into()],
                "ArtBox" => vec![0.into(), 150.into(), 200.into(), 200.into()],
            },
        )],
        dictionary! {},
    ))
    .unwrap();
    let page = document.get_pages()[&1];
    let thumbnail = document.add_object(Stream::new(dictionary! {}, vec![0; 3]));
    document
        .get_dictionary_mut(page)
        .unwrap()
        .set("Thumb", thumbnail);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    let output = crop_pdf_bytes(&input, "", &[crop(1, [0.0, 0.5, 0.5, 1.0])]).unwrap();
    let document = Document::load_mem(&output).unwrap();
    assert_box(page_box(&document, 1, b"CropBox"), [0.0, 0.0, 100.0, 100.0]);
    assert_box(
        page_box(&document, 1, b"TrimBox"),
        [10.0, 10.0, 100.0, 100.0],
    );
    // The art box lies wholly in the part cropped away.
    assert_eq!(page_box(&document, 1, b"ArtBox"), None);
    let page = document.get_dictionary(document.get_pages()[&1]).unwrap();
    assert!(!page.has(b"Thumb"));
}

#[test]
fn crop_rejects_bad_areas_and_pages() {
    let input = numbered_pdf(2);
    let error = |crops: &[PageCrop]| crop_pdf_bytes(&input, "", crops).unwrap_err();
    assert!(error(&[]).contains("at least one page"));
    assert!(error(&[crop(1, [0.5, 0.0, 0.5, 1.0])]).contains("inside the page"));
    assert!(error(&[crop(1, [0.0, 0.0, 1.2, 1.0])]).contains("inside the page"));
    assert!(error(&[crop(1, [0.0, 0.0, f32::NAN, 1.0])]).contains("inside the page"));
    assert!(error(&[crop(1, [0.0, 0.0, 0.001, 1.0])]).contains("too small"));
    assert!(error(&[crop(3, [0.0, 0.0, 0.5, 1.0])]).contains("between 1 and 2"));
    assert!(
        error(&[crop(1, [0.0, 0.0, 0.5, 1.0]), crop(1, [0.5, 0.0, 1.0, 1.0])])
            .contains("two crop areas")
    );
}

/// Where each signature image lands on the page, in the page's own
/// coordinates, from the forms the page draws.
fn signature_boxes(bytes: &[u8], page: u32) -> Vec<[f32; 4]> {
    let document = Document::load_mem(bytes).unwrap();
    let page_id = document.get_pages()[&page];
    let mut boxes = Vec::new();
    for form in stamp_forms(&document, page_id) {
        let stream = document.get_object(form).unwrap().as_stream().unwrap();
        let content = Content::decode(
            &stream
                .decompressed_content()
                .unwrap_or(stream.content.clone()),
        )
        .unwrap();
        let mut stack = vec![[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]];
        for operation in content.operations {
            match operation.operator.as_str() {
                "q" => stack.push(*stack.last().unwrap()),
                "Q" => {
                    stack.pop();
                }
                "cm" => {
                    let [a, b, c, d, e, f]: [f32; 6] = operation
                        .operands
                        .iter()
                        .map(|value| value.as_float().unwrap())
                        .collect::<Vec<_>>()
                        .try_into()
                        .unwrap();
                    let [ta, tb, tc, td, te, tf] = *stack.last().unwrap();
                    *stack.last_mut().unwrap() = [
                        a * ta + b * tc,
                        a * tb + b * td,
                        c * ta + d * tc,
                        c * tb + d * td,
                        e * ta + f * tc + te,
                        e * tb + f * td + tf,
                    ];
                }
                "Do" => {
                    let [a, b, c, d, e, f] = *stack.last().unwrap();
                    let corners = [(0.0, 0.0), (1.0, 1.0)]
                        .map(|(x, y): (f32, f32)| (a * x + c * y + e, b * x + d * y + f));
                    boxes.push([
                        corners[0].0.min(corners[1].0),
                        corners[0].1.min(corners[1].1),
                        corners[0].0.max(corners[1].0),
                        corners[0].1.max(corners[1].1),
                    ]);
                }
                _ => {}
            }
        }
    }
    boxes
}

fn sign_at(page: u32, place: [f32; 3]) -> SignaturePlacement {
    SignaturePlacement { page, place }
}

#[test]
fn signs_at_the_top_left_and_width_given_keeping_the_image_shape() {
    let input = pdf_with_pages(
        &[(
            "0 0 m",
            dictionary! { "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()] },
        )],
        dictionary! {},
    );
    // The image is twice as wide as it is tall.
    let output =
        add_signature_bytes(&input, "", &rgba_png(), &[sign_at(1, [0.5, 0.25, 0.25])]).unwrap();
    assert_eq!(signature_boxes(&output, 1).len(), 1);
    assert_box(
        signature_boxes(&output, 1).pop(),
        [100.0, 275.0, 150.0, 300.0],
    );
    assert!(page_contents(&output)[0].contains("0 0 m"));
    let document = Document::load_mem(&output).unwrap();
    let has_mask = document.objects.values().any(|object| {
        object
            .as_stream()
            .is_ok_and(|stream| stream.dict.get(b"SMask").is_ok())
    });
    assert!(has_mask, "the signature lost its transparency");
}

#[test]
fn signs_the_corner_the_reader_sees_on_a_turned_page() {
    let input = pdf_with_pages(
        &[(
            "",
            dictionary! {
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()],
                "Rotate" => 90,
            },
        )],
        dictionary! {},
    );
    // Seen turned, the page is 400 by 200, so a quarter of its width is 100
    // points and the top left is the page's own bottom left.
    let output =
        add_signature_bytes(&input, "", &rgba_png(), &[sign_at(1, [0.0, 0.0, 0.25])]).unwrap();
    assert_box(signature_boxes(&output, 1).pop(), [0.0, 0.0, 50.0, 100.0]);
}

#[test]
fn signs_a_page_more_than_once_and_keeps_signatures_on_the_page() {
    let input = numbered_pdf(2);
    let output = add_signature_bytes(
        &input,
        "",
        &rgba_png(),
        &[
            sign_at(2, [0.1, 0.1, 0.25]),
            // Would run off the bottom right corner.
            sign_at(2, [0.9, 0.95, 0.5]),
        ],
    )
    .unwrap();
    assert!(signature_boxes(&output, 1).is_empty());
    let boxes = signature_boxes(&output, 2);
    assert_eq!(boxes.len(), 2);
    assert_box(Some(boxes[0]), [40.0, 490.0, 140.0, 540.0]);
    assert_box(Some(boxes[1]), [200.0, 0.0, 400.0, 100.0]);
    for (content, original) in page_contents(&output).iter().zip(["1", "2"]) {
        assert!(content.contains(original), "{content}");
    }
}

#[test]
fn signing_rejects_bad_placements_and_images() {
    let input = numbered_pdf(2);
    let error = |image: &[u8], placements: &[SignaturePlacement]| {
        add_signature_bytes(&input, "", image, placements).unwrap_err()
    };
    let png = rgba_png();
    assert!(error(&png, &[]).contains("at least one page"));
    assert!(error(&png, &[sign_at(1, [0.5, 0.5, 0.0])]).contains("on the page"));
    assert!(error(&png, &[sign_at(1, [1.0, 0.5, 0.2])]).contains("on the page"));
    assert!(error(&png, &[sign_at(1, [f32::NAN, 0.5, 0.2])]).contains("on the page"));
    assert!(error(&png, &[sign_at(3, [0.5, 0.5, 0.2])]).contains("between 1 and 2"));
    assert!(error(b"not an image", &[sign_at(1, [0.5, 0.5, 0.2])]).contains("JPG and PNG"));
}

/// A 200 point square page carrying `annotations`, each given an appearance
/// stream 100 by 60 that fills its box, plus a catalog form when `fields` is
/// not empty. Annotation dictionaries name their appearance with `"AP" =>
/// "appearance"` placeholders filled in here.
fn annotated_pdf(
    annotations: Vec<Dictionary>,
    form: Option<Dictionary>,
    page: Dictionary,
) -> (Vec<u8>, Vec<ObjectId>) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let appearance = |document: &mut Document, content: &str| {
        document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 60.into()],
                "Resources" => dictionary! {},
            },
            content.as_bytes().to_vec(),
        ))
    };
    let mut ids = Vec::new();
    for mut annotation in annotations {
        if annotation.get(b"AP").ok() == Some(&Object::string_literal("appearance")) {
            let normal = appearance(&mut document, "0 0 100 60 re f");
            annotation.set("AP", dictionary! { "N" => normal });
        } else if annotation.get(b"AP").ok() == Some(&Object::string_literal("states")) {
            let on = appearance(&mut document, "% on");
            let off = appearance(&mut document, "% off");
            annotation.set(
                "AP",
                dictionary! { "N" => dictionary! { "On" => on, "Off" => off } },
            );
        }
        annotation.set("P", page_id);
        ids.push(document.add_object(annotation));
    }
    // Popups point at the annotation before them.
    for index in 0..ids.len() {
        let is_popup = document.get_dictionary(ids[index]).is_ok_and(|annotation| {
            annotation.get(b"Subtype").ok() == Some(&Object::Name(b"Popup".to_vec()))
        });
        if is_popup && index > 0 {
            let parent = ids[index - 1];
            document
                .get_dictionary_mut(ids[index])
                .unwrap()
                .set("Parent", parent);
        }
    }
    let content = document.add_object(Stream::new(dictionary! {}, b"0 0 m".to_vec()));
    let mut page = page;
    page.set("Type", "Page");
    page.set("Parent", pages_id);
    page.set("Contents", content);
    page.set("MediaBox", vec![0.into(), 0.into(), 200.into(), 200.into()]);
    page.set(
        "Annots",
        ids.iter()
            .map(|id| Object::Reference(*id))
            .collect::<Vec<_>>(),
    );
    document.objects.insert(page_id, Object::Dictionary(page));
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
        ),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    if let Some(mut form) = form {
        let widgets = ids
            .iter()
            .filter(|id| {
                document.get_dictionary(**id).is_ok_and(|annotation| {
                    annotation.get(b"Subtype").ok() == Some(&Object::Name(b"Widget".to_vec()))
                })
            })
            .map(|id| Object::Reference(*id))
            .collect::<Vec<_>>();
        form.set("Fields", widgets);
        catalog.set("AcroForm", form);
    }
    let catalog_id = document.add_object(catalog);
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    (bytes, ids)
}

fn square(rect: [i64; 4]) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => rect.iter().map(|&value| Object::Integer(value)).collect::<Vec<_>>(),
        "AP" => Object::string_literal("appearance"),
    }
}

fn text_field(name: &str) -> Dictionary {
    dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal(name),
        "V" => Object::string_literal("filled in"),
        "Rect" => vec![100.into(), 100.into(), 150.into(), 130.into()],
        "AP" => Object::string_literal("appearance"),
    }
}

/// The page's annotation subtypes, and what its flattened drawing says.
fn flattened_page(bytes: &[u8]) -> (Vec<String>, String, Document) {
    let document = Document::load_mem(bytes).unwrap();
    let page_id = document.get_pages()[&1];
    let subtypes = document
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .ok()
        .map(|annotations| {
            annotations
                .as_array()
                .unwrap()
                .iter()
                .map(|annotation| {
                    let annotation = document
                        .get_dictionary(annotation.as_reference().unwrap())
                        .unwrap();
                    String::from_utf8_lossy(annotation.get(b"Subtype").unwrap().as_name().unwrap())
                        .into_owned()
                })
                .collect()
        })
        .unwrap_or_default();
    let drawing = stamp_forms(&document, page_id)
        .into_iter()
        .map(|form| {
            let stream = document.get_object(form).unwrap().as_stream().unwrap();
            String::from_utf8_lossy(
                &stream
                    .decompressed_content()
                    .unwrap_or(stream.content.clone()),
            )
            .into_owned()
        })
        .collect::<String>();
    (subtypes, drawing, document)
}

#[test]
fn flattening_draws_each_appearance_fitted_to_its_rectangle() {
    let (input, _) = annotated_pdf(vec![square([10, 10, 60, 40])], None, dictionary! {});
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    assert_eq!(output.kept, 0);
    let (subtypes, drawing, _) = flattened_page(&output.bytes);
    assert!(subtypes.is_empty(), "{subtypes:?}");
    // 100 by 60 onto 50 by 30.
    assert!(drawing.contains("0.5 0 0 0.5 10 10 cm"), "{drawing}");
    assert!(page_contents(&output.bytes)[0].contains("0 0 m"));
}

#[test]
fn flattening_a_form_removes_its_fields_and_the_form() {
    let (input, _) = annotated_pdf(
        vec![text_field("name")],
        Some(dictionary! { "XFA" => Object::string_literal("<xdp/>") }),
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    let (subtypes, drawing, document) = flattened_page(&output.bytes);
    assert!(subtypes.is_empty());
    assert!(drawing.contains("100 100 cm"), "{drawing}");
    assert!(!document.catalog().unwrap().has(b"AcroForm"));
}

#[test]
fn flattening_draws_the_checkbox_state_shown() {
    let mut checkbox = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Btn",
        "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
        "AP" => Object::string_literal("states"),
        "AS" => "Off",
    };
    let (input, _) = annotated_pdf(vec![checkbox.clone()], Some(dictionary! {}), dictionary! {});
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    let document = Document::load_mem(&output.bytes).unwrap();
    let drawn = document.objects.values().any(|object| {
        object
            .as_stream()
            .is_ok_and(|stream| stream.content.starts_with(b"% off"))
    });
    let unused = document.objects.values().any(|object| {
        object
            .as_stream()
            .is_ok_and(|stream| stream.content.starts_with(b"% on"))
    });
    assert!(
        drawn && !unused,
        "the shown state should be drawn and the other dropped"
    );

    // No state chosen shows nothing, so nothing is drawn but it still goes.
    checkbox.remove(b"AS");
    let (input, _) = annotated_pdf(vec![checkbox], Some(dictionary! {}), dictionary! {});
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    let (subtypes, drawing, _) = flattened_page(&output.bytes);
    assert!(subtypes.is_empty() && drawing.is_empty());
}

#[test]
fn flattening_keeps_what_it_cannot_draw_faithfully() {
    // No stored appearance, and a field the form asks viewers to redraw.
    let mut bare = square([10, 10, 60, 40]);
    bare.remove(b"AP");
    let (input, _) = annotated_pdf(
        vec![bare, text_field("name")],
        Some(dictionary! { "NeedAppearances" => true }),
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    assert_eq!(output.kept, 2);
    let (subtypes, drawing, document) = flattened_page(&output.bytes);
    assert_eq!(subtypes, ["Square", "Widget"]);
    assert!(drawing.is_empty());
    assert!(document.catalog().unwrap().has(b"AcroForm"));
}

#[test]
fn flattening_leaves_links_attachments_and_print_only_marks_and_drops_the_hidden() {
    let mut link = square([0, 0, 10, 10]);
    link.set("Subtype", "Link");
    let mut attachment = square([20, 20, 30, 30]);
    attachment.set("Subtype", "FileAttachment");
    let mut print_only = square([40, 40, 50, 50]);
    print_only.set("F", 32 | 4);
    let mut hidden = square([60, 60, 70, 70]);
    hidden.set("F", 2);
    let note = square([80, 80, 90, 90]);
    let popup = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Popup",
        "Rect" => vec![100.into(), 100.into(), 180.into(), 140.into()],
    };
    let (input, _) = annotated_pdf(
        vec![link, attachment, print_only, hidden, note, popup],
        None,
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    let (subtypes, drawing, _) = flattened_page(&output.bytes);
    assert_eq!(subtypes, ["Link", "FileAttachment", "Square"]);
    // Only the note is drawn; the hidden square went without a trace.
    assert_eq!(drawing.matches(" Do").count(), 1, "{drawing}");
    assert!(drawing.contains("80 80 cm"), "{drawing}");
}

#[test]
fn flattening_form_fields_only_leaves_other_annotations() {
    let (input, _) = annotated_pdf(
        vec![square([10, 10, 60, 40]), text_field("name")],
        Some(dictionary! {}),
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::FormFields).unwrap();
    let (subtypes, drawing, _) = flattened_page(&output.bytes);
    assert_eq!(subtypes, ["Square"]);
    assert_eq!(drawing.matches(" Do").count(), 1);
}

#[test]
fn flattening_keeps_a_no_rotate_annotation_upright_on_a_turned_page() {
    let mut note = square([10, 150, 60, 180]);
    note.set("F", 16);
    let (input, _) = annotated_pdf(vec![note], None, dictionary! { "Rotate" => 90 });
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    let (_, drawing, _) = flattened_page(&output.bytes);
    // Fitted (scale 0.5 at 10, 150), then a quarter turn counterclockwise
    // about the top left corner (10, 180).
    assert!(drawing.contains("0 0.5 -0.5 0 40 180 cm"), "{drawing}");
}

#[test]
fn flattening_draws_form_fields_over_other_annotations() {
    let (input, _) = annotated_pdf(
        vec![text_field("name"), square([10, 10, 60, 40])],
        Some(dictionary! {}),
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    let (_, drawing, _) = flattened_page(&output.bytes);
    let square = drawing.find("10 10 cm").unwrap();
    let field = drawing.find("100 100 cm").unwrap();
    assert!(square < field, "{drawing}");
}

#[test]
fn flattening_removes_empty_fields_that_show_nothing() {
    let mut empty = text_field("blank");
    empty.remove(b"AP");
    empty.remove(b"V");
    let mut boxed = empty.clone();
    boxed.set("T", Object::string_literal("boxed"));
    boxed.set("MK", dictionary! { "BC" => vec![0.into()] });
    let mut drawn = text_field("drawn");
    drawn.remove(b"V");
    let (input, _) = annotated_pdf(
        vec![empty, boxed, drawn],
        Some(dictionary! {}),
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    // The bordered one would be drawn by a viewer, so it stays.
    assert_eq!(output.kept, 1);
    let (subtypes, drawing, _) = flattened_page(&output.bytes);
    assert_eq!(subtypes, ["Widget"]);
    assert_eq!(drawing.matches(" Do").count(), 1, "{drawing}");
}

#[test]
fn flattening_keeps_even_empty_fields_a_form_asks_viewers_to_redraw() {
    let mut empty = text_field("empty");
    empty.remove(b"V");
    let (input, _) = annotated_pdf(
        vec![empty],
        Some(dictionary! { "NeedAppearances" => true }),
        dictionary! {},
    );
    let output = flatten_pdf_bytes(&input, "", FlattenScope::Everything).unwrap();
    assert_eq!(output.kept, 1);
    assert_eq!(flattened_page(&output.bytes).0, ["Widget"]);
}

// Redaction. Pages are 600 by 800 points; areas are fractions of the page
// from its top left, so a point y up the page is at 1 - y / 800.

use crate::{PageImage, RedactOptions, Redacted, Redaction, redact_pdf_bytes};

/// A 600 by 800 page drawing `content`. `build` adds whatever objects the
/// page needs and returns its resources, extra page entries and extra
/// catalog entries.
fn redaction_pdf(
    content: &[u8],
    build: impl FnOnce(&mut Document, ObjectId) -> (Dictionary, Dictionary, Dictionary),
) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let (resources, extra, catalog_extra) = build(&mut document, page_id);
    let content_id = document.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
        "Resources" => resources,
    };
    for (key, value) in extra {
        page.set(key, value);
    }
    document.objects.insert(page_id, Object::Dictionary(page));
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
    for (key, value) in catalog_extra {
        catalog.set(key, value);
    }
    let catalog_id = document.add_object(catalog);
    document.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

/// Helvetica with no widths given, which readers measure from Adobe's metrics.
fn helvetica_resources(document: &mut Document) -> Dictionary {
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    dictionary! { "Font" => dictionary! { "F1" => font } }
}

/// "Hello World" at 20 points from (100, 700): "Hello " is 51.12 points wide
/// and "World" the 52.22 after it.
const HELLO: &[u8] = b"BT /F1 20 Tf 100 700 Td (Hello World) Tj ET";

/// Just over "World": the space before it is under the box by less than the
/// quarter it takes to remove a glyph.
const WORLD: [f32; 4] = [151.0 / 600.0, 0.1, 204.0 / 600.0, 0.1375];

fn redact(input: &[u8], areas: &[(u32, [f32; 4])]) -> Redacted {
    redact_with(input, areas, &[], false)
}

fn redact_with(
    input: &[u8],
    areas: &[(u32, [f32; 4])],
    page_images: &[PageImage<'_>],
    remove_metadata: bool,
) -> Redacted {
    let redactions = areas
        .iter()
        .map(|&(page, area)| Redaction { page, area })
        .collect::<Vec<_>>();
    redact_pdf_bytes(
        input,
        "",
        RedactOptions {
            redactions: &redactions,
            color: [0.0, 0.0, 0.0],
            remove_metadata,
            page_images,
        },
    )
    .unwrap()
}

fn redacted(result: Redacted) -> Vec<u8> {
    match result {
        Redacted::Done { bytes, imaged } => {
            assert!(imaged.is_empty(), "drawn from pictures: {imaged:?}");
            bytes
        }
        Redacted::NeedsImages(pages) => panic!("pages {pages:?} need pictures"),
    }
}

/// Every stream in the file, decoded, as text.
fn every_stream(bytes: &[u8]) -> String {
    let document = Document::load_mem(bytes).unwrap();
    document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .map(|stream| {
            String::from_utf8_lossy(
                &stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            )
            .into_owned()
        })
        .collect()
}

fn first_page_content(bytes: &[u8]) -> String {
    let document = Document::load_mem(bytes).unwrap();
    String::from_utf8_lossy(&document.get_page_content(document.get_pages()[&1])).into_owned()
}

#[test]
fn redaction_removes_the_text_under_the_box_and_keeps_the_rest_in_place() {
    let input = redaction_pdf(HELLO, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let output = redacted(redact(&input, &[(1, WORLD)]));
    let content = first_page_content(&output);
    // "Hello " stays, and the pen moves on by the width of "World", so
    // anything after it on the line stays where it was.
    assert!(content.contains("[<48656C6C6F20> -2611] TJ"), "{content}");
    let everything = every_stream(&output);
    assert!(!everything.contains("World"), "{everything}");
    // The box is painted over where the text was.
    assert!(
        content.contains("0 0 0 rg\n151 690 53 30 re\nf"),
        "{content}"
    );
}

#[test]
fn redaction_copies_what_it_leaves_alone_byte_for_byte() {
    let content = b"0.123456789   0 0 1 0 0 cm\n1 0 0 RG 0 0 m 10 10 l S\n% a comment\nBT /F1 20 Tf 100 700 Td (Hello World) Tj ET";
    let input = redaction_pdf(content, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    // Over nothing at all.
    let output = redacted(redact(&input, &[(1, [0.5, 0.5, 0.6, 0.6])]));
    let written = first_page_content(&output);
    for kept in [
        "0.123456789   0 0 1 0 0 cm",
        "1 0 0 RG",
        "0 0 m",
        "10 10 l",
        "(Hello World) Tj",
    ] {
        assert!(written.contains(kept), "{kept} in {written}");
    }
}

#[test]
fn redaction_removes_glyphs_of_a_composite_font_and_reads_their_text() {
    let content = b"BT /F2 10 Tf 50 500 Td <000100020003> Tj ET";
    let input = redaction_pdf(content, |document, _| {
        let to_unicode = document.add_object(Stream::new(
            dictionary! {},
            b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
              1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
              1 beginbfrange <0001> <0003> <0041> endbfrange\n\
              endcmap CMapName currentdict /CMap defineresource pop end end"
                .to_vec(),
        ));
        let descriptor = document.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "Example",
            "Ascent" => 900,
            "Descent" => -200,
        });
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "Example",
            "Encoding" => "Identity-H",
            "ToUnicode" => to_unicode,
            "DescendantFonts" => vec![dictionary! {
                "Type" => "Font",
                "Subtype" => "CIDFontType2",
                "BaseFont" => "Example",
                "FontDescriptor" => descriptor,
                "W" => vec![1.into(), vec![500.into(), 600.into(), 700.into()].into()],
            }
            .into()],
        });
        (
            dictionary! { "Font" => dictionary! { "F2" => font } },
            dictionary! {},
            dictionary! {},
        )
    });

    let texts = crate::page_texts(&input, "").unwrap();
    assert_eq!(texts[0].text.concat(), "ABC");
    let [left, top, right, bottom] = texts[0].boxes[1];
    assert!((left * 600.0 - 55.0).abs() < 0.01, "{left}");
    assert!((right * 600.0 - 61.0).abs() < 0.01, "{right}");
    assert!(((1.0 - top) * 800.0 - 509.0).abs() < 0.01, "{top}");
    assert!(((1.0 - bottom) * 800.0 - 498.0).abs() < 0.01, "{bottom}");

    // Over most of B and none of A or C.
    let area = [
        55.5 / 600.0,
        1.0 - 510.0 / 800.0,
        60.5 / 600.0,
        1.0 - 495.0 / 800.0,
    ];
    let output = redacted(redact(&input, &[(1, area)]));
    let content = first_page_content(&output);
    assert!(content.contains("[<0001> -600 <0003>] TJ"), "{content}");
    assert_eq!(
        crate::page_texts(&output, "").unwrap()[0].text.concat(),
        "AC"
    );
}

#[test]
fn redaction_finds_text_where_readers_show_it_on_a_turned_page() {
    let input = redaction_pdf(HELLO, |document, _| {
        (
            helvetica_resources(document),
            dictionary! { "Rotate" => 90 },
            dictionary! {},
        )
    });
    // The page is 800 wide and 600 tall as shown; "World" runs down it.
    let output = redacted(redact(&input, &[(1, [0.865, 0.252, 0.9, 0.34])]));
    assert!(first_page_content(&output).contains("[<48656C6C6F20> -2611] TJ"));
    let texts = crate::page_texts(&input, "").unwrap();
    let [left, top, ..] = texts[0].boxes[0];
    // "H" at (100, 696) to (114.44, 716) on the page.
    assert!((left * 800.0 - 696.0).abs() < 0.01, "{left}");
    assert!((top * 600.0 - 100.0).abs() < 0.01, "{top}");
}

#[test]
fn redaction_asks_for_a_picture_when_text_under_a_box_cannot_be_measured() {
    let input = redaction_pdf(HELLO, |document, _| {
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "Mystery",
        });
        (
            dictionary! { "Font" => dictionary! { "F1" => font } },
            dictionary! {},
            dictionary! {},
        )
    });
    assert!(matches!(
        redact(&input, &[(1, WORLD)]),
        Redacted::NeedsImages(pages) if pages.iter().map(|(page, _)| *page).eq([1])
    ));
    // Nowhere near its line, the text is no concern.
    redacted(redact(&input, &[(1, [0.1, 0.8, 0.3, 0.9])]));

    let picture = rgba_png();
    let output = redact_with(
        &input,
        &[(1, WORLD)],
        &[PageImage {
            page: 1,
            image: &picture,
        }],
        false,
    );
    let Redacted::Done { bytes, imaged } = output else {
        panic!("a picture was given");
    };
    assert_eq!(imaged, [1]);
    let content = first_page_content(&bytes);
    assert!(content.contains("/Page Do"), "{content}");
    assert!(!every_stream(&bytes).contains("World"));
}

/// A 4 by 4 image drawn over (0, 0) to (100, 100), every sample 200.
fn image_pdf(filter: Option<&str>, samples: Vec<u8>, components: i64) -> Vec<u8> {
    redaction_pdf(b"q 100 0 0 100 0 0 cm /Im1 Do Q", |document, _| {
        let mut dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 4,
            "Height" => 4,
            "ColorSpace" => if components == 1 { "DeviceGray" } else { "DeviceRGB" },
            "BitsPerComponent" => 8,
        };
        if let Some(filter) = filter {
            dict.set("Filter", filter);
        }
        let image = document.add_object(Stream::new(dict, samples));
        (
            dictionary! { "XObject" => dictionary! { "Im1" => image } },
            dictionary! {},
            dictionary! {},
        )
    })
}

/// The page's images, decoded.
fn page_images(bytes: &[u8]) -> Vec<Vec<u8>> {
    let document = Document::load_mem(bytes).unwrap();
    document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| stream.dict.get(b"Subtype").ok() == Some(&Object::Name(b"Image".to_vec())))
        .map(|stream| {
            stream
                .decompressed_content()
                .unwrap_or(stream.content.clone())
        })
        .collect()
}

#[test]
fn redaction_overwrites_the_pixels_under_the_box_in_a_copy() {
    let input = image_pdf(None, vec![200; 48], 3);
    // The left half: pixel centres at 12.5 and 37.5 points.
    let output = redacted(redact(&input, &[(1, [0.0, 0.8, 50.0 / 600.0, 1.0])]));
    let images = page_images(&output);
    assert_eq!(images.len(), 1, "the original is gone");
    for row in images[0].chunks(12) {
        assert_eq!(row, [0, 0, 0, 0, 0, 0, 200, 200, 200, 200, 200, 200]);
    }
    assert!(first_page_content(&output).contains("/Redacted1 Do"));

    // All of it under the box: it is not drawn at all.
    let output = redacted(redact(&input, &[(1, [0.0, 0.8, 0.5, 1.0])]));
    assert!(page_images(&output).is_empty());
    assert!(!first_page_content(&output).contains("Do"));
}

#[test]
fn redaction_reencodes_a_jpeg_with_the_pixels_under_the_box_painted() {
    let jpeg = jpeg_bytes(32, 32, 95);
    let original = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&jpeg))
        .decode()
        .unwrap();
    let input = redaction_pdf(b"q 160 0 0 160 0 0 cm /Im1 Do Q", |document, _| {
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 32,
                "Height" => 32,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            jpeg.clone(),
        ));
        (
            dictionary! { "XObject" => dictionary! { "Im1" => image } },
            dictionary! {},
            dictionary! {},
        )
    });
    let output = redacted(redact(&input, &[(1, [0.0, 0.8, 80.0 / 600.0, 1.0])]));
    let document = Document::load_mem(&output).unwrap();
    let image = document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .find(|stream| is_jpeg_image(&stream.dict))
        .expect("still a JPEG");
    let mut decoder =
        zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&image.content));
    let pixels = decoder.decode().unwrap();
    let at =
        |pixels: &[u8], x: usize, y: usize| pixels[(y * 32 + x) * 3..(y * 32 + x) * 3 + 3].to_vec();
    assert!(
        at(&pixels, 4, 8).iter().all(|&channel| channel < 16),
        "{:?}",
        at(&pixels, 4, 8)
    );
    // Right of the box, past the block its edge lies in: what the source had.
    let (after, before) = (at(&pixels, 24, 8), at(&original, 24, 8));
    assert!(
        after
            .iter()
            .zip(&before)
            .all(|(a, b)| (i32::from(*a) - i32::from(*b)).abs() < 8),
        "{after:?} {before:?}"
    );
}

#[test]
fn redaction_asks_for_a_picture_for_an_image_it_cannot_decode() {
    let input = image_pdf(Some("CCITTFaxDecode"), vec![0; 8], 1);
    assert!(matches!(
        redact(&input, &[(1, [0.0, 0.8, 50.0 / 600.0, 1.0])]),
        Redacted::NeedsImages(pages) if pages.iter().map(|(page, _)| *page).eq([1])
    ));
    // Whole, it can simply go.
    redacted(redact(&input, &[(1, [0.0, 0.8, 0.5, 1.0])]));
}

#[test]
fn redaction_paints_an_unfiltered_inline_image_in_place() {
    let mut content = b"q 100 0 0 100 0 0 cm BI /W 2 /H 2 /CS /G /BPC 8 ID ".to_vec();
    content.extend_from_slice(&[0x80, 0x80, 0x80, 0x80]);
    content.extend_from_slice(b" EI Q BT /F1 20 Tf 100 700 Td (Hello World) Tj ET");
    let input = redaction_pdf(&content, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let output = redacted(redact(&input, &[(1, [0.0, 0.8, 50.0 / 600.0, 1.0])]));
    let document = Document::load_mem(&output).unwrap();
    let written = document.get_page_content(document.get_pages()[&1]);
    let data = written
        .windows(4)
        .position(|window| window == b" ID ")
        .map(|at| &written[at + 4..at + 8])
        .unwrap();
    assert_eq!(data, [0, 0x80, 0, 0x80]);
    // The text after it was read past the image correctly.
    assert!(String::from_utf8_lossy(&written).contains("(Hello World) Tj"));
}

/// Signed area of each closed subpath painted by `f`, from `m`, `l`, `h`.
fn filled_area(content: &str) -> f64 {
    let operations = Content::decode(content.as_bytes()).unwrap().operations;
    let mut area = 0.0;
    let mut points: Vec<(f64, f64)> = Vec::new();
    let close = |points: &mut Vec<(f64, f64)>| {
        let mut twice = 0.0;
        for index in 0..points.len() {
            let (x0, y0) = points[index];
            let (x1, y1) = points[(index + 1) % points.len()];
            twice += x0 * y1 - x1 * y0;
        }
        points.clear();
        twice.abs() / 2.0
    };
    let mut pending = 0.0;
    for operation in operations {
        let numbers = operation
            .operands
            .iter()
            .filter_map(|operand| operand.as_float().ok().map(f64::from))
            .collect::<Vec<_>>();
        match operation.operator.as_str() {
            "m" => {
                pending += close(&mut points);
                points.push((numbers[0], numbers[1]));
            }
            "l" => points.push((numbers[0], numbers[1])),
            "h" => pending += close(&mut points),
            "f" | "f*" => {
                pending += close(&mut points);
                area += pending;
                pending = 0.0;
            }
            "re" | "S" | "n" => {
                points.clear();
                pending = 0.0;
            }
            _ => {}
        }
    }
    area
}

#[test]
fn redaction_cuts_the_box_out_of_filled_and_stroked_paths() {
    let input = redaction_pdf(
        b"0 0 1 rg 10 10 m 110 10 l 110 110 l 10 110 l h f 0 50 m 200 50 l S",
        |_, _| (dictionary! {}, dictionary! {}, dictionary! {}),
    );
    // x 50 to 150, y 40 to 150: over part of the square and the line.
    let area = [
        50.0 / 600.0,
        1.0 - 150.0 / 800.0,
        150.0 / 600.0,
        1.0 - 40.0 / 800.0,
    ];
    let output = redacted(redact(&input, &[(1, area)]));
    let content = first_page_content(&output);
    // The box's own fill is drawn with re, which the sum leaves out.
    let square_left = 100.0 * 100.0 - 60.0 * 70.0;
    assert!(
        (filled_area(&content) - square_left).abs() < 0.01,
        "{content}"
    );
    assert!(
        content.contains("0 50 m\n50 50 l\n150 50 m\n200 50 l\nS"),
        "{content}"
    );
}

#[test]
fn redaction_redacts_a_copy_of_a_form_and_leaves_other_pages_its_original() {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let resources = helvetica_resources(&mut document);
    let form = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
            "Resources" => resources,
        },
        HELLO.to_vec(),
    ));
    let kids = (0..2)
        .map(|_| {
            let content = document.add_object(Stream::new(dictionary! {}, b"/Fm1 Do".to_vec()));
            document
                .add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "Contents" => content,
                    "MediaBox" => vec![0.into(), 0.into(), 600.into(), 800.into()],
                    "Resources" => dictionary! { "XObject" => dictionary! { "Fm1" => form } },
                })
                .into()
        })
        .collect::<Vec<Object>>();
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 2 }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    let output = redacted(redact(&input, &[(1, WORLD)]));
    let texts = crate::page_texts(&output, "").unwrap();
    assert_eq!(texts[0].text.concat(), "Hello ");
    assert_eq!(texts[1].text.concat(), "Hello World");
    assert!(first_page_content(&output).contains("/Redacted1 Do"));
}

#[test]
fn redaction_removes_annotations_and_fields_under_the_box() {
    let input = redaction_pdf(b"", |document, page_id| {
        let comment = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![160.into(), 695.into(), 180.into(), 715.into()],
            "Contents" => Object::string_literal("a private note"),
            "P" => page_id,
        });
        let elsewhere = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Text",
            "Rect" => vec![400.into(), 100.into(), 420.into(), 120.into()],
            "Contents" => Object::string_literal("kept"),
        });
        let field = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("name"),
            "V" => Object::string_literal("a private name"),
            "Rect" => vec![150.into(), 700.into(), 200.into(), 710.into()],
        });
        (
            dictionary! {},
            dictionary! { "Annots" => vec![comment.into(), elsewhere.into(), field.into()] },
            dictionary! { "AcroForm" => dictionary! { "Fields" => vec![field.into()] } },
        )
    });
    let output = redacted(redact(&input, &[(1, WORLD)]));
    let document = Document::load_mem(&output).unwrap();
    let raw = String::from_utf8_lossy(&output);
    assert!(!raw.contains("private"), "{raw}");
    let page = document.get_dictionary(document.get_pages()[&1]).unwrap();
    assert_eq!(page.get(b"Annots").unwrap().as_array().unwrap().len(), 1);
    assert!(document.catalog().unwrap().get(b"AcroForm").is_err());
}

#[test]
fn redaction_drops_text_that_describes_what_was_removed() {
    let content = b"/P <</MCID 0>> BDC /Span <</ActualText (Hello World)>> BDC BT /F1 20 Tf 100 700 Td (Hello World) Tj ET EMC EMC /P <</MCID 1>> BDC BT /F1 20 Tf 100 100 Td (Other) Tj ET EMC";
    let input = redaction_pdf(content, |document, page_id| {
        let resources = helvetica_resources(document);
        let root = document.new_object_id();
        let touched = document.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => "P",
            "P" => root,
            "Pg" => page_id,
            "K" => 0,
            "Alt" => Object::string_literal("Hello World"),
        });
        let untouched = document.add_object(dictionary! {
            "Type" => "StructElem",
            "S" => "P",
            "P" => root,
            "Pg" => page_id,
            "K" => 1,
            "Alt" => Object::string_literal("Other"),
        });
        document.objects.insert(
            root,
            Object::Dictionary(dictionary! {
                "Type" => "StructTreeRoot",
                "K" => vec![touched.into(), untouched.into()],
            }),
        );
        (
            resources,
            dictionary! { "StructParents" => 0 },
            dictionary! { "StructTreeRoot" => root, "MarkInfo" => dictionary! { "Marked" => true } },
        )
    });
    let output = redacted(redact(&input, &[(1, WORLD)]));
    let raw = String::from_utf8_lossy(&output);
    assert!(!raw.contains("World"));
    assert!(!every_stream(&output).contains("World"));
    let content = first_page_content(&output);
    assert!(content.contains("/Span BMC"), "{content}");
    let document = Document::load_mem(&output).unwrap();
    let alts = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter_map(|element| element.get(b"Alt").ok())
        .count();
    assert_eq!(alts, 1, "the untouched element keeps its alternate text");
}

#[test]
fn redaction_removes_metadata_when_asked() {
    let input = redaction_pdf(HELLO, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let mut document = Document::load_mem(&input).unwrap();
    let info =
        document.add_object(dictionary! { "Title" => Object::string_literal("Hello World") });
    document.trailer.set("Info", info);
    let mut with_info = Vec::new();
    document.save_to(&mut with_info).unwrap();

    let kept = redacted(redact(&with_info, &[(1, WORLD)]));
    assert!(Document::load_mem(&kept).unwrap().trailer.has(b"Info"));
    let removed = redacted(redact_with(&with_info, &[(1, WORLD)], &[], true));
    assert!(!Document::load_mem(&removed).unwrap().trailer.has(b"Info"));
}

#[test]
fn redaction_rejects_bad_areas_and_pages() {
    let input = redaction_pdf(HELLO, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let error = |areas: &[Redaction]| {
        redact_pdf_bytes(
            &input,
            "",
            RedactOptions {
                redactions: areas,
                color: [0.0; 3],
                remove_metadata: false,
                page_images: &[],
            },
        )
        .err()
        .unwrap()
    };
    assert!(error(&[]).contains("at least one"));
    assert!(
        error(&[Redaction {
            page: 1,
            area: [0.5, 0.5, 0.4, 0.6]
        }])
        .contains("inside")
    );
    assert!(
        error(&[Redaction {
            page: 1,
            area: [0.0, 0.0, 1.5, 0.6]
        }])
        .contains("inside")
    );
    assert!(
        error(&[Redaction {
            page: 2,
            area: [0.0, 0.0, 0.5, 0.5]
        }])
        .contains("between 1 and 1")
    );
}

#[test]
fn redaction_keeps_the_part_of_a_form_field_outside_the_box() {
    let input = redaction_pdf(b"", |document, _| {
        let resources = helvetica_resources(document);
        // "Hello World" as the field shows it, drawn from its own origin.
        let appearance = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 300.into(), 30.into()],
                "Resources" => resources,
            },
            b"BT /F1 20 Tf 0 4 Td (Hello World) Tj ET".to_vec(),
        ));
        let field = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Tx",
            "T" => Object::string_literal("name"),
            "V" => Object::string_literal("Hello World"),
            "Rect" => vec![100.into(), 696.into(), 400.into(), 726.into()],
            "AP" => dictionary! { "N" => appearance },
        });
        (
            dictionary! {},
            dictionary! { "Annots" => vec![field.into()] },
            dictionary! { "AcroForm" => dictionary! { "Fields" => vec![field.into()] } },
        )
    });
    let output = redacted(redact(&input, &[(1, WORLD)]));
    let document = Document::load_mem(&output).unwrap();
    let page = document.get_dictionary(document.get_pages()[&1]).unwrap();
    assert!(page.get(b"Annots").is_err(), "the field is gone");
    assert!(document.catalog().unwrap().get(b"AcroForm").is_err());
    // What it showed outside the box is now part of the page.
    assert_eq!(
        crate::page_texts(&output, "").unwrap()[0].text.concat(),
        "Hello "
    );
    assert!(!String::from_utf8_lossy(&output).contains("World"));
    assert!(!every_stream(&output).contains("World"));
}

#[test]
fn redaction_moves_the_pen_past_character_spacing_too() {
    // "a b c" at 10 points with 2 points between characters and 5 more
    // after spaces: b starts at 117.34 and moves the pen 5.56 + 2.
    let input = redaction_pdf(
        b"BT /F1 10 Tf 2 Tc 5 Tw 100 500 Td (a b c) Tj ET",
        |document, _| {
            (
                helvetica_resources(document),
                dictionary! {},
                dictionary! {},
            )
        },
    );
    let area = [
        117.5 / 600.0,
        1.0 - 510.0 / 800.0,
        122.5 / 600.0,
        1.0 - 495.0 / 800.0,
    ];
    let output = redacted(redact(&input, &[(1, area)]));
    let content = first_page_content(&output);
    assert!(content.contains("[<6120> -756 <2063>] TJ"), "{content}");
    let texts = crate::page_texts(&output, "").unwrap();
    // c is where it was, past b and the space after it, which word spacing
    // widens: 117.34 + 7.56 + 9.78.
    let c = texts[0].text.iter().position(|text| text == "c").unwrap();
    assert!((texts[0].boxes[c][0] * 600.0 - 134.68).abs() < 0.01);
}

#[test]
fn redaction_undoes_png_average_prediction_itself() {
    // lopdf 0.44 decodes PNG Average rows wrongly (pdf.js corpus:
    // issue14814.pdf). Rows here are Average-filtered, with the parameters
    // held by reference, which lopdf alone would ignore.
    let (width, height) = (4usize, 2usize);
    let pixels = (0..width * height * 3)
        .map(|index| (index * 37 % 251) as u8)
        .collect::<Vec<_>>();
    let row = width * 3;
    let mut filtered = Vec::new();
    for y in 0..height {
        filtered.push(3);
        for x in 0..row {
            let left = if x >= 3 { pixels[y * row + x - 3] } else { 0 };
            let above = if y > 0 { pixels[(y - 1) * row + x] } else { 0 };
            let average = ((u16::from(left) + u16::from(above)) / 2) as u8;
            filtered.push(pixels[y * row + x].wrapping_sub(average));
        }
    }
    let compressed = deflate_best(&filtered).unwrap();
    let input = redaction_pdf(b"q 100 0 0 100 0 0 cm /Im1 Do Q", |document, _| {
        let parameters = document.add_object(dictionary! {
            "Predictor" => 15,
            "Colors" => 3,
            "Columns" => 4,
            "BitsPerComponent" => 8,
        });
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 4,
                "Height" => 2,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "FlateDecode",
                "DecodeParms" => parameters,
            },
            compressed,
        ));
        (
            dictionary! { "XObject" => dictionary! { "Im1" => image } },
            dictionary! {},
            dictionary! {},
        )
    });
    let output = redacted(redact(&input, &[(1, [0.0, 0.8, 50.0 / 600.0, 1.0])]));
    let images = page_images(&output);
    assert_eq!(images.len(), 1);
    for y in 0..height {
        let line = &images[0][y * row..(y + 1) * row];
        assert_eq!(&line[..6], [0; 6], "row {y}: the left half is painted");
        assert_eq!(&line[6..], &pixels[y * row + 6..(y + 1) * row], "row {y}");
    }
}

#[test]
fn redaction_leaves_a_pixel_that_still_shows_outside_the_box() {
    // A one-pixel mask stretched into a rule across the page, as in pdf.js
    // corpus issue4436r.pdf: the box hides part of its only pixel, whose
    // colour shows either side, so the rule stays whole.
    let mut content = b"q 180 0 0 1 10 600 cm BI /IM true /W 1 /H 1 /BPC 1 ID ".to_vec();
    content.push(0);
    content.extend_from_slice(b" EI Q");
    let input = redaction_pdf(&content, |_, _| {
        (dictionary! {}, dictionary! {}, dictionary! {})
    });
    let output = redacted(redact(&input, &[(1, [0.1, 0.2, 0.2, 0.3])]));
    let written = first_page_content(&output);
    assert!(
        written.contains("BI /IM true /W 1 /H 1 /BPC 1 ID "),
        "{written}"
    );
}

#[test]
fn redaction_reads_cids_an_encoding_cmap_gives_with_bfchar() {
    // As bug920426.pdf in the pdf.js corpus does: codes map to CIDs through
    // bfchar, and only CID 2 has a width of its own.
    let content = b"BT /F2 10 Tf 50 500 Td <004100420043> Tj ET";
    let input = redaction_pdf(content, |document, _| {
        let encoding = document.add_object(Stream::new(
            dictionary! { "Type" => "CMap", "CMapName" => "Example" },
            b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
              1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
              1 beginbfchar <0042> <0002> endbfchar\n\
              1 beginbfrange <0041> <0041> <0001> endbfrange\n\
              endcmap end end"
                .to_vec(),
        ));
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "Example",
            "Encoding" => encoding,
            "DescendantFonts" => vec![dictionary! {
                "Type" => "Font",
                "Subtype" => "CIDFontType2",
                "BaseFont" => "Example",
                "DW" => 500,
                "W" => vec![2.into(), vec![1000.into()].into()],
            }
            .into()],
        });
        (
            dictionary! { "Font" => dictionary! { "F2" => font } },
            dictionary! {},
            dictionary! {},
        )
    });
    let texts = crate::page_texts(&input, "").unwrap();
    let left = |index: usize| texts[0].boxes[index][0] * 600.0;
    // A at 50, B (CID 2, 10 points wide) at 55, the third code at 65.
    assert!((left(1) - 55.0).abs() < 0.01, "{}", left(1));
    assert!((left(2) - 65.0).abs() < 0.01, "{}", left(2));
}

fn annotation(page: u32, kind: crate::AnnotationKind<'static>) -> crate::Annotation<'static> {
    crate::Annotation {
        page,
        kind,
        color: [1.0, 0.0, 0.0],
        opacity: 1.0,
        comment: "",
    }
}

/// The page's annotations as dictionaries, with each one's appearance content.
fn page_annotations(bytes: &[u8], page: u32) -> Vec<(Dictionary, String)> {
    let document = Document::load_mem(bytes).unwrap();
    let page_id = document.get_pages()[&page];
    let Ok(annotations) = document
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .and_then(Object::as_array)
    else {
        return Vec::new();
    };
    annotations
        .iter()
        .map(|entry| {
            let annotation = document
                .get_dictionary(entry.as_reference().unwrap())
                .unwrap()
                .clone();
            let normal = annotation
                .get(b"AP")
                .and_then(Object::as_dict)
                .and_then(|appearance| appearance.get(b"N"))
                .and_then(Object::as_reference)
                .unwrap();
            let stream = document.get_object(normal).unwrap().as_stream().unwrap();
            let content = String::from_utf8_lossy(
                &stream
                    .decompressed_content()
                    .unwrap_or(stream.content.clone()),
            )
            .into_owned();
            assert_eq!(
                floats(stream.dict.get(b"BBox").unwrap()),
                floats(annotation.get(b"Rect").unwrap()),
                "the appearance would be fitted rather than drawn as written"
            );
            (annotation, content)
        })
        .collect()
}

fn floats(value: &Object) -> Vec<f32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_float().unwrap())
        .collect()
}

fn subtype(annotation: &Dictionary) -> String {
    String::from_utf8_lossy(annotation.get(b"Subtype").unwrap().as_name().unwrap()).into_owned()
}

#[test]
fn annotates_with_appearances_placed_where_the_reader_points() {
    use crate::{AnnotationKind, Markup, Shape};
    let input = pdf_with_pages(&[("0 0 m", square_page(200))], dictionary! {});
    let output = crate::annotate_pdf_bytes(
        &input,
        "",
        &[
            annotation(
                1,
                AnnotationKind::Markup {
                    style: Markup::Highlight,
                    boxes: vec![[0.1, 0.1, 0.5, 0.2]],
                },
            ),
            annotation(
                1,
                AnnotationKind::Shape {
                    shape: Shape::Rectangle,
                    area: [0.5, 0.5, 1.0, 1.0],
                    width: 2.0,
                    fill: None,
                },
            ),
            annotation(
                1,
                AnnotationKind::Ink {
                    strokes: vec![vec![[0.0, 0.0], [0.5, 0.5], [1.0, 0.0]]],
                    width: 4.0,
                },
            ),
        ],
        &[],
        false,
    )
    .unwrap();
    let annotations = page_annotations(&output, 1);
    assert_eq!(
        annotations
            .iter()
            .map(|(found, _)| subtype(found))
            .collect::<Vec<_>>(),
        ["Highlight", "Square", "Ink"]
    );
    let (highlight, content) = &annotations[0];
    assert_box(
        floats(highlight.get(b"Rect").unwrap()).try_into().ok(),
        [20.0, 160.0, 100.0, 180.0],
    );
    // Upper left, upper right, lower left, lower right.
    assert_eq!(
        floats(highlight.get(b"QuadPoints").unwrap()),
        [20.0, 180.0, 100.0, 180.0, 20.0, 160.0, 100.0, 160.0]
    );
    assert!(content.contains("20 160 80 20 re"), "{content}");
    assert_eq!(highlight.get(b"F").unwrap().as_i64().unwrap(), 4);
    let (square, content) = &annotations[1];
    assert_box(
        floats(square.get(b"Rect").unwrap()).try_into().ok(),
        [100.0, 0.0, 200.0, 100.0],
    );
    // The border sits inside the box.
    assert!(content.contains("101 1 98 98 re"), "{content}");
    let (ink, _) = &annotations[2];
    assert_eq!(
        floats(&ink.get(b"InkList").unwrap().as_array().unwrap()[0]),
        [0.0, 200.0, 100.0, 100.0, 200.0, 200.0]
    );
    assert_box(
        floats(ink.get(b"Rect").unwrap()).try_into().ok(),
        [-2.0, 98.0, 202.0, 202.0],
    );
    assert!(page_contents(&output)[0].contains("0 0 m"));
}

#[test]
fn annotates_the_spot_the_reader_sees_on_a_turned_page() {
    use crate::AnnotationKind;
    let input = pdf_with_pages(
        &[(
            "",
            dictionary! {
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()],
                "Rotate" => 90,
            },
        )],
        dictionary! {},
    );
    // Seen turned, the page is 400 by 200 and its top left is the page's own
    // bottom left.
    let output = crate::annotate_pdf_bytes(
        &input,
        "",
        &[annotation(
            1,
            AnnotationKind::Line {
                from: [0.0, 0.0],
                to: [0.25, 0.0],
                width: 1.0,
                arrow: false,
            },
        )],
        &[],
        false,
    )
    .unwrap();
    let (line, content) = &page_annotations(&output, 1)[0];
    assert_eq!(floats(line.get(b"L").unwrap()), [0.0, 0.0, 0.0, 100.0]);
    // Drawn through the frame, so the appearance turns with the page.
    assert!(content.contains("0 1 -1 0 200 0 cm"), "{content}");
}

#[test]
fn text_boxes_wrap_in_the_box_and_keep_their_font() {
    use crate::{AnnotationKind, FontFamily};
    let input = pdf_with_pages(&[("", square_page(200))], dictionary! {});
    let output = crate::annotate_pdf_bytes(
        &input,
        "",
        &[crate::Annotation {
            comment: "Résumé",
            ..annotation(
                1,
                AnnotationKind::Text {
                    // 60 points wide, so 56 inside the padding.
                    area: [0.0, 0.0, 0.3, 0.5],
                    text: "Hello there world",
                    family: FontFamily::Courier,
                    bold: false,
                    size: 10.0,
                    fill: Some([1.0, 1.0, 0.0]),
                },
            )
        }],
        &[],
        false,
    )
    .unwrap();
    let (text, content) = &page_annotations(&output, 1)[0];
    assert_eq!(subtype(text), "FreeText");
    // Courier is 6 points a letter at 10 points: "Hello" fits, "Hello there"
    // does not.
    let lines = content.matches(" Tj").count();
    assert_eq!(lines, 3, "{content}");
    assert!(content.contains("<48656C6C6F> Tj"), "{content}");
    assert_eq!(floats(text.get(b"C").unwrap()), [1.0, 1.0, 0.0]);
    let comment = lopdf::decode_text_string(text.get(b"Contents").unwrap()).unwrap();
    assert_eq!(comment, "Résumé");

    let refused = crate::annotate_pdf_bytes(
        &input,
        "",
        &[annotation(
            1,
            AnnotationKind::Text {
                area: [0.0, 0.0, 0.5, 0.5],
                text: "日本",
                family: FontFamily::Helvetica,
                bold: false,
                size: 10.0,
                fill: None,
            },
        )],
        &[],
        false,
    );
    assert!(refused.unwrap_err().contains("“日”"));
}

#[test]
fn wraps_long_words_inside_and_keeps_line_breaks() {
    use crate::FontFamily;
    let lines = crate::annotate::wrap("ab\n\nabcdefgh", FontFamily::Courier, false, 10.0, 30.0);
    assert_eq!(
        lines,
        [
            b"ab".to_vec(),
            Vec::new(),
            b"abcde".to_vec(),
            b"fgh".to_vec()
        ]
    );
}

#[test]
fn flattened_annotations_are_drawn_into_the_page() {
    use crate::{AnnotationKind, Shape};
    let input = pdf_with_pages(&[("0 0 m", square_page(200))], dictionary! {});
    let ellipse = || {
        annotation(
            1,
            AnnotationKind::Shape {
                shape: Shape::Ellipse,
                area: [0.0, 0.0, 0.5, 0.5],
                width: 1.0,
                fill: Some([0.0, 0.0, 1.0]),
            },
        )
    };
    let output = crate::annotate_pdf_bytes(&input, "", &[ellipse(), ellipse()], &[], true).unwrap();
    assert!(page_annotations(&output, 1).is_empty());
    let (_, drawing, _) = flattened_page(&output);
    assert_eq!(drawing.matches("Do").count(), 2, "{drawing}");
    assert!(page_contents(&output)[0].contains("0 0 m"));

    let note = crate::Annotation {
        comment: "Look",
        ..annotation(1, AnnotationKind::Note { at: [0.5, 0.5] })
    };
    assert!(crate::annotate_pdf_bytes(&input, "", &[note], &[], true).is_err());
}

#[test]
fn notes_images_and_translucent_marks_are_annotations_readers_open() {
    use crate::AnnotationKind;
    let input = pdf_with_pages(&[("", square_page(200))], dictionary! {});
    let output = crate::annotate_pdf_bytes(
        &input,
        "",
        &[
            crate::Annotation {
                comment: "Check this",
                opacity: 0.5,
                ..annotation(1, AnnotationKind::Note { at: [0.95, 0.0] })
            },
            annotation(
                1,
                AnnotationKind::Image {
                    place: [0.5, 0.5, 0.5],
                    bytes: Box::leak(rgba_png().into_boxed_slice()),
                },
            ),
        ],
        &[],
        false,
    )
    .unwrap();
    let annotations = page_annotations(&output, 1);
    let (note, content) = &annotations[0];
    assert_eq!(subtype(note), "Text");
    // Pulled back onto the page.
    assert_box(
        floats(note.get(b"Rect").unwrap()).try_into().ok(),
        [180.0, 180.0, 200.0, 200.0],
    );
    assert_eq!(note.get(b"CA").unwrap().as_float().unwrap(), 0.5);
    assert!(content.contains("/G0 gs"), "{content}");
    let (image, _) = &annotations[1];
    assert_eq!(subtype(image), "Stamp");
    // Twice as wide as tall.
    assert_box(
        floats(image.get(b"Rect").unwrap()).try_into().ok(),
        [100.0, 50.0, 200.0, 100.0],
    );
    assert!(Document::load_mem(&output).unwrap().version.as_str() >= "1.4");
}

#[test]
fn annotate_rejects_what_is_off_the_page() {
    use crate::AnnotationKind;
    let input = numbered_pdf(1);
    let off = annotation(
        1,
        AnnotationKind::Markup {
            style: crate::Markup::Underline,
            boxes: vec![[0.5, 0.5, 1.5, 0.6]],
        },
    );
    assert!(crate::annotate_pdf_bytes(&input, "", &[off], &[], false).is_err());
    let missing_page = annotation(2, AnnotationKind::Note { at: [0.0, 0.0] });
    assert!(crate::annotate_pdf_bytes(&input, "", &[missing_page], &[], false).is_err());
    assert!(crate::annotate_pdf_bytes(&input, "", &[], &[], false).is_err());
}

#[test]
fn objects_added_in_place_never_answer_a_reference_to_a_missing_one() {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, b"0 0 m".to_vec()));
    let page_id = document.new_object_id();
    // Two past the last object: the /Size lopdf writes already covers the
    // one after it, so this is the first id an object added after loading
    // takes.
    let missing = (document.max_id + 2, 0);
    let popup_id = document.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Popup",
        "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        "Parent" => missing,
    });
    let missing = (document.max_id + 1, 0);
    document
        .get_dictionary_mut(popup_id)
        .unwrap()
        .set("Parent", missing);
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content_id,
            "Annots" => vec![popup_id.into()],
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![page_id.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    // Two past the last object: the /Size lopdf writes already covers the
    // one after it, so this is the first id an object added after loading
    // takes.
    let missing = (document.max_id + 2, 0);
    document
        .get_dictionary_mut(popup_id)
        .unwrap()
        .set("Parent", missing);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    let output = crate::annotate_pdf_bytes(
        &input,
        "",
        &[crate::Annotation {
            comment: "Note",
            ..annotation(1, crate::AnnotationKind::Note { at: [0.0, 0.0] })
        }],
        &[],
        false,
    )
    .unwrap();
    let document = Document::load_mem(&output).unwrap();
    let parents = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dictionary| {
            dictionary.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Popup")
        })
        .filter_map(|popup| popup.get(b"Parent").and_then(Object::as_reference).ok())
        .collect::<Vec<_>>();
    assert_eq!(parents.len(), 1);
    assert!(
        document.get_object(parents[0]).is_err(),
        "the popup's missing parent became {:?}",
        document.get_object(parents[0])
    );
}

#[test]
fn annotate_deletes_existing_annotations_with_their_popups_and_fields() {
    let (input, _) = annotated_pdf(vec![square([10, 10, 60, 40])], None, dictionary! {});
    let document = Document::load_mem(&input).unwrap();
    let page_id = document.get_pages()[&1];
    let existing = document
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .and_then(Object::as_array)
        .unwrap()[0]
        .as_reference()
        .unwrap();
    let output = crate::annotate_pdf_bytes(&input, "", &[], &[existing], false).unwrap();
    assert!(page_annotations(&output, 1).is_empty());
    assert!(page_contents(&output)[0].contains("0 0 m"));
    // An id that is not an annotation on any page.
    assert!(crate::annotate_pdf_bytes(&input, "", &[], &[(9_999, 0)], false).is_err());
}

#[test]
fn merging_copies_of_one_document_stores_their_shared_objects_once() {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let program = document.add_object(Stream::new(dictionary! {}, vec![7; 4_000]));
    let descriptor = document.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "Example",
        "FontFile2" => program,
    });
    let font = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "Example",
        "FontDescriptor" => descriptor,
    });
    let content = document.add_object(Stream::new(dictionary! {}, b"BT /F1 9 Tf ET".to_vec()));
    let page = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content,
    });
    // Direct resources on the tree node, which every page inherits.
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![page.into()],
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut input = Vec::new();
    document.save_to(&mut input).unwrap();

    let output = merge_pdf_bytes(&[&input, &input, &input]).unwrap();
    let merged = Document::load_mem(&output).unwrap();
    let count = |kind: &[u8]| {
        merged
            .objects
            .values()
            .filter(|object| {
                object
                    .as_dict()
                    .ok()
                    .and_then(|dictionary| dictionary.get(b"Type").ok())
                    .and_then(|value| value.as_name().ok())
                    == Some(kind)
            })
            .count()
    };
    assert_eq!(count(b"Font"), 1);
    assert_eq!(count(b"FontDescriptor"), 1);
    let programs = merged
        .objects
        .values()
        .filter(|object| {
            object.as_stream().is_ok_and(|stream| {
                stream
                    .decompressed_content()
                    .unwrap_or(stream.content.clone())
                    .len()
                    == 4_000
            })
        })
        .count();
    assert_eq!(programs, 1);
    // Pages are never shared, however alike, and each still finds its font.
    let pages = merged.get_pages();
    assert_eq!(pages.len(), 3);
    for page in pages.values() {
        let resources = merged
            .get_dictionary(*page)
            .unwrap()
            .get(b"Resources")
            .and_then(Object::as_reference)
            .and_then(|id| merged.get_dictionary(id))
            .unwrap();
        assert!(resources.get(b"Font").is_ok());
    }
}

#[test]
fn inherited_resources_are_shared_by_reference_not_copied_onto_each_page() {
    let input = pdf_with_pages(
        &[("0 0 m", dictionary! {}), ("1 1 m", dictionary! {})],
        dictionary! {
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => dictionary! { "ProcSet" => vec![Object::Name(b"PDF".to_vec())] },
        },
    );
    let output = merge_pdf_bytes(&[&input, &one_page_pdf("x")]).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let references = document
        .get_pages()
        .values()
        .take(2)
        .map(|&page| {
            document
                .get_dictionary(page)
                .unwrap()
                .get(b"Resources")
                .unwrap()
                .as_reference()
                .expect("the resources were copied by value")
        })
        .collect::<Vec<_>>();
    assert_eq!(references[0], references[1]);
}

// Form filling. Fields sit on a 200 point page; widgets are 100 by 20 unless
// a test says otherwise.

use crate::{FieldFill, FieldValue, fill_form_bytes};

fn widget(name: Option<&str>, kind: Option<&str>, rect: [i64; 4]) -> Dictionary {
    let mut widget = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "Rect" => rect.iter().map(|&value| Object::Integer(value)).collect::<Vec<_>>(),
        "F" => 4,
    };
    if let Some(name) = name {
        widget.set("T", Object::string_literal(name));
    }
    if let Some(kind) = kind {
        widget.set("FT", kind);
    }
    widget
}

/// A one page PDF whose form holds what `build` adds. `build` gets the page's
/// id and returns the top-level fields and the widgets to put on the page.
fn form_pdf(
    form: Dictionary,
    build: impl FnOnce(&mut Document, ObjectId) -> (Vec<ObjectId>, Vec<ObjectId>),
) -> Vec<u8> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let (fields, widgets) = build(&mut document, page_id);
    let content = document.add_object(Stream::new(dictionary! {}, b"0 0 m".to_vec()));
    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Annots" => widgets.into_iter().map(Object::Reference).collect::<Vec<_>>(),
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
        ),
    );
    let mut form = form;
    form.set(
        "Fields",
        fields
            .into_iter()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    form.set(
        "DR",
        dictionary! { "Font" => dictionary! {
            "Helv" => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
            "TiRo" => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Times-Roman" },
        } },
    );
    if !form.has(b"DA") {
        form.set("DA", Object::string_literal("/Helv 0 Tf 0 g"));
    }
    let catalog = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => form,
    });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

/// A form with one field per widget, each a field and widget in one.
fn simple_form(form: Dictionary, widgets: Vec<Dictionary>) -> (Vec<u8>, Vec<ObjectId>) {
    let mut ids = Vec::new();
    let bytes = form_pdf(form, |document, page| {
        for mut widget in widgets {
            widget.set("P", page);
            ids.push(document.add_object(widget));
        }
        (ids.clone(), ids.clone())
    });
    (bytes, ids)
}

fn text_fill(widget: ObjectId, text: &str) -> FieldFill<'_> {
    FieldFill {
        widget,
        value: FieldValue::Text(text),
    }
}

/// The page's widgets in order, read back from the output.
fn filled_widgets(bytes: &[u8]) -> (Document, Vec<Dictionary>) {
    let document = Document::load_mem(bytes).unwrap();
    let page = document.get_pages()[&1];
    let widgets = document
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .map(|annotations| {
            annotations
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    document
                        .get_dictionary(entry.as_reference().unwrap())
                        .unwrap()
                        .clone()
                })
                .collect()
        })
        .unwrap_or_default();
    (document, widgets)
}

/// The normal appearance a widget shows, or the named state's.
fn shown(document: &Document, widget: &Dictionary, state: Option<&[u8]>) -> (Stream, String) {
    let normal = widget
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap();
    let id = match state {
        Some(state) => {
            let states = match normal {
                Object::Reference(id) => document.get_dictionary(*id).unwrap(),
                states => states.as_dict().unwrap(),
            };
            states.get(state).unwrap().as_reference().unwrap()
        }
        None => normal.as_reference().unwrap(),
    };
    let stream = document
        .get_object(id)
        .unwrap()
        .as_stream()
        .unwrap()
        .clone();
    let content = String::from_utf8_lossy(
        &stream
            .decompressed_content()
            .unwrap_or(stream.content.clone()),
    )
    .into_owned();
    (stream, content)
}

fn hex_of(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02X}")).collect()
}

fn text_value(widget: &Dictionary) -> String {
    lopdf::decode_text_string(widget.get(b"V").unwrap()).unwrap()
}

#[test]
fn filling_a_text_field_sets_its_value_and_draws_it() {
    let mut name = widget(Some("name"), Some("Tx"), [20, 150, 120, 170]);
    name.set("DA", Object::string_literal("/TiRo 10 Tf 0 0 1 rg"));
    name.set("RV", Object::string_literal("<body>old</body>"));
    name.set(
        "MK",
        dictionary! { "BG" => vec![1.into()], "BC" => vec![0.into()] },
    );
    let (input, ids) = simple_form(dictionary! {}, vec![name]);
    let output = fill_form_bytes(&input, "", &[text_fill(ids[0], "Ada Lovelace")], false).unwrap();
    assert_eq!(output.kept, 0);
    let (document, widgets) = filled_widgets(&output.bytes);
    let field = &widgets[0];
    assert_eq!(text_value(field), "Ada Lovelace");
    assert!(
        !field.has(b"RV"),
        "a stale rich text value would be shown instead"
    );
    let (stream, content) = shown(&document, field, None);
    assert!(content.contains(&hex_of("Ada Lovelace")), "{content}");
    assert!(content.contains("/F0 10 Tf"), "{content}");
    assert!(content.contains("0 0 1 rg"), "{content}");
    assert!(
        content.contains("/Tx BMC") && content.contains("EMC"),
        "{content}"
    );
    // White background, black border, both from /MK.
    assert!(
        content.contains("1 g") && content.contains("0 G"),
        "{content}"
    );
    let font = stream
        .dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Font")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"F0")
        .unwrap()
        .as_reference()
        .unwrap();
    let font = document.get_dictionary(font).unwrap();
    assert_eq!(
        font.get(b"BaseFont").unwrap().as_name().unwrap(),
        b"Times-Roman"
    );
}

#[test]
fn filling_fits_automatic_text_to_the_field_and_honours_alignment() {
    let mut right = widget(Some("right"), Some("Tx"), [20, 150, 120, 170]);
    right.set("Q", 2);
    let (input, ids) = simple_form(dictionary! {}, vec![right]);
    let long = "a much longer text than fits at the usual size";
    let output = fill_form_bytes(&input, "", &[text_fill(ids[0], long)], false).unwrap();
    let (document, widgets) = filled_widgets(&output.bytes);
    let (_, content) = shown(&document, &widgets[0], None);
    let size = content
        .lines()
        .find(|line| line.ends_with(" Tf"))
        .and_then(|line| line.split(' ').nth(1))
        .unwrap()
        .parse::<f32>()
        .unwrap();
    let width = crate::stamps::text_width(FontFamily::Helvetica, false, long.as_bytes()) as f32
        / 1000.0
        * size;
    assert!(width <= 100.0 - 4.0 + 0.01, "{size}: {width}");
    let x = content
        .lines()
        .find(|line| line.ends_with(" Tm"))
        .and_then(|line| line.split(' ').nth(4))
        .unwrap()
        .parse::<f32>()
        .unwrap();
    assert!(
        (x + width - 98.0).abs() < 0.01,
        "right aligned, {x} + {width}"
    );
}

#[test]
fn filling_wraps_multiline_fields_and_spreads_combs() {
    let mut notes = widget(Some("notes"), Some("Tx"), [20, 100, 120, 170]);
    notes.set("Ff", 1 << 12);
    notes.set("DA", Object::string_literal("/Helv 10 Tf 0 g"));
    let mut code = widget(Some("code"), Some("Tx"), [20, 50, 120, 70]);
    code.set("Ff", 1 << 24);
    code.set("MaxLen", 5);
    let (input, ids) = simple_form(dictionary! {}, vec![notes, code]);
    let output = fill_form_bytes(
        &input,
        "",
        &[
            text_fill(ids[0], "first line\nand a second one that wraps around"),
            text_fill(ids[1], "AB12"),
        ],
        false,
    )
    .unwrap();
    let (document, widgets) = filled_widgets(&output.bytes);
    let (_, notes) = shown(&document, &widgets[0], None);
    assert!(notes.matches(" Tm").count() >= 3, "{notes}");
    let (_, code) = shown(&document, &widgets[1], None);
    // One character a cell, each cell 20 points wide.
    let xs = code
        .lines()
        .filter(|line| line.ends_with(" Tm"))
        .map(|line| line.split(' ').nth(4).unwrap().parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(xs.len(), 4, "{code}");
    assert!(
        xs.windows(2)
            .all(|pair| (pair[1] - pair[0] - 20.0).abs() < 4.0),
        "{xs:?}"
    );
}

#[test]
fn filling_turns_a_check_box_on_and_off() {
    let (input, ids) = {
        let mut ids = Vec::new();
        let bytes = form_pdf(dictionary! {}, |document, page| {
            let on = document.add_object(Stream::new(dictionary! {}, b"% on".to_vec()));
            let off = document.add_object(Stream::new(dictionary! {}, b"% off".to_vec()));
            let mut check = widget(Some("agree"), Some("Btn"), [20, 20, 40, 40]);
            check.set("P", page);
            check.set(
                "AP",
                dictionary! { "N" => dictionary! { "Yes" => on, "Off" => off } },
            );
            check.set("AS", "Off");
            check.set("V", "Off");
            ids.push(document.add_object(check));
            (ids.clone(), ids.clone())
        });
        (bytes, ids)
    };
    let on = FieldFill {
        widget: ids[0],
        value: FieldValue::Button(true),
    };
    let output = fill_form_bytes(&input, "", &[on], false).unwrap();
    let (_, widgets) = filled_widgets(&output.bytes);
    assert_eq!(widgets[0].get(b"AS").unwrap().as_name().unwrap(), b"Yes");
    assert_eq!(widgets[0].get(b"V").unwrap().as_name().unwrap(), b"Yes");

    let off = FieldFill {
        widget: ids[0],
        value: FieldValue::Button(false),
    };
    let output = fill_form_bytes(&output.bytes, "", &[off], false).unwrap();
    let (_, widgets) = filled_widgets(&output.bytes);
    assert_eq!(widgets[0].get(b"AS").unwrap().as_name().unwrap(), b"Off");
    assert_eq!(widgets[0].get(b"V").unwrap().as_name().unwrap(), b"Off");
}

#[test]
fn filling_a_radio_group_turns_on_one_kid() {
    let mut kids = Vec::new();
    let input = form_pdf(dictionary! {}, |document, page| {
        let parent = document.new_object_id();
        for (index, state) in ["Red", "Blue"].iter().enumerate() {
            let on = document.add_object(Stream::new(dictionary! {}, b"% on".to_vec()));
            let off = document.add_object(Stream::new(dictionary! {}, b"% off".to_vec()));
            let x = 20 + 30 * index as i64;
            let mut kid = widget(None, None, [x, 20, x + 20, 40]);
            kid.set("P", page);
            kid.set("Parent", parent);
            kid.set(
                "AP",
                dictionary! { "N" => dictionary! { *state => on, "Off" => off } },
            );
            kid.set("AS", if index == 0 { *state } else { "Off" });
            kids.push(document.add_object(kid));
        }
        document.objects.insert(
            parent,
            Object::Dictionary(dictionary! {
                "FT" => "Btn",
                "Ff" => 1 << 15,
                "T" => Object::string_literal("colour"),
                "V" => "Red",
                "Kids" => kids.iter().map(|&kid| Object::Reference(kid)).collect::<Vec<_>>(),
            }),
        );
        (vec![parent], kids.clone())
    });
    let blue = FieldFill {
        widget: kids[1],
        value: FieldValue::Button(true),
    };
    let output = fill_form_bytes(&input, "", &[blue], false).unwrap();
    let (document, widgets) = filled_widgets(&output.bytes);
    let states = widgets
        .iter()
        .map(|kid| kid.get(b"AS").unwrap().as_name().unwrap().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(states, [b"Off".to_vec(), b"Blue".to_vec()]);
    let parent = document
        .get_dictionary(widgets[0].get(b"Parent").unwrap().as_reference().unwrap())
        .unwrap();
    assert_eq!(parent.get(b"V").unwrap().as_name().unwrap(), b"Blue");
}

#[test]
fn filling_a_list_sets_its_value_and_indices_and_draws_the_choice() {
    let mut list = widget(Some("size"), Some("Ch"), [20, 100, 120, 170]);
    list.set(
        "Opt",
        vec![
            Object::Array(vec![
                Object::string_literal("s"),
                Object::string_literal("Small"),
            ]),
            Object::Array(vec![
                Object::string_literal("l"),
                Object::string_literal("Large"),
            ]),
        ],
    );
    list.set("I", vec![0.into()]);
    let (input, ids) = simple_form(dictionary! {}, vec![list]);
    let choose = |choices: Vec<&'static str>| FieldFill {
        widget: ids[0],
        value: FieldValue::Choices(choices),
    };
    let output = fill_form_bytes(&input, "", &[choose(vec!["l"])], false).unwrap();
    let (document, widgets) = filled_widgets(&output.bytes);
    assert_eq!(text_value(&widgets[0]), "l");
    assert_eq!(
        widgets[0].get(b"I").unwrap().as_array().unwrap(),
        &vec![Object::Integer(1)]
    );
    let (_, content) = shown(&document, &widgets[0], None);
    assert!(content.contains(&hex_of("Large")) && content.contains(&hex_of("Small")));
    assert!(content.contains("0.6 0.75 0.85 rg"), "{content}");

    let error = fill_form_bytes(&input, "", &[choose(vec!["m"])], false)
        .err()
        .unwrap();
    assert!(error.contains("no option “m”"), "{error}");
    let error = fill_form_bytes(&input, "", &[choose(vec!["s", "l"])], false)
        .err()
        .unwrap();
    assert!(error.contains("Choose one option"), "{error}");
}

#[test]
fn filling_refuses_what_it_cannot_save_faithfully() {
    let mut locked = widget(Some("locked"), Some("Tx"), [20, 20, 120, 40]);
    locked.set("Ff", 1);
    let mut secret = widget(Some("secret"), Some("Tx"), [20, 50, 120, 70]);
    secret.set("Ff", 1 << 13);
    let mut short = widget(Some("short"), Some("Tx"), [20, 80, 120, 100]);
    short.set("MaxLen", 3);
    short.set("TU", Object::string_literal("Initials"));
    let plain = widget(Some("plain"), Some("Tx"), [20, 110, 120, 130]);
    let mut note = square([10, 10, 60, 40]);
    note.remove(b"AP");
    let (input, ids) = simple_form(dictionary! {}, vec![locked, secret, short, plain, note]);
    let error = |fill: FieldFill<'_>| fill_form_bytes(&input, "", &[fill], false).err().unwrap();
    assert!(error(text_fill(ids[0], "x")).contains("read-only"));
    assert!(error(text_fill(ids[1], "x")).contains("password"));
    assert!(error(text_fill(ids[2], "ABCD")).contains("“Initials” takes at most 3"));
    let undrawable = error(text_fill(ids[3], "Жанна"));
    assert!(undrawable.contains("“Ж” in “plain”"), "{undrawable}");
    assert!(error(text_fill(ids[4], "x")).contains("no longer in this PDF"));
    assert!(fill_form_bytes(&input, "", &[], false).is_err());
}

#[test]
fn filling_draws_a_turned_widget_upright_in_its_own_box() {
    let mut turned = widget(Some("turned"), Some("Tx"), [20, 20, 40, 120]);
    turned.set("MK", dictionary! { "R" => 90 });
    let (input, ids) = simple_form(dictionary! {}, vec![turned]);
    let output = fill_form_bytes(&input, "", &[text_fill(ids[0], "up")], false).unwrap();
    let (document, widgets) = filled_widgets(&output.bytes);
    let (stream, _) = shown(&document, &widgets[0], None);
    let reals = |key: &[u8]| {
        stream
            .dict
            .get(key)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_float().unwrap())
            .collect::<Vec<_>>()
    };
    // 20 wide and 100 tall on the page, so 100 by 20 as the text reads.
    assert_eq!(reals(b"BBox"), [0.0, 0.0, 100.0, 20.0]);
    assert_eq!(reals(b"Matrix"), [0.0, 1.0, -1.0, 0.0, 20.0, 0.0]);
}

#[test]
fn filling_draws_a_check_box_that_has_no_look_of_its_own() {
    let mut bare = widget(Some("bare"), Some("Btn"), [20, 20, 40, 40]);
    bare.set("MK", dictionary! { "BC" => vec![0.into()] });
    let (input, ids) = simple_form(dictionary! {}, vec![bare]);
    let on = FieldFill {
        widget: ids[0],
        value: FieldValue::Button(true),
    };
    let output = fill_form_bytes(&input, "", &[on], false).unwrap();
    let (document, widgets) = filled_widgets(&output.bytes);
    assert_eq!(widgets[0].get(b"AS").unwrap().as_name().unwrap(), b"Yes");
    let (stream, content) = shown(&document, &widgets[0], Some(b"Yes"));
    assert!(content.contains("<34> Tj"), "a check: {content}");
    let font = stream
        .dict
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Font")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"F0")
        .unwrap()
        .as_reference()
        .unwrap();
    assert_eq!(
        document
            .get_dictionary(font)
            .unwrap()
            .get(b"BaseFont")
            .unwrap()
            .as_name()
            .unwrap(),
        b"ZapfDingbats"
    );
    let (_, off) = shown(&document, &widgets[0], Some(b"Off"));
    assert!(!off.contains("Tj"), "{off}");
}

#[test]
fn filling_and_flattening_draws_every_value_even_in_a_form_that_asks_for_redrawing() {
    let mut untouched = widget(Some("untouched"), Some("Tx"), [20, 20, 120, 40]);
    untouched.set("V", Object::string_literal("already there"));
    let filled = widget(Some("filled"), Some("Tx"), [20, 50, 120, 70]);
    let (input, ids) = simple_form(
        dictionary! {
            "NeedAppearances" => true,
            "XFA" => Object::string_literal("<xdp/>"),
        },
        vec![untouched, filled],
    );
    let output = fill_form_bytes(&input, "", &[text_fill(ids[1], "new")], true).unwrap();
    assert_eq!(output.kept, 0);
    let (subtypes, drawing, document) = flattened_page(&output.bytes);
    assert!(subtypes.is_empty(), "{subtypes:?}");
    assert_eq!(drawing.matches(" Do").count(), 2, "{drawing}");
    assert!(!document.catalog().unwrap().has(b"AcroForm"));
    let text = document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .map(|stream| {
            String::from_utf8_lossy(
                &stream
                    .decompressed_content()
                    .unwrap_or(stream.content.clone()),
            )
            .into_owned()
        })
        .collect::<String>();
    assert!(text.contains(&hex_of("already there")) && text.contains(&hex_of("new")));
}

#[test]
fn filling_without_flattening_keeps_the_form_but_drops_xfa() {
    let (input, ids) = simple_form(
        dictionary! { "NeedAppearances" => true, "XFA" => Object::string_literal("<xdp/>") },
        vec![widget(Some("name"), Some("Tx"), [20, 20, 120, 40])],
    );
    let output = fill_form_bytes(&input, "", &[text_fill(ids[0], "x")], false).unwrap();
    let document = Document::load_mem(&output.bytes).unwrap();
    let form = document
        .catalog()
        .unwrap()
        .get(b"AcroForm")
        .unwrap()
        .as_dict()
        .unwrap();
    assert!(!form.has(b"XFA"));
    assert!(form.get(b"NeedAppearances").unwrap().as_bool().unwrap());
}

fn edit(
    input: &[u8],
    replace: &[crate::TextRemoval],
    erase: &[(u32, [f32; 4], [f32; 3])],
    additions: &[crate::Annotation<'_>],
) -> crate::Edited {
    let erase = erase
        .iter()
        .map(|&(page, area, fill)| crate::Erasure { page, area, fill })
        .collect::<Vec<_>>();
    crate::edit_pdf_bytes(
        input,
        "",
        crate::EditOptions {
            images: &[],
            replace,
            erase: &erase,
            additions,
        },
    )
    .unwrap()
}

/// A blue band behind "Hello World", from (90, 680) to (290, 720).
const BANDED: &[u8] =
    b"0 0 1 rg 90 680 200 40 re f 0 g BT /F1 20 Tf 100 700 Td (Hello World) Tj ET";

fn world(cover: [f32; 3]) -> crate::TextRemoval {
    crate::TextRemoval {
        page: 1,
        area: WORLD,
        shown: WORLD,
        cover,
    }
}

#[test]
fn editing_replaces_text_and_leaves_what_is_behind_it() {
    let input = redaction_pdf(BANDED, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let replacement = annotation(
        1,
        crate::AnnotationKind::Text {
            area: [WORLD[0], 0.09, 0.5, 0.14],
            text: "Earth",
            family: FontFamily::Helvetica,
            bold: false,
            size: 20.0,
            fill: None,
        },
    );
    let edited = edit(&input, &[world([0.0, 0.0, 1.0])], &[], &[replacement]);
    assert!(edited.covered.is_empty());
    let content = first_page_content(&edited.bytes);
    assert!(content.contains("[<48656C6C6F20> -2611] TJ"), "{content}");
    // The band is neither cut nor painted over.
    assert!(content.contains("90 680 200 40 re"), "{content}");
    assert!(!content.contains("0 0 1 rg\n151"), "{content}");
    let everything = every_stream(&edited.bytes);
    assert!(!everything.contains("World"), "{everything}");
    assert!(everything.contains("<4561727468>"), "{everything}");
}

#[test]
fn erasing_takes_everything_out_and_paints_it_over() {
    let input = redaction_pdf(BANDED, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let edited = edit(&input, &[], &[(1, WORLD, [0.0, 0.0, 1.0])], &[]);
    assert!(edited.covered.is_empty());
    let content = first_page_content(&edited.bytes);
    assert!(!every_stream(&edited.bytes).contains("World"));
    assert!(!content.contains("90 680 200 40 re"), "the band is cut");
    assert!(
        content.contains("0 0 1 rg\n151 690 53 30 re\nf"),
        "{content}"
    );
}

#[test]
fn erasing_paints_each_area_in_its_own_colour_in_order() {
    let input = redaction_pdf(BANDED, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    let left = [0.1, 0.05, 0.2, 0.1];
    let edited = edit(
        &input,
        &[],
        &[
            (1, WORLD, [1.0, 1.0, 1.0]),
            (1, left, [1.0, 0.0, 0.0]),
            (1, WORLD, [0.0, 1.0, 0.0]),
        ],
        &[],
    );
    let content = first_page_content(&edited.bytes);
    let red = content.find("1 0 0 rg\n60 720 60 40 re").expect(&content);
    let green = content.find("0 1 0 rg\n151 690 53 30 re").expect(&content);
    assert!(red < green, "{content}");
    // A later area takes out an earlier one's paint like anything else.
    assert!(!content.contains("1 1 1 rg\n151 690"), "{content}");
}

#[test]
fn editing_covers_text_it_cannot_measure_and_says_so() {
    let input = redaction_pdf(HELLO, |document, _| {
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "Mystery",
        });
        (
            dictionary! { "Font" => dictionary! { "F1" => font } },
            dictionary! {},
            dictionary! {},
        )
    });
    let edited = edit(&input, &[world([1.0, 0.0, 0.0])], &[], &[]);
    assert_eq!(edited.covered, [(1, crate::Unremovable::Text)]);
    assert!(every_stream(&edited.bytes).contains("1 0 0 rg"));
}

#[test]
fn editing_refuses_to_do_nothing_and_areas_off_the_page() {
    let input = redaction_pdf(HELLO, |document, _| {
        (
            helvetica_resources(document),
            dictionary! {},
            dictionary! {},
        )
    });
    fn options(erase: &[crate::Erasure]) -> crate::EditOptions<'_> {
        crate::EditOptions {
            images: &[],
            replace: &[],
            erase,
            additions: &[],
        }
    }
    assert!(crate::edit_pdf_bytes(&input, "", options(&[])).is_err());
    let off = [crate::Erasure {
        page: 1,
        area: [0.5, 0.5, 1.5, 0.6],
        fill: [1.0; 3],
    }];
    assert!(crate::edit_pdf_bytes(&input, "", options(&off)).is_err());
    let missing = [crate::Erasure {
        page: 2,
        area: [0.1, 0.1, 0.2, 0.2],
        fill: [1.0; 3],
    }];
    assert!(crate::edit_pdf_bytes(&input, "", options(&missing)).is_err());
    let unseen = [crate::Erasure {
        page: 1,
        area: [0.1, 0.1, 0.2, 0.2],
        fill: [2.0, 0.0, 0.0],
    }];
    assert!(crate::edit_pdf_bytes(&input, "", options(&unseen)).is_err());
}

#[test]
fn page_text_reports_how_each_glyph_is_drawn() {
    let content = b"BT 1 0 0 rg /F1 20 Tf 100 700 Td (Hi) Tj ET \
        BT /F2 10 Tf 3 Tr 100 100 Td (Lo) Tj ET \
        BT 0 1 -1 0 300 300 Tm /F1 12 Tf (Up) Tj ET";
    let input = redaction_pdf(content, |document, _| {
        let bold = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "ABCDEF+Arial-BoldMT",
            "FirstChar" => 72,
            "LastChar" => 111,
            "Widths" => vec![600.into(); 40],
        });
        let descriptor = document.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "Garamondish",
            "Flags" => 2 + 64,
        });
        let serif = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "Garamondish",
            "FirstChar" => 72,
            "LastChar" => 111,
            "Widths" => vec![500.into(); 40],
            "FontDescriptor" => descriptor,
        });
        (
            dictionary! { "Font" => dictionary! { "F1" => bold, "F2" => serif } },
            dictionary! {},
            dictionary! {},
        )
    });
    let pages = crate::page_texts(&input, "").unwrap();
    let styles = &pages[0].styles;
    assert_eq!(pages[0].text.concat(), "HiLoUp");
    let hi = styles[0];
    assert!((hi.size - 20.0).abs() < 1e-3);
    assert!((hi.baseline - 0.125).abs() < 1e-4);
    assert_eq!(hi.color, Some([1.0, 0.0, 0.0]));
    assert_eq!(
        (hi.family, hi.bold, hi.italic, hi.invisible, hi.upright),
        (FontFamily::Helvetica, true, false, false, true)
    );
    let lo = styles[2];
    assert_eq!(
        lo.color,
        Some([1.0, 0.0, 0.0]),
        "colour belongs to the graphics state, not the text object"
    );
    assert_eq!(
        (lo.family, lo.bold, lo.italic, lo.invisible),
        (FontFamily::Times, false, true, true)
    );
    assert!(!styles[4].upright, "turned text");
}

#[test]
fn page_text_knows_the_clones_of_the_standard_fonts_by_name() {
    let names = [
        "TACTGM+NimbusRomNo9L-Medi",
        "KUYGUP+NimbusRomNo9L-ReguItal",
        "ABCMRX+CMR10",
        "QWERTY+CMBX12",
        "TeXGyreTermes-Italic",
        "LMMono10-Regular",
        "NimbusMonL-Regu",
        "MinionPro-It",
        "CMTT10",
    ];
    let content = (0..names.len())
        .map(|index| format!("BT /F{index} 10 Tf 100 {} Td (Hi) Tj ET ", 100 + 50 * index))
        .collect::<String>();
    let input = redaction_pdf(content.as_bytes(), |document, _| {
        let mut fonts = Dictionary::new();
        for (index, name) in names.iter().enumerate() {
            let font = document.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => Object::Name(name.as_bytes().to_vec()),
                "FirstChar" => 72,
                "LastChar" => 105,
                "Widths" => vec![500.into(); 34],
            });
            fonts.set(format!("F{index}"), font);
        }
        (
            dictionary! { "Font" => fonts },
            dictionary! {},
            dictionary! {},
        )
    });
    let pages = crate::page_texts(&input, "").unwrap();
    let looks = pages[0]
        .styles
        .iter()
        .step_by(2)
        .map(|style| (style.family, style.bold, style.italic))
        .collect::<Vec<_>>();
    use FontFamily::{Courier, Times};
    assert_eq!(
        looks,
        [
            (Times, true, false),
            (Times, false, true),
            (Times, false, false),
            (Times, true, false),
            (Times, false, true),
            (Courier, false, false),
            (Courier, false, false),
            (Times, false, true),
            (Courier, false, false),
        ]
    );
}

#[test]
fn page_text_reads_a_tint_of_black_as_grey() {
    let content = b"BT /K cs 0.25 sc /F1 20 Tf 100 700 Td (Hi) Tj ET";
    let input = redaction_pdf(content, |document, _| {
        let mut resources = helvetica_resources(document);
        let tint = document.add_object(dictionary! {
            "FunctionType" => 2,
            "Domain" => vec![0.into(), 1.into()],
            "C0" => vec![1.into()],
            "C1" => vec![0.into()],
            "N" => 1,
        });
        resources.set(
            "ColorSpace",
            dictionary! {
                "K" => vec![
                    Object::Name(b"Separation".to_vec()),
                    Object::Name(b"Black".to_vec()),
                    Object::Name(b"DeviceGray".to_vec()),
                    tint.into(),
                ],
            },
        );
        (resources, dictionary! {}, dictionary! {})
    });
    let pages = crate::page_texts(&input, "").unwrap();
    assert_eq!(pages[0].styles[0].color, Some([0.75; 3]));
}

#[test]
fn editing_a_line_leaves_lines_set_closer_than_their_font_is_tall() {
    // 20 point text on lines 17 points apart, in a font whose glyphs stand
    // 1.1 em above the baseline and 0.35 em below it.
    let content = b"BT /F1 20 Tf 17 TL 100 717 Td (Above) Tj T* (Middle) Tj T* (Below) Tj ET";
    let input = redaction_pdf(content, |document, _| {
        let descriptor = document.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "Tall",
            "Flags" => 32,
            "Ascent" => 1100,
            "Descent" => -350,
        });
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Tall",
            "FirstChar" => 65,
            "LastChar" => 122,
            "Widths" => vec![500.into(); 58],
            "FontDescriptor" => descriptor,
        });
        (
            dictionary! { "Font" => dictionary! { "F1" => font } },
            dictionary! {},
            dictionary! {},
        )
    });
    // The band the app sends: from the middle line's baseline at 700 up
    // 0.55 em.
    let band = [
        100.0 / 600.0,
        1.0 - (700.0 + 11.0) / 800.0,
        400.0 / 600.0,
        1.0 - 700.0 / 800.0,
    ];
    let removal = crate::TextRemoval {
        page: 1,
        area: band,
        shown: band,
        cover: [1.0; 3],
    };
    let edited = edit(&input, &[removal], &[], &[]);
    let everything = every_stream(&edited.bytes);
    assert!(!everything.contains("Middle"), "{everything}");
    assert!(everything.contains("Above"), "{everything}");
    assert!(everything.contains("Below"), "{everything}");
}

/// A page drawing one image four ways: filling the page, plainly, inside a
/// form, and inline.
fn pictured_pdf() -> Vec<u8> {
    let content = b"q 600 0 0 800 0 0 cm /Im1 Do Q \
        q 100 0 0 100 50 600 cm /Im1 Do Q \
        q 1 0 0 1 300 100 cm /Fm1 Do Q \
        q 40 0 0 40 400 400 cm BI /W 2 /H 2 /CS /G /BPC 8 ID \x10\x20\x30\x40 EI Q";
    redaction_pdf(content, |document, _| {
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![200; 4],
        ));
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image } },
            },
            b"q 60 0 0 30 0 0 cm /Im1 Do Q".to_vec(),
        ));
        (
            dictionary! { "XObject" => dictionary! { "Im1" => image, "Fm1" => form } },
            dictionary! {},
            dictionary! {},
        )
    })
}

fn listed_images(bytes: &[u8]) -> Vec<[f32; 4]> {
    crate::page_texts(bytes, "").unwrap().remove(0).images
}

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-4)
}

#[test]
fn page_text_lists_where_images_are_drawn_but_not_backgrounds() {
    let images = listed_images(&pictured_pdf());
    let expected = [
        [50.0 / 600.0, 0.125, 0.25, 0.25],
        [0.5, 1.0 - 130.0 / 800.0, 0.6, 0.875],
        [400.0 / 600.0, 0.45, 440.0 / 600.0, 0.5],
    ];
    assert_eq!(images.len(), 3, "{images:?}");
    for (found, expected) in images.iter().zip(expected) {
        assert!(near(*found, expected), "{found:?} {expected:?}");
    }
}

#[test]
fn editing_moves_resizes_and_takes_away_images_the_page_draws() {
    let input = pictured_pdf();
    let listed = listed_images(&input);
    let moves = [
        crate::ImageMove {
            page: 1,
            from: listed[0],
            to: Some([0.5, 0.5, 0.6, 0.6]),
        },
        crate::ImageMove {
            page: 1,
            from: listed[1],
            to: None,
        },
        crate::ImageMove {
            page: 1,
            from: listed[2],
            to: Some([0.1, 0.8, 0.2, 0.85]),
        },
    ];
    let edited = crate::edit_pdf_bytes(
        &input,
        "",
        crate::EditOptions {
            images: &moves,
            replace: &[],
            erase: &[],
            additions: &[],
        },
    )
    .unwrap();
    let mut images = listed_images(&edited.bytes);
    images.sort_by(|a, b| a[0].total_cmp(&b[0]));
    assert_eq!(images.len(), 2, "{images:?}");
    assert!(near(images[0], [0.1, 0.8, 0.2, 0.85]), "{images:?}");
    assert!(near(images[1], [0.5, 0.5, 0.6, 0.6]), "{images:?}");
    // The same image filling the page is drawn as it was.
    assert!(first_page_content(&edited.bytes).contains("600 0 0 800 0 0 cm"));
    assert!(every_stream(&edited.bytes).contains("ID \x10\x20\x30\x40"));
}

#[test]
fn editing_refuses_to_move_an_image_that_is_not_there() {
    let input = pictured_pdf();
    let moves = [crate::ImageMove {
        page: 1,
        from: [0.3, 0.3, 0.4, 0.4],
        to: None,
    }];
    let options = crate::EditOptions {
        images: &moves,
        replace: &[],
        erase: &[],
        additions: &[],
    };
    assert!(crate::edit_pdf_bytes(&input, "", options).is_err());
}

fn ocr_word(text: &str, left: f32, width: f32, space: bool) -> OcrWord<'_> {
    OcrWord {
        text,
        left,
        width,
        baseline: 0.5,
        size: 0.05,
        angle: 0.0,
        space,
    }
}

fn ocr_page(words: Vec<OcrWord<'_>>) -> Vec<OcrPage<'_>> {
    vec![OcrPage { page: 1, words }]
}

fn tall_page(entries: Dictionary) -> Vec<u8> {
    let mut page = dictionary! { "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()] };
    for (key, value) in entries.iter() {
        page.set(key.clone(), value.clone());
    }
    pdf_with_pages(&[("0 0 m", page)], dictionary! {})
}

#[test]
fn ocr_text_reads_back_where_it_was_found() {
    let input = tall_page(dictionary! {});
    let words = vec![
        ocr_word("Hello", 0.1, 0.3, true),
        ocr_word("world", 0.45, 0.25, false),
    ];
    let output = add_text_layer_bytes(&input, "", &ocr_page(words)).unwrap();
    let page = glyph_texts(&output, "").unwrap().remove(0);
    assert_eq!(page.text.concat(), "Hello world");
    assert!(page.styles.iter().all(|style| style.invisible));
    let [left, top, _, bottom] = page.boxes[0];
    assert!((left - 0.1).abs() < 1e-3, "{left}");
    // The word fills its width: its last letter ends where the word did.
    assert!((page.boxes[4][2] - 0.4).abs() < 1e-3, "{:?}", page.boxes[4]);
    assert!(
        (page.boxes[6][0] - 0.45).abs() < 1e-3,
        "{:?}",
        page.boxes[6]
    );
    assert!(
        (page.boxes[10][2] - 0.7).abs() < 1e-3,
        "{:?}",
        page.boxes[10]
    );
    // Ascenders to descenders, around the baseline.
    assert!((page.styles[0].baseline - 0.5).abs() < 1e-3);
    assert!((page.styles[0].size - 20.0).abs() < 0.01);
    assert!(
        (top - 0.46).abs() < 1e-3 && (bottom - 0.51).abs() < 1e-3,
        "{top} {bottom}"
    );
    assert!(page_contents(&output)[0].contains("0 0 m"));
}

#[test]
fn ocr_text_lands_where_the_reader_sees_it_on_a_turned_page() {
    let input = tall_page(dictionary! { "Rotate" => 90 });
    let output = add_text_layer_bytes(
        &input,
        "",
        &ocr_page(vec![ocr_word("Turned", 0.2, 0.5, false)]),
    )
    .unwrap();
    let page = glyph_texts(&output, "").unwrap().remove(0);
    assert_eq!(page.text.concat(), "Turned");
    assert!(page.styles.iter().all(|style| style.upright));
    assert!((page.boxes[0][0] - 0.2).abs() < 1e-3, "{:?}", page.boxes[0]);
    assert!((page.boxes[5][2] - 0.7).abs() < 1e-3, "{:?}", page.boxes[5]);
    assert!((page.styles[0].baseline - 0.5).abs() < 1e-3);
}

#[test]
fn ocr_text_keeps_any_script() {
    let input = tall_page(dictionary! {});
    let words = vec![
        ocr_word("Ελληνικά", 0.1, 0.3, true),
        ocr_word("漢字", 0.45, 0.1, true),
        ocr_word("𝄞", 0.6, 0.05, false),
    ];
    let output = add_text_layer_bytes(&input, "", &ocr_page(words)).unwrap();
    let page = glyph_texts(&output, "").unwrap().remove(0);
    assert_eq!(page.text.concat(), "Ελληνικά 漢字 𝄞");
}

#[test]
fn ocr_font_is_embedded_and_never_draws_notdef() {
    let input = tall_page(dictionary! {});
    let words = vec![
        ocr_word("abc", 0.1, 0.3, true),
        ocr_word("cab", 0.5, 0.3, false),
    ];
    let output = add_text_layer_bytes(&input, "", &ocr_page(words)).unwrap();
    let document = Document::load_mem(&output).unwrap();
    let descendant = document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .find(|dictionary| {
            dictionary.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"CIDFontType2")
        })
        .expect("no CID font");
    let map = document
        .get_object(
            descendant
                .get(b"CIDToGIDMap")
                .unwrap()
                .as_reference()
                .unwrap(),
        )
        .unwrap()
        .as_stream()
        .unwrap();
    // a, b, c and the space: four CIDs past .notdef, all on glyph 1.
    assert_eq!(
        map.decompressed_content().unwrap(),
        [0, 0, 0, 1, 0, 1, 0, 1, 0, 1]
    );
    let descriptor = document
        .get_dictionary(
            descendant
                .get(b"FontDescriptor")
                .unwrap()
                .as_reference()
                .unwrap(),
        )
        .unwrap();
    assert!(descriptor.get(b"FontFile2").is_ok());
    // An embedded font is what lets the result become PDF/A.
    convert_to_pdfa_bytes(&output, "", PdfALevel::A2b, &[]).unwrap();
}

#[test]
fn ocr_leaves_other_pages_alone_and_refuses_nothing_to_write() {
    let page = dictionary! { "MediaBox" => vec![0.into(), 0.into(), 200.into(), 400.into()] };
    let input = pdf_with_pages(&[("", page.clone()), ("", page)], dictionary! {});
    let pages = vec![OcrPage {
        page: 2,
        words: vec![ocr_word("Second", 0.1, 0.3, false)],
    }];
    let output = add_text_layer_bytes(&input, "", &pages).unwrap();
    let texts = glyph_texts(&output, "").unwrap();
    assert!(texts[0].text.is_empty());
    assert_eq!(texts[1].text.concat(), "Second");

    let blank = ocr_page(vec![
        ocr_word("  ", 0.1, 0.3, false),
        ocr_word("x", 0.1, 0.0, false),
    ]);
    assert!(add_text_layer_bytes(&input, "", &blank).is_err());
}

/// A pale sheet with lines of "text" inside its margins, seen at an angle on
/// a dark desk.
fn photographed_sheet(width: usize, height: usize, corners: [(f32, f32); 4]) -> Vec<u8> {
    let inside = |x: f32, y: f32| {
        (0..4).all(|side| {
            let (ax, ay) = corners[side];
            let (bx, by) = corners[(side + 1) % 4];
            (bx - ax) * (y - ay) - (by - ay) * (x - ax) >= 0.0
        })
    };
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let shade = if inside(fx, fy) {
                if y % 24 < 4 && x % 40 > 14 && (260..580).contains(&x) && (150..460).contains(&y) {
                    60
                } else {
                    228
                }
            } else {
                45 + ((x * 7 + y * 13) % 11) as u8
            };
            rgba.extend_from_slice(&[shade, shade, shade.saturating_sub(8), 255]);
        }
    }
    rgba
}

#[test]
fn scan_finds_a_sheet_and_writes_it_one_bit_deep() {
    let (width, height) = (800, 600);
    let truth = [
        (220.0, 90.0),
        (610.0, 120.0),
        (640.0, 540.0),
        (170.0, 510.0),
    ];
    let rgba = photographed_sheet(width, height, truth);
    let found = crate::find_page(&rgba, width as u32, height as u32).expect("a sheet is found");
    for (corner, (x, y)) in truth.iter().enumerate() {
        let error =
            (found[corner * 2] * width as f32 - x).hypot(found[corner * 2 + 1] * height as f32 - y);
        assert!(error < 4.0, "corner {corner} is {error} px off: {found:?}");
    }

    let image = crate::scan_image_bytes(
        &rgba,
        width as u32,
        height as u32,
        found,
        1,
        crate::ScanLook::BlackWhite,
        1000,
    )
    .unwrap();
    assert!(image.starts_with(b"\x89PNG"));
    let pdf =
        crate::scans_to_pdf_bytes(&[&image], crate::ScanPaper::Sheet(595.28, 841.89)).unwrap();
    let document = Document::load_mem(&pdf).unwrap();
    let pages = document.get_pages();
    assert_eq!(pages.len(), 1);
    let images: Vec<_> = document
        .objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| {
            stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image")
        })
        .collect();
    assert_eq!(images.len(), 1);
    assert_eq!(
        images[0]
            .dict
            .get(b"BitsPerComponent")
            .unwrap()
            .as_i64()
            .unwrap(),
        1
    );
    // A quarter turn puts the sheet's long side across: a landscape page.
    let page = document.get_dictionary(pages[&1]).unwrap();
    let media = page.get(b"MediaBox").unwrap().as_array().unwrap();
    assert!(media[2].as_float().unwrap() > media[3].as_float().unwrap());
}

#[test]
fn scan_finds_nothing_on_a_plain_photo() {
    let rgba = vec![120; 320 * 240 * 4];
    assert!(crate::find_page(&rgba, 320, 240).is_none());
}
