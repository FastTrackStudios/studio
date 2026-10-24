//! Serialization logic for writing `reaper-fxfolders.ini` files.

use super::types::{FxFolder, FxFolderItem, FxFolders, PluginType};

/// The magic type code that marks a smart folder in the INI file.
const SMART_FOLDER_TYPE: u32 = 1048576;

impl FxFolders {
    /// Serialize this `FxFolders` back to the `reaper-fxfolders.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CR-LF) to match REAPER's format.
    pub fn serialize(&self) -> String {
        let mut out = String::new();

        // -----------------------------------------------------------------------
        // [Folders] section
        // -----------------------------------------------------------------------
        out.push_str("[Folders]\r\n");
        out.push_str(&format!("NbFolders={}\r\n", self.folders.len()));

        for (idx, folder) in self.folders.iter().enumerate() {
            out.push_str(&format!("Name{}={}\r\n", idx, folder.name));
            out.push_str(&format!("ID{}={}\r\n", idx, folder.folder_ref_id));
        }

        // -----------------------------------------------------------------------
        // One [FolderN] section per unique folder_ref_id.
        // We emit a section for every folder in order.  When multiple named
        // folders share the same folder_ref_id the section is only written once
        // (on its first encounter) to avoid duplicates.
        // -----------------------------------------------------------------------
        let mut emitted_ids: Vec<u32> = Vec::new();

        for folder in &self.folders {
            if emitted_ids.contains(&folder.folder_ref_id) {
                continue;
            }
            emitted_ids.push(folder.folder_ref_id);
            serialize_folder_section(folder, &mut out);
        }

        out
    }
}

fn serialize_folder_section(folder: &FxFolder, out: &mut String) {
    out.push_str(&format!("[Folder{}]\r\n", folder.folder_ref_id));
    out.push_str(&format!("Nb={}\r\n", folder.items.len()));

    for (idx, item) in folder.items.iter().enumerate() {
        match item {
            FxFolderItem::Plugin { plugin_type, name } => {
                out.push_str(&format!("Type{}={}\r\n", idx, plugin_type.to_raw()));
                out.push_str(&format!("Item{}={}\r\n", idx, name));
            }
            FxFolderItem::SmartFilter { filter } => {
                out.push_str(&format!("Type{}={}\r\n", idx, SMART_FOLDER_TYPE));
                out.push_str(&format!("Item{}={}\r\n", idx, filter));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
