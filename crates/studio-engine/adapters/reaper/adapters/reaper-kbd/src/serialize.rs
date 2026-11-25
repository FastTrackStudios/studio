//! Serialization logic for writing REAPER keyboard mapping files.

use crate::types::*;

impl KeyMap {
    /// Serialize this `KeyMap` back to the `reaper-kb.ini` text format.
    ///
    /// Each line is terminated with `\r\n` (CR-LF) to match REAPER's format.
    pub fn serialize(&self) -> String {
        let mut output = String::new();
        for entry in &self.entries {
            match entry {
                Entry::Key(kb) => serialize_key(kb, &mut output),
                Entry::Action(act) => serialize_act(act, &mut output),
                Entry::Script(scr) => serialize_scr(scr, &mut output),
            }
        }
        output
    }
}

fn serialize_command_id(cmd: &CommandId) -> String {
    match cmd {
        CommandId::Native(n) => n.to_string(),
        CommandId::Named(s) => s.clone(),
    }
}

fn serialize_key(kb: &KeyBinding, out: &mut String) {
    let section_raw = kb.section.to_raw();
    out.push_str(&format!(
        "KEY {} {} {} {}\r\n",
        kb.modifier,
        kb.key,
        serialize_command_id(&kb.command),
        section_raw,
    ));

    // If global, emit the paired global-flag line.
    if let Some(scope) = &kb.global {
        let global_section = match kb.section {
            Section::MainAlt => 103,
            _ => 102,
        };
        out.push_str(&format!(
            "KEY {} {} {} {}\r\n",
            kb.modifier,
            kb.key,
            scope.to_raw(),
            global_section,
        ));
    }
}

fn serialize_act(act: &CustomAction, out: &mut String) {
    let actions_str: Vec<String> = act.actions.iter().map(serialize_command_id).collect();
    out.push_str(&format!(
        "ACT {} {} \"{}\" \"{}\" {}\r\n",
        act.flags.bits(),
        act.section.to_raw(),
        act.command_id,
        act.description,
        actions_str.join(" "),
    ));
}

fn serialize_scr(scr: &ScriptBinding, out: &mut String) {
    out.push_str(&format!(
        "SCR {} {} {} \"{}\" {}\r\n",
        scr.flags.bits(),
        scr.section.to_raw(),
        scr.command_id,
        scr.description,
        scr.script_file,
    ));
}
