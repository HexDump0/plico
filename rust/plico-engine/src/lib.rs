use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};
use wasm_bindgen::prelude::*;

pub fn merge_pdf_bytes(files: &[&[u8]]) -> Result<Vec<u8>, String> {
    if files.len() < 2 {
        return Err("Choose at least two PDFs to merge.".into());
    }

    let mut documents = Vec::with_capacity(files.len());
    for (index, bytes) in files.iter().enumerate() {
        let document = Document::load_mem(bytes)
            .map_err(|error| format!("PDF {} could not be read: {error}", index + 1))?;
        if document.is_encrypted() {
            return Err(format!(
                "PDF {} is password protected. Unlock it before merging.",
                index + 1
            ));
        }
        if document.get_pages().is_empty() {
            return Err(format!("PDF {} has no pages.", index + 1));
        }
        documents.push(document);
    }

    merge_documents(documents)
}

fn merge_documents(documents: Vec<Document>) -> Result<Vec<u8>, String> {
    let version = documents
        .iter()
        .map(|document| document.version.as_str())
        .max()
        .unwrap_or("1.5")
        .to_owned();
    let mut output = Document::with_version(version);
    let mut next_id = 1;
    let mut pages = BTreeMap::new();
    let mut objects = BTreeMap::new();

    for mut document in documents {
        document.renumber_objects_with(next_id);
        next_id = document.max_id + 1;

        for page_id in document.get_pages().into_values() {
            let page = document
                .get_object(page_id)
                .map_err(|error| format!("A PDF page could not be read: {error}"))?
                .to_owned();
            pages.insert(page_id, page);
        }
        objects.extend(document.objects);
    }

    let mut catalog: Option<(ObjectId, Object)> = None;
    let mut page_tree: Option<(ObjectId, Object)> = None;

    for (object_id, object) in objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog.is_none() {
                    catalog = Some((object_id, object));
                }
            }
            b"Pages" => {
                if page_tree.is_none() {
                    page_tree = Some((object_id, object));
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                output.objects.insert(object_id, object);
            }
        }
    }

    let (pages_id, pages_object) = page_tree.ok_or("A PDF is missing its page tree.")?;
    let (catalog_id, catalog_object) = catalog.ok_or("A PDF is missing its document catalog.")?;

    for (object_id, object) in &pages {
        let mut dictionary = object
            .as_dict()
            .map_err(|_| "A PDF contains an invalid page object.")?
            .clone();
        dictionary.set("Parent", pages_id);
        output
            .objects
            .insert(*object_id, Object::Dictionary(dictionary));
    }

    let mut pages_dictionary = pages_object
        .as_dict()
        .map_err(|_| "A PDF contains an invalid page tree.")?
        .clone();
    pages_dictionary.set("Count", pages.len() as u32);
    pages_dictionary.set(
        "Kids",
        pages.into_keys().map(Object::Reference).collect::<Vec<_>>(),
    );
    output
        .objects
        .insert(pages_id, Object::Dictionary(pages_dictionary));

    let mut catalog_dictionary = catalog_object
        .as_dict()
        .map_err(|_| "A PDF contains an invalid document catalog.")?
        .clone();
    catalog_dictionary.set("Pages", pages_id);
    catalog_dictionary.remove(b"Outlines");
    output
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dictionary));
    output.trailer.set("Root", catalog_id);
    output.max_id = output.objects.len() as u32;
    output.renumber_objects();
    output.compress();

    let mut bytes = Vec::new();
    output
        .save_to(&mut bytes)
        .map_err(|error| format!("The merged PDF could not be created: {error}"))?;
    Ok(bytes)
}

#[wasm_bindgen]
pub fn merge_pdfs(input: &[u8], lengths: &[u32]) -> Result<Vec<u8>, JsValue> {
    let expected_length = lengths
        .iter()
        .try_fold(0usize, |total, length| total.checked_add(*length as usize));
    if expected_length != Some(input.len()) {
        return Err(JsValue::from_str("The PDF input was incomplete."));
    }

    let mut offset = 0;
    let files = lengths
        .iter()
        .map(|length| {
            let end = offset + *length as usize;
            let bytes = &input[offset..end];
            offset = end;
            bytes
        })
        .collect::<Vec<_>>();

    merge_pdf_bytes(&files).map_err(|error| JsValue::from_str(&error))
}

#[cfg(test)]
mod tests {
    use lopdf::{Document, Object, Stream, dictionary};

    use super::merge_pdf_bytes;

    fn one_page_pdf(label: &str) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let content_id =
            document.add_object(Stream::new(dictionary! {}, label.as_bytes().to_vec()));
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
}
