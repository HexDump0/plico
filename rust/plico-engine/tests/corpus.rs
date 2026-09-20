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

use lopdf::{Document, Object, Stream, dictionary};
use plico_engine::{
    CompressOptions, SplitMode, compress_pdf_bytes, merge_pdf_bytes, split_pdf_bytes,
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
    // startxref reaches the cross-reference stream, not the object graph.
    document
        .prune_objects()
        .into_iter()
        .filter(|id| types[id] != b"XRef")
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
