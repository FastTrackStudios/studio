//! Parsing logic for `reaper-fxfolders.ini` files.

use std::collections::HashMap;

use thiserror::Error;

use crate::ini::IniFile;

use super::types::{FxFolder, FxFolderItem, FxFolders, PluginType};

/// The magic type code that marks a folder as a smart folder.
const SMART_FOLDER_TYPE: u32 = 1048576;

/// Errors that can occur during parsing.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid integer value '{value}' on line {line}: {source}")]
    InvalidInt {
        line: usize,
        value: String,
        #[source]
        source: std::num::ParseIntError,
    },

    #[error("duplicate key '{key}' in section '[{section}]' on line {line}")]
    DuplicateKey {
        line: usize,
        section: String,
        key: String,
    },

    #[error("missing 'NbFolders' entry in [Folders] section")]
    MissingNbFolders,

    #[error(
        "folder index {index} has a 'NameN' entry but no corresponding 'IdN' entry in [Folders]"
    )]
    MissingFolderId { index: u32 },
}

// ---------------------------------------------------------------------------
// Public parser
// ---------------------------------------------------------------------------

/// Parse the full contents of a `reaper-fxfolders.ini` file.
pub fn parse(input: &str) -> Result<FxFolders, ParseError> {
    let ini = IniFile::parse(input);

    // -----------------------------------------------------------------------
    // 1.  Parse [Folders] section.
    // -----------------------------------------------------------------------
    let folders_entries = ini.section_entries("Folders");

    // Build a first-occurrence map for simple scalar lookups.
    let mut folders_kv: HashMap<&str, &str> = HashMap::new();
    for (key, value) in &folders_entries {
        folders_kv.entry(key).or_insert(value);
    }

    // NbFolders
    let nb_folders: u32 = if let Some(v) = folders_kv.get("NbFolders") {
        v.trim()
            .parse::<u32>()
            .map_err(|e| ParseError::InvalidInt {
                line: 0,
                value: v.to_string(),
                source: e,
            })?
    } else {
        // An empty / absent [Folders] section is valid — no folders.
        return Ok(FxFolders::default());
    };

    // Collect NameN and IdN pairs.  The key variants seen in the wild are:
    //   NameN  (always)
    //   IDN    (uppercase, used in some examples)
    //   IdN    (mixed-case, used in other examples)
    // We normalise to lowercase for lookup.
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut ids: HashMap<u32, u32> = HashMap::new();

    for (key, value) in &folders_entries {
        let key_lower = key.to_ascii_lowercase();
        if let Some(rest) = key_lower.strip_prefix("name") {
            if let Ok(idx) = rest.parse::<u32>() {
                names.entry(idx).or_insert_with(|| value.to_string());
            }
        } else if let Some(rest) = key_lower.strip_prefix("id")
            && let Ok(idx) = rest.parse::<u32>()
        {
            let ref_id = value
                .trim()
                .parse::<u32>()
                .map_err(|e| ParseError::InvalidInt {
                    line: 0,
                    value: value.to_string(),
                    source: e,
                })?;
            ids.entry(idx).or_insert(ref_id);
        }
    }

    // -----------------------------------------------------------------------
    // 2.  Parse [FolderN] sections and assemble FxFolder list.
    // -----------------------------------------------------------------------
    let mut folders: Vec<FxFolder> = Vec::with_capacity(nb_folders as usize);

    for folder_idx in 0..nb_folders {
        let name = match names.get(&folder_idx) {
            Some(n) if !n.is_empty() => n.clone(),
            // Folders with empty names are ignored by REAPER; we skip them too.
            _ => continue,
        };

        let folder_ref_id = match ids.get(&folder_idx) {
            Some(&id) => id,
            None => return Err(ParseError::MissingFolderId { index: folder_idx }),
        };

        // The corresponding section is named "Folder{folder_ref_id}".
        let section_name = format!("Folder{}", folder_ref_id);
        let folder_entries = ini.section_entries(&section_name);

        // Deduplicate: only first occurrence per key.
        let mut folder_kv: HashMap<&str, &str> = HashMap::new();
        for (key, value) in &folder_entries {
            folder_kv.entry(key).or_insert(value);
        }

        // Nb — number of items (items with index >= Nb are ignored).
        let nb: u32 = if let Some(v) = folder_kv.get("Nb") {
            v.trim()
                .parse::<u32>()
                .map_err(|e| ParseError::InvalidInt {
                    line: 0,
                    value: v.to_string(),
                    source: e,
                })?
        } else {
            0
        };

        let items = parse_folder_items(&folder_kv, nb)?;

        folders.push(FxFolder {
            name,
            folder_ref_id,
            items,
        });
    }

    Ok(FxFolders { folders })
}

/// Parse the `TypeN`/`ItemN` pairs from a folder section's key-value map.
fn parse_folder_items(kv: &HashMap<&str, &str>, nb: u32) -> Result<Vec<FxFolderItem>, ParseError> {
    let mut items: Vec<FxFolderItem> = Vec::with_capacity(nb as usize);

    for item_idx in 0..nb {
        let type_key = format!("Type{}", item_idx);
        let item_key = format!("Item{}", item_idx);

        let type_val = match kv.get(type_key.as_str()) {
            Some(v) => *v,
            // A TypeN without a matching ItemN is ignored per the spec.
            None => continue,
        };
        let item_val = match kv.get(item_key.as_str()) {
            Some(v) => *v,
            // A TypeN without a matching ItemN is ignored per the spec.
            None => continue,
        };

        let type_code = type_val
            .trim()
            .parse::<u32>()
            .map_err(|e| ParseError::InvalidInt {
                line: 0,
                value: type_val.to_string(),
                source: e,
            })?;

        let item = if type_code == SMART_FOLDER_TYPE {
            FxFolderItem::SmartFilter {
                filter: item_val.to_string(),
            }
        } else {
            FxFolderItem::Plugin {
                plugin_type: PluginType::from_raw(type_code),
                name: item_val.to_string(),
            }
        };

        items.push(item);
    }

    Ok(items)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
