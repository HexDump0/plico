//! Structural checks over a directory of real PDFs.
//!
//! Ignored by default because it needs a corpus on disk. Point `PLICO_CORPUS` at
//! one and run:
//!
//! ```sh
//! cargo test --release --test corpus -- --ignored --nocapture
//! ```
//!
//! Every file that lopdf can load on its own is merged with a small generated
//! page, and the result is checked for the invariants merging is supposed to
//! hold. Files the loader rejects outright are counted, not failed: a corpus
//! like pdf.js's contains deliberately broken documents.
//!
//! This validates structure, not rendering. Two pages can both parse and still
//! differ visually, so a rasterising comparison against a reference merger is
//! the next thing this should grow.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use lopdf::{Document, LoadOptions, Object, Stream, dictionary};
use plico_engine::{
    CompressOptions, FontFamily, PageImage, PageNumberOptions, PdfALevel, Position, ProtectOptions,
    RedactOptions, Redacted, Redaction, SplitMode, StandardFont, TextStyle, WatermarkContent,
    WatermarkOptions, add_page_numbers_bytes, add_watermark_bytes, compress_pdf_bytes,
    convert_to_pdfa_bytes, merge_pdf_bytes, page_texts, protect_pdf_bytes, redact_pdf_bytes,
    split_pdf_bytes, split_pdf_bytes_with_password, unlock_pdf_bytes,
};

const DEFAULT_CORPUS: &str = "../../testing/pdfjs/test/pdfs";

fn marker_pdf() -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, b"marker".to_vec()));
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

fn corpus_files() -> Vec<PathBuf> {
    let root = std::env::var("PLICO_CORPUS").unwrap_or_else(|_| DEFAULT_CORPUS.to_owned());
    let root = Path::new(&root);
    let Ok(entries) = fs::read_dir(root) else {
        panic!("no corpus at {}; set PLICO_CORPUS", root.display());
    };

    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pdf"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Page count, and whether every page resolves the required /MediaBox.
fn describe(document: &Document) -> (usize, bool) {
    let pages = document.get_pages();
    let complete = pages.values().all(|id| {
        document
            .get_dictionary(*id)
            .is_ok_and(|p| p.has(b"MediaBox"))
    });
    (pages.len(), complete)
}

fn unreachable_count(document: &mut Document) -> usize {
    let types = document
        .objects
        .iter()
        .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
        .collect::<BTreeMap<_, _>>();
    // startxref reaches the cross-reference stream, not the object graph, and
    // the reader keeps the object streams it unpacked.
    document
        .prune_objects()
        .into_iter()
        .filter(|id| !matches!(types[id].as_slice(), b"XRef" | b"ObjStm"))
        .count()
}

#[test]
#[ignore = "needs a PDF corpus on disk"]
fn merges_every_loadable_document() {
    let marker = marker_pdf();
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");

    let mut unloadable = 0usize;
    let mut encrypted = 0usize;
    let mut password_protected = 0usize;
    let mut needs_password = 0usize;
    let mut merged_ok = 0usize;
    let mut input_bytes = 0u64;
    let mut output_bytes = 0u64;

    let mut merge_failed: Vec<String> = Vec::new();
    let mut reload_failed: Vec<String> = Vec::new();
    let mut wrong_page_count: Vec<String> = Vec::new();
    let mut missing_mediabox: Vec<String> = Vec::new();
    let mut leaked_objects: Vec<String> = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };

        // Only hold the engine to files it could read standalone.
        let Ok(mut source) = Document::load_mem(&bytes) else {
            unloadable += 1;
            continue;
        };
        // Not skipped: a document restricted by an owner password only should
        // still merge, so this exercises the empty-password path. The page tree
        // is only reachable once it is decrypted.
        if source.is_encrypted() {
            encrypted += 1;
            if source.decrypt("").is_err() {
                needs_password += 1;
                continue;
            }
        }
        let (source_pages, _) = describe(&source);
        if source_pages == 0 {
            unloadable += 1;
            continue;
        }
        drop(source);

        let merged = match merge_pdf_bytes(&[&bytes, &marker]) {
            Ok(merged) => merged,
            Err(error) => {
                if error.contains("password protected") {
                    password_protected += 1;
                } else {
                    merge_failed.push(format!("{name}: {error}"));
                }
                continue;
            }
        };

        let mut result = match Document::load_mem(&merged) {
            Ok(document) => document,
            Err(error) => {
                reload_failed.push(format!("{name}: {error}"));
                continue;
            }
        };

        let (pages, complete) = describe(&result);
        if pages != source_pages + 1 {
            wrong_page_count.push(format!("{name}: {source_pages} + 1 -> {pages}"));
        }
        if !complete {
            missing_mediabox.push(name.clone());
        }
        let leaked = unreachable_count(&mut result);
        if leaked > 0 {
            leaked_objects.push(format!("{name}: {leaked}"));
        }

        merged_ok += 1;
        input_bytes += (bytes.len() + marker.len()) as u64;
        output_bytes += merged.len() as u64;
    }

    let report = |label: &str, cases: &[String]| {
        if cases.is_empty() {
            return;
        }
        println!("\n{label}: {}", cases.len());
        for case in cases.iter().take(15) {
            println!("  {case}");
        }
        if cases.len() > 15 {
            println!("  ... and {} more", cases.len() - 15);
        }
    };

    println!("\ncorpus: {} files", files.len());
    println!("  skipped, would not load standalone: {unloadable}");
    println!("  carried an /Encrypt dictionary: {encrypted}");
    println!("  encrypted, needs a real password: {needs_password}");
    println!("  refused as password protected by the engine: {password_protected}");
    println!("  merged and checked: {merged_ok}");
    println!(
        "  size: {:.1} MiB in -> {:.1} MiB out ({:.1}%)",
        input_bytes as f64 / 1048576.0,
        output_bytes as f64 / 1048576.0,
        output_bytes as f64 / input_bytes.max(1) as f64 * 100.0
    );

    report("merge returned an error", &merge_failed);
    report("merged output would not reparse", &reload_failed);
    report("page count wrong", &wrong_page_count);
    report("page without a resolvable MediaBox", &missing_mediabox);
    report("unreachable objects kept", &leaked_objects);

    assert!(merge_failed.is_empty(), "merge rejected loadable documents");
    assert!(reload_failed.is_empty(), "merged output does not reparse");
    assert!(wrong_page_count.is_empty(), "pages lost or duplicated");
    assert!(missing_mediabox.is_empty(), "pages lost their geometry");
}

#[test]
#[ignore = "needs a PDF corpus on disk"]
fn splits_last_page_of_every_loadable_document() {
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(mut source) = Document::load_mem(&bytes) else {
            continue;
        };
        if source.is_encrypted() && source.decrypt("").is_err() {
            continue;
        }
        let pages = source.get_pages();
        let Some((&last_number, &last_id)) = pages.last_key_value() else {
            continue;
        };
        let original_content = source.get_page_content(last_id);
        drop(source);

        let outputs = match split_pdf_bytes(
            &bytes,
            SplitMode::Ranges(&[(last_number, last_number)], false),
        ) {
            Ok(outputs) => outputs,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        let Ok(mut result) = Document::load_mem(&outputs[0]) else {
            failures.push(format!("{name}: split output would not reparse"));
            continue;
        };
        let (count, complete) = describe(&result);
        if count != 1 || !complete {
            failures.push(format!(
                "{name}: {count} pages, MediaBox complete: {complete}"
            ));
            continue;
        }
        if result.get_page_content(result.get_pages()[&1]) != original_content {
            failures.push(format!("{name}: selected page content changed"));
            continue;
        }
        let leaked = unreachable_count(&mut result);
        if leaked > 0 {
            failures.push(format!("{name}: {leaked} unreachable objects"));
            continue;
        }
        checked += 1;
    }

    println!("\nsplit corpus: {checked} files checked");
    for failure in failures.iter().take(15) {
        println!("  {failure}");
    }
    assert!(failures.is_empty(), "split failed on real PDFs");
}

/// Compresses every loadable corpus file at the strongest setting and checks
/// the result still reparses with identical pages. This is the structural half
/// of the quality question; rendered output still wants a rasterising compare.
#[test]
#[ignore = "needs a PDF corpus on disk"]
fn compresses_every_loadable_document() {
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");

    let mut checked = 0usize;
    let mut input_bytes = 0u64;
    let mut output_bytes = 0u64;
    let mut shrunk = 0usize;
    let mut kept_original = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(mut source) = Document::load_mem(&bytes) else {
            continue;
        };
        if source.is_encrypted() && source.decrypt("").is_err() {
            continue;
        }
        let pages = source.get_pages();
        if pages.is_empty() {
            continue;
        }
        let contents = pages
            .values()
            .map(|id| source.get_page_content(*id))
            .collect::<Vec<_>>();
        drop(source);

        let compressed = match compress_pdf_bytes(
            &bytes,
            CompressOptions {
                reflate: true,
                image_quality: 50,
                max_image_dimension: 1600,
                remove_metadata: false,
                remove_thumbnails: false,
            },
        ) {
            Ok(compressed) => compressed,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        // A file the engine cannot shrink comes back as the original bytes.
        // Every check below is then a property of the source, not of a rebuild.
        if compressed.len() == bytes.len() {
            kept_original += 1;
            input_bytes += bytes.len() as u64;
            output_bytes += compressed.len() as u64;
            checked += 1;
            continue;
        }
        let Ok(mut result) = Document::load_mem(&compressed) else {
            failures.push(format!("{name}: compressed output would not reparse"));
            continue;
        };
        let (count, _) = describe(&result);
        // Unlike merge, compress keeps the original page tree, so pages may
        // still resolve /MediaBox through their ancestors.
        if count != contents.len() {
            failures.push(format!("{count} pages, expected {}", contents.len()));
            continue;
        }
        // Recompression must not change decoded content, only how it is packed.
        if result
            .get_pages()
            .into_values()
            .map(|id| result.get_page_content(id))
            .zip(&contents)
            .any(|(after, before)| &after != before)
        {
            failures.push(format!("{name}: page content changed"));
            continue;
        }
        let types = result
            .objects
            .iter()
            .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
            .collect::<BTreeMap<_, _>>();
        let leaked = result
            .prune_objects()
            .into_iter()
            .filter(|id| types[id] != b"XRef" && types[id] != b"ObjStm")
            .count();
        if leaked > 0 {
            failures.push(format!("{name}: {leaked} unreachable objects"));
            continue;
        }

        checked += 1;
        input_bytes += bytes.len() as u64;
        output_bytes += compressed.len() as u64;
        shrunk += 1;
    }

    println!(
        "\ncompress corpus: {checked} files checked, {shrunk} rebuilt smaller, {kept_original} passed through"
    );
    println!(
        "  size: {:.1} MiB in -> {:.1} MiB out ({:.1}%)",
        input_bytes as f64 / 1048576.0,
        output_bytes as f64 / 1048576.0,
        output_bytes as f64 / input_bytes.max(1) as f64 * 100.0
    );
    for failure in failures.iter().take(15) {
        println!("  {failure}");
    }
    assert!(failures.is_empty(), "compress failed on real PDFs");
}

/// The protected files in pdf.js's corpus, with the passwords its own test
/// manifest supplies. They cover RC4 and AES, revisions 2 to 6, and passwords
/// that need PDFDocEncoding or SASLprep.
const PROTECTED: [(&str, &str); 7] = [
    ("issue15893_reduced.pdf", "test"),
    ("issue3371.pdf", "ELXRTQWS"),
    ("issue6010_1.pdf", "abc"),
    ("issue6010_2.pdf", "æøå"),
    ("bug1782186.pdf", "Hello"),
    ("issue21579.pdf", "pässwört"),
    ("saslprep-r6.pdf", "SªSL\u{ad}prep"),
];

/// Every page must come out unlocked and carry exactly the content lopdf reads
/// from the source when given the same password. A wrong password must fail
/// rather than decrypt into noise.
#[test]
#[ignore = "needs a PDF corpus on disk"]
fn unlocks_protected_documents_with_their_passwords() {
    let root = std::env::var("PLICO_CORPUS").unwrap_or_else(|_| DEFAULT_CORPUS.to_owned());
    let mut checked = 0usize;
    let mut unsupported = Vec::new();
    let mut failures = Vec::new();
    for (name, password) in PROTECTED {
        let Ok(bytes) = fs::read(Path::new(&root).join(name)) else {
            continue;
        };
        let options = LoadOptions::with_password(password);
        let Ok(source) = Document::load_mem_with_options(&bytes, options) else {
            // Encryption lopdf cannot handle has to be refused, never written
            // out as whatever a failed decrypt left behind.
            if split_pdf_bytes_with_password(&bytes, password, SplitMode::Every(u32::MAX)).is_ok() {
                failures.push(format!(
                    "{name}: lopdf cannot decrypt it, yet it produced output"
                ));
            } else {
                unsupported.push(name);
            }
            continue;
        };
        let expected = source
            .get_pages()
            .into_values()
            .map(|id| source.get_page_content(id))
            .collect::<Vec<_>>();

        if split_pdf_bytes(&bytes, SplitMode::Every(u32::MAX)).is_ok() {
            failures.push(format!("{name}: opened without its password"));
            continue;
        }
        if split_pdf_bytes_with_password(&bytes, "wrong", SplitMode::Every(u32::MAX)).is_ok() {
            failures.push(format!("{name}: opened with a wrong password"));
            continue;
        }
        let outputs =
            match split_pdf_bytes_with_password(&bytes, password, SplitMode::Every(u32::MAX)) {
                Ok(outputs) => outputs,
                Err(error) => {
                    failures.push(format!("{name}: {error}"));
                    continue;
                }
            };
        let result = Document::load_mem(&outputs[0]).unwrap();
        if result.is_encrypted() {
            failures.push(format!("{name}: output is still encrypted"));
            continue;
        }
        let actual = result
            .get_pages()
            .into_values()
            .map(|id| result.get_page_content(id))
            .collect::<Vec<_>>();
        if actual != expected {
            failures.push(format!(
                "{name}: page content differs from the decrypted source"
            ));
            continue;
        }
        let unlocked = unlock_pdf_bytes(&bytes, password)
            .ok()
            .and_then(|bytes| Document::load_mem(&bytes).ok());
        let unlocked_pages = unlocked.map(|document| {
            document
                .get_pages()
                .into_values()
                .map(|id| document.get_page_content(id))
                .collect::<Vec<_>>()
        });
        if unlocked_pages.as_ref() != Some(&expected) {
            failures.push(format!("{name}: unlocking changed or lost page content"));
            continue;
        }
        checked += 1;
    }

    println!(
        "\nprotected corpus: {checked} of {} unlocked, refused as unsupported: {unsupported:?}",
        PROTECTED.len()
    );
    for failure in &failures {
        println!("  {failure}");
    }
    assert!(failures.is_empty(), "unlocking failed on real PDFs");
}

/// Protects every loadable corpus file with a password and restrictions, then
/// requires it to reopen only with that password, with every page's content
/// unchanged, and to unlock back to the same pages.
#[test]
#[ignore = "needs a PDF corpus on disk"]
fn protects_every_loadable_document() {
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(source) = Document::load_mem(&bytes) else {
            continue;
        };
        if source.trailer.get(b"Encrypt").is_ok() || source.get_pages().is_empty() {
            continue;
        }
        let expected = source
            .get_pages()
            .into_values()
            .map(|id| source.get_page_content(id))
            .collect::<Vec<_>>();
        drop(source);

        let locked = match protect_pdf_bytes(
            &bytes,
            "",
            ProtectOptions {
                user_password: "open sesame",
                owner_password: "",
                allow_printing: false,
                allow_copying: true,
                allow_editing: false,
            },
        ) {
            Ok(locked) => locked,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        if Document::load_mem(&locked).is_ok_and(|document| !document.get_pages().is_empty()) {
            failures.push(format!("{name}: opened without its password"));
            continue;
        }
        let reopened =
            Document::load_mem_with_options(&locked, LoadOptions::with_password("open sesame"));
        let pages = reopened.map(|document| {
            document
                .get_pages()
                .into_values()
                .map(|id| document.get_page_content(id))
                .collect::<Vec<_>>()
        });
        if pages.as_ref().ok() != Some(&expected) {
            failures.push(format!("{name}: pages changed once protected"));
            continue;
        }
        let unlocked = unlock_pdf_bytes(&locked, "open sesame")
            .ok()
            .and_then(|bytes| Document::load_mem(&bytes).ok())
            .map(|document| {
                document
                    .get_pages()
                    .into_values()
                    .map(|id| document.get_page_content(id))
                    .collect::<Vec<_>>()
            });
        if unlocked.as_ref() != Some(&expected) {
            failures.push(format!("{name}: pages changed after unlocking"));
            continue;
        }
        checked += 1;
    }

    println!("\nprotect corpus: {checked} files protected, reopened and unlocked");
    for failure in failures.iter().take(15) {
        println!("  {failure}");
    }
    assert!(failures.is_empty(), "protecting failed on real PDFs");
}

/// Numbers every page of every loadable corpus file, then watermarks it, and
/// requires the result to reparse with the same pages, every page's original
/// content intact and in place, both stamps drawn once each, and nothing left
/// unreachable.
#[test]
#[ignore = "needs a PDF corpus on disk"]
fn stamps_every_loadable_document() {
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");

    let style = TextStyle {
        family: FontFamily::Helvetica,
        bold: true,
        size: 12.0,
        color: [0.2, 0.2, 0.2],
    };
    let mut checked = 0usize;
    let mut damaged_trees = Vec::new();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(mut source) = Document::load_mem(&bytes) else {
            continue;
        };
        if source.is_encrypted() && source.decrypt("").is_err() {
            continue;
        }
        let pages = source.get_pages();
        if pages.is_empty() {
            continue;
        }
        let contents = pages
            .values()
            .map(|id| source.get_page_content(*id))
            .collect::<Vec<_>>();
        drop(source);

        let stamped = add_page_numbers_bytes(
            &bytes,
            "",
            PageNumberOptions {
                template: "Page {n} of {total}",
                first_number: 1,
                pages: &[],
                position: Position::BottomRight,
                margin: 24.0,
                style,
                opacity: 1.0,
            },
        )
        .and_then(|numbered| {
            add_watermark_bytes(
                &numbered,
                "",
                WatermarkOptions {
                    content: WatermarkContent::Text {
                        text: "CONFIDENTIAL",
                        style: TextStyle {
                            size: 48.0,
                            ..style
                        },
                    },
                    pages: &[],
                    position: Position::Center,
                    margin: 24.0,
                    rotation: 45.0,
                    opacity: 0.3,
                    behind: false,
                    tile: false,
                },
            )
        });
        let stamped = match stamped {
            Ok(stamped) => stamped,
            // A page lopdf cannot read is refused rather than miscounted.
            Err(error) if error.contains("damaged") => {
                damaged_trees.push(name);
                continue;
            }
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        let Ok(mut result) = Document::load_mem(&stamped) else {
            failures.push(format!("{name}: stamped output would not reparse"));
            continue;
        };
        let pages = result.get_pages();
        if pages.len() != contents.len() {
            failures.push(format!(
                "{name}: {} pages, expected {}",
                pages.len(),
                contents.len()
            ));
            continue;
        }
        let draws = |content: &[u8]| content.windows(3).filter(|window| window == b" Do").count();
        let damaged =
            pages
                .values()
                .zip(&contents)
                .enumerate()
                .find_map(|(index, (id, before))| {
                    let after = result.get_page_content(*id);
                    if draws(&after) != draws(before) + 2 {
                        Some(format!(
                            "page {} draws {} times, expected {} + 2",
                            index + 1,
                            draws(&after),
                            draws(before)
                        ))
                    } else if !before.is_empty()
                        && !after
                            .windows(before.len())
                            .any(|window| window == &before[..])
                    {
                        Some(format!("page {} lost its content", index + 1))
                    } else {
                        None
                    }
                });
        if let Some(damage) = damaged {
            failures.push(format!("{name}: {damage}"));
            continue;
        }
        let types = result
            .objects
            .iter()
            .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
            .collect::<BTreeMap<_, _>>();
        let leaked = result
            .prune_objects()
            .into_iter()
            .filter(|id| types[id] != b"XRef" && types[id] != b"ObjStm")
            .count();
        if leaked > 0 {
            failures.push(format!("{name}: {leaked} unreachable objects"));
            continue;
        }
        checked += 1;
    }

    println!("\nstamp corpus: {checked} files numbered and watermarked");
    println!("  refused, page tree lists an unreadable page: {damaged_trees:?}");
    for failure in failures.iter().take(15) {
        println!("  {failure}");
    }
    if failures.len() > 15 {
        println!("  ... and {} more", failures.len() - 15);
    }
    assert!(failures.is_empty(), "stamping failed on real PDFs");
}

/// Structural only: a PDF/A claim needs a validator. Set `PLICO_PDFA_OUT` to a
/// directory to keep every output, then check them with veraPDF
/// (`scripts/pdfa-check.sh` does both).
#[test]
#[ignore = "needs a PDF corpus on disk"]
fn converts_every_loadable_document_to_pdfa() {
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");
    let output = std::env::var("PLICO_PDFA_OUT").ok().map(PathBuf::from);
    if let Some(output) = &output {
        fs::create_dir_all(output).unwrap();
    }

    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../static/pdfa-fonts");
    let substitutes = fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cff"))
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let metrics = fs::read_to_string(path.with_extension("txt")).unwrap();
            (name, fs::read(&path).unwrap(), metrics)
        })
        .collect::<Vec<_>>();
    let mut converted = 0usize;
    let mut refusals: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(mut source) = Document::load_mem(&bytes) else {
            continue;
        };
        if source.is_encrypted() && source.decrypt("").is_err() {
            continue;
        }
        let pages = source.get_pages().len();
        if pages == 0 {
            continue;
        }
        drop(source);

        let fonts = substitutes
            .iter()
            .map(|(name, program, metrics)| StandardFont {
                name,
                program,
                metrics,
            })
            .collect::<Vec<_>>();
        let result = match convert_to_pdfa_bytes(&bytes, "", PdfALevel::A2b, &fonts) {
            Ok(result) => result,
            Err(error) => {
                // Group refusals by reason, not by the font or page they name.
                let reason = error
                    .split('“')
                    .next()
                    .unwrap_or(&error)
                    .split(" on page ")
                    .next()
                    .unwrap_or(&error)
                    .to_owned();
                refusals.entry(reason).or_default().push(name);
                continue;
            }
        };
        let Ok(reloaded) = Document::load_mem(&result) else {
            failures.push(format!("{name}: output would not reparse"));
            continue;
        };
        if reloaded.get_pages().len() != pages {
            failures.push(format!(
                "{name}: {} pages, expected {pages}",
                reloaded.get_pages().len()
            ));
            continue;
        }
        if let Some(output) = &output {
            fs::write(output.join(&name), &result).unwrap();
        }
        converted += 1;
    }

    println!("\npdf/a corpus: {converted} files converted to PDF/A-2b");
    for (reason, names) in &refusals {
        println!("  refused {}: {reason}", names.len());
    }
    for failure in failures.iter().take(15) {
        println!("  {failure}");
    }
    assert!(failures.is_empty(), "PDF/A conversion broke real PDFs");
}

/// A box across the middle of every page, where most pages have text,
/// images and paths. Pages the engine asks a picture for get a blank one, as
/// structure is all this checks; `npm run test:raster:redact` renders them.
#[test]
#[ignore = "needs a PDF corpus on disk"]
fn redacts_every_loadable_document() {
    let files = corpus_files();
    assert!(!files.is_empty(), "corpus is empty");
    let area = [0.2, 0.3, 0.8, 0.6];
    let picture = {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0, 0, 0])
            .unwrap();
        bytes
    };

    let mut checked = 0usize;
    let mut pages_checked = 0usize;
    let mut pictured_files = Vec::new();
    let mut pictured_pages = 0usize;
    let mut reasons = BTreeMap::new();
    let mut refused = Vec::new();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(source) = Document::load_mem(&bytes) else {
            continue;
        };
        if source.is_encrypted() {
            continue;
        }
        let count = source.get_pages().len();
        if count == 0 {
            continue;
        }
        drop(source);
        let redactions = (1..=count as u32)
            .map(|page| Redaction { page, area })
            .collect::<Vec<_>>();
        let run = |images: &[PageImage<'_>]| {
            std::panic::catch_unwind(|| {
                redact_pdf_bytes(
                    &bytes,
                    "",
                    RedactOptions {
                        redactions: &redactions,
                        color: [0.0; 3],
                        remove_metadata: false,
                        page_images: images,
                    },
                )
            })
        };
        let first = match run(&[]) {
            Ok(result) => result,
            Err(_) => {
                failures.push(format!("{name}: panicked"));
                continue;
            }
        };
        let (output, imaged) = match first {
            Ok(Redacted::Done { bytes, imaged }) => (bytes, imaged),
            Ok(Redacted::NeedsImages(pages)) => {
                for (_, reason) in &pages {
                    *reasons.entry(format!("{reason:?}")).or_insert(0usize) += 1;
                }
                let images = pages
                    .iter()
                    .map(|&(page, _)| PageImage {
                        page,
                        image: &picture,
                    })
                    .collect::<Vec<_>>();
                match run(&images) {
                    Ok(Ok(Redacted::Done { bytes, imaged })) => (bytes, imaged),
                    Ok(Ok(Redacted::NeedsImages(more))) => {
                        failures.push(format!("{name}: asked again for pictures of {more:?}"));
                        continue;
                    }
                    Ok(Err(error)) => {
                        failures.push(format!("{name}: {error}"));
                        continue;
                    }
                    Err(_) => {
                        failures.push(format!("{name}: panicked with pictures"));
                        continue;
                    }
                }
            }
            Err(error) if error.contains("damaged") => {
                refused.push(name);
                continue;
            }
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        let Ok(mut result) = Document::load_mem(&output) else {
            failures.push(format!("{name}: redacted output would not reparse"));
            continue;
        };
        if result.get_pages().len() != count {
            failures.push(format!(
                "{name}: {} pages, expected {count}",
                result.get_pages().len()
            ));
            continue;
        }
        let types = result
            .objects
            .iter()
            .map(|(id, object)| (*id, object.type_name().unwrap_or(b"").to_vec()))
            .collect::<BTreeMap<_, _>>();
        let leaked = result
            .prune_objects()
            .into_iter()
            .filter(|id| types[id] != b"XRef" && types[id] != b"ObjStm")
            .count();
        if leaked > 0 {
            failures.push(format!("{name}: {leaked} unreachable objects"));
            continue;
        }
        // Nothing the engine itself reads as text may still lie under a box.
        let texts = match page_texts(&output, "") {
            Ok(texts) => texts,
            Err(error) => {
                failures.push(format!("{name}: output text unreadable: {error}"));
                continue;
            }
        };
        let survivor = texts.iter().enumerate().find_map(|(index, page)| {
            page.boxes.iter().zip(&page.text).find_map(|(glyph, text)| {
                let [left, top, right, bottom] = *glyph;
                let width = (right.min(area[2]) - left.max(area[0])).max(0.0);
                let height = (bottom.min(area[3]) - top.max(area[1])).max(0.0);
                let size = (right - left) * (bottom - top);
                let covered = if size <= 1e-9 {
                    let (x, y) = ((left + right) / 2.0, (top + bottom) / 2.0);
                    x > area[0] && x < area[2] && y > area[1] && y < area[3]
                } else {
                    width * height >= 0.3 * size
                };
                covered.then(|| format!("page {}: {text:?} at {glyph:?}", index + 1))
            })
        });
        if let Some(survivor) = survivor {
            failures.push(format!("{name}: text left under the box, {survivor}"));
            continue;
        }
        if !imaged.is_empty() {
            pictured_pages += imaged.len();
            pictured_files.push(format!("{name} {imaged:?}"));
        }
        pages_checked += count;
        checked += 1;
    }

    println!("\nredact corpus: {checked} files, {pages_checked} pages redacted and checked");
    println!(
        "  pages drawn from a picture: {pictured_pages} in {} files",
        pictured_files.len()
    );
    println!("  why: {reasons:?}");
    for pictured in pictured_files.iter().take(40) {
        println!("    {pictured}");
    }
    println!("  refused, page tree lists an unreadable page: {refused:?}");
    for failure in failures.iter().take(30) {
        println!("  {failure}");
    }
    if failures.len() > 30 {
        println!("  ... and {} more", failures.len() - 30);
    }
    assert!(failures.is_empty(), "redaction failed on real PDFs");
}
