use std::collections::BTreeMap;

use lopdf::{Document, Object, Stream, dictionary};

use super::{
    CompressOptions, ImagePdfOptions, SplitMode, compress_pdf_bytes, images_to_pdf_bytes,
    merge_pdf_bytes, organize_pdf_bytes, organize_pdfs_bytes, split_pdf_bytes,
};
use crate::compression::{deflate_best, filter_matches, image_transcode::is_jpeg_image};
use crate::documents::parse_version;
use crate::images::jpeg_orientation;

fn image_pdf_options() -> ImagePdfOptions {
    ImagePdfOptions {
        page_width: 595.0,
        page_height: 842.0,
        margin: 18.0,
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
    // startxref reaches the cross-reference stream, not the object graph, so
    // the reader always leaves that one looking unreferenced.
    let leaked = document
        .prune_objects()
        .into_iter()
        .filter(|id| types[id] != b"XRef")
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
        .filter(|id| types[id] != b"XRef")
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
