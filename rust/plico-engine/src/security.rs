use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{Document, EncryptionState, EncryptionVersion, LoadOptions, Object, Permissions};

use crate::documents::{MAX_DECOMPRESSED_STREAM, load_document};

/// What opening a PDF takes. `Restricted` opens without a password but carries
/// permission limits, which viewers enforce and pdf.js does not report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    None,
    Restricted,
    Password,
}

pub fn protection_of(input: &[u8]) -> Result<Protection, String> {
    let document = Document::load_mem_with_options(
        input,
        LoadOptions {
            max_decompressed_size: Some(MAX_DECOMPRESSED_STREAM),
            ..Default::default()
        },
    )
    .map_err(|error| format!("The PDF could not be read: {error}"))?;
    // lopdf drops /Encrypt once the empty password opens a file, so one still
    // in the trailer means a real password is needed (see `load_document`).
    Ok(if document.trailer.get(b"Encrypt").is_ok() {
        Protection::Password
    } else if document.was_encrypted() {
        Protection::Restricted
    } else {
        Protection::None
    })
}

/// Whether the engine can open a PDF, which is what decides that a file needs
/// repairing. A file that parses but wants a password it was not given is
/// `Locked`, not damaged: repairing it would change nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Readable { pages: u32 },
    Locked,
    Damaged,
}

pub fn condition_of(input: &[u8], password: &str) -> Condition {
    match load_document(input, 1, password) {
        Ok(document) => Condition::Readable {
            pages: document.get_pages().len() as u32,
        },
        Err(_) => match protection_of(input) {
            Ok(Protection::Password) => Condition::Locked,
            _ => Condition::Damaged,
        },
    }
}

/// Writes `input` back without its encryption and otherwise as it was: no page
/// tree rebuild and no renumbering, so references stay exactly where they
/// point. Only the object stream containers the load already unpacked, and
/// anything else unreachable, are dropped. A restricted PDF unlocks with an
/// empty `password`.
pub fn unlock_pdf_bytes(input: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let mut document = load_document(input, 1, password)?;
    if !document.was_encrypted() {
        return Err("This PDF is not password protected.".into());
    }
    document.prune_objects();
    save(document)
}

pub struct ProtectOptions<'a> {
    /// Needed to open the PDF. Empty means anyone can open it, and only the
    /// permissions below apply.
    pub user_password: &'a str,
    /// Lifts the permissions. Empty generates a random one nobody knows, so
    /// the permissions cannot be lifted at all.
    pub owner_password: &'a str,
    pub allow_printing: bool,
    pub allow_copying: bool,
    pub allow_editing: bool,
}

/// Encrypts with AES-256 (revision 6), the only handler PDF 2.0 does not
/// deprecate. `password` opens `input` if it is already protected, so this
/// also changes a password.
pub fn protect_pdf_bytes(
    input: &[u8],
    password: &str,
    options: ProtectOptions<'_>,
) -> Result<Vec<u8>, String> {
    let restricted = !(options.allow_printing && options.allow_copying && options.allow_editing);
    if options.user_password.is_empty() && !restricted {
        return Err("Choose a password or restrict what the PDF allows.".into());
    }
    // Revision 6 truncates passwords to 127 bytes of UTF-8; refuse rather than
    // quietly protect with a different password than the one typed.
    if options.user_password.len() > 127 || options.owner_password.len() > 127 {
        return Err("Passwords can be at most 127 bytes long.".into());
    }
    // Opening with a password that is also the owner password grants every
    // permission, which would make the restrictions meaningless.
    if restricted
        && !options.owner_password.is_empty()
        && options.owner_password == options.user_password
    {
        return Err("The permissions password has to differ from the open password.".into());
    }

    let mut document = load_document(input, 1, password)?;
    document.prune_objects();
    document.compress();
    ensure_file_id(&mut document)?;

    let random_owner;
    let owner_password = if options.owner_password.is_empty() {
        random_owner = random_hex(32)?;
        random_owner.as_str()
    } else {
        options.owner_password
    };
    let mut file_encryption_key = [0u8; 32];
    getrandom::fill(&mut file_encryption_key)
        .map_err(|error| format!("No secure random numbers are available: {error}"))?;

    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &file_encryption_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password: options.user_password,
        permissions: permissions(&options),
    })
    .map_err(|error| format!("The PDF could not be protected: {error}"))?;
    document
        .encrypt(&state)
        .map_err(|error| format!("The PDF could not be protected: {error}"))?;
    complete_encrypt_dictionary(&mut document)?;
    save(document)
}

/// lopdf leaves out the key lengths ISO 32000-2 (7.6.4, 7.6.5) lists for
/// AES-256. pdf.js and qpdf cope, but poppler then reads the file with the
/// wrong key length and every stream decrypts to noise.
fn complete_encrypt_dictionary(document: &mut Document) -> Result<(), String> {
    let id = document
        .trailer
        .get(b"Encrypt")
        .and_then(Object::as_reference)
        .map_err(|error| format!("The PDF could not be protected: {error}"))?;
    let encrypt = document
        .get_dictionary_mut(id)
        .map_err(|error| format!("The PDF could not be protected: {error}"))?;
    encrypt.set("Length", 256);
    if let Ok(filter) = encrypt
        .get_mut(b"CF")
        .and_then(Object::as_dict_mut)
        .and_then(|filters| filters.get_mut(b"StdCF"))
        .and_then(Object::as_dict_mut)
    {
        filter.set("Length", 32);
        filter.set("AuthEvent", "DocOpen");
    }
    Ok(())
}

fn permissions(options: &ProtectOptions<'_>) -> Permissions {
    // Screen readers keep text access whatever else is refused.
    let mut granted = Permissions::COPYABLE_FOR_ACCESSIBILITY;
    if options.allow_printing {
        granted |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
    }
    if options.allow_copying {
        granted |= Permissions::COPYABLE;
    }
    if options.allow_editing {
        granted |= Permissions::MODIFIABLE
            | Permissions::ANNOTABLE
            | Permissions::FILLABLE
            | Permissions::ASSEMBLABLE;
    }
    granted
}

/// Readers expect /ID on an encrypted file, and older handlers derive the key
/// from it. Random rather than hashed: an ID derived from the content would
/// say something about a document the password is meant to hide.
pub(crate) fn ensure_file_id(document: &mut Document) -> Result<(), String> {
    // A malformed one counts as none (pdf.js corpus: issue11651.pdf loads with
    // an /ID that writes out empty).
    let well_formed = document
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .is_ok_and(|id| {
            id.len() == 2
                && id
                    .iter()
                    .all(|part| part.as_str().is_ok_and(|part| !part.is_empty()))
        });
    if well_formed {
        return Ok(());
    }
    let mut id = [0u8; 16];
    getrandom::fill(&mut id)
        .map_err(|error| format!("No secure random numbers are available: {error}"))?;
    let id = Object::String(id.to_vec(), lopdf::StringFormat::Hexadecimal);
    document.trailer.set("ID", vec![id.clone(), id]);
    Ok(())
}

fn random_hex(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer)
        .map_err(|error| format!("No secure random numbers are available: {error}"))?;
    Ok(buffer.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn save(mut document: Document) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .map_err(|error| format!("The PDF could not be created: {error}"))?;
    Ok(bytes)
}
