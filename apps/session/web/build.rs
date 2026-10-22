//! Compile `docs/guides/session/*.md` into the site.
//!
//! The guide is a **vault**: markdown notes with frontmatter and
//! `[[wikilink]]` cross-references. `ssg-build` reads them, renders them
//! to HTML on the host, and codegens the page table `src/guide.rs`
//! includes — so the site ships finished pages rather than a markdown
//! parser and a pile of source.
//!
//! Reading outside the crate is what a build script is for.
//! `include_str!` across that boundary would be invisible to cargo and
//! would fail at compile time rather than resolution time; the
//! `cargo:rerun-if-changed` lines `emit` prints are what make editing a
//! guide page rebuild the site.

fn main() {
    ssg_build::Vault::at("../../../docs/guides/session")
        .link_base("/guide")
        .emit();
    tailwind_placeholder();
}

/// `assets/tailwind.css` is generated (`just web-tailwind`) and gitignored,
/// but `asset!("/assets/tailwind.css")` fails to compile when it's absent —
/// which took `cargo check --workspace` down on any fresh checkout. Write an
/// empty stand-in so the crate type-checks; `just web`/`web-build` run
/// `web-tailwind` first and overwrite it with the real sheet.
fn tailwind_placeholder() {
    let path = std::path::Path::new("assets/tailwind.css");
    println!("cargo:rerun-if-changed=assets/tailwind.css");
    if path.exists() {
        return;
    }
    let _ = std::fs::create_dir_all("assets");
    match std::fs::write(
        path,
        "/* placeholder: run `just web-tailwind` for the real sheet */\n",
    ) {
        Ok(()) => println!(
            "cargo:warning=apps/session/web/assets/tailwind.css was missing; wrote an empty placeholder (run `just web-tailwind`)"
        ),
        Err(e) => println!("cargo:warning=could not write tailwind.css placeholder: {e}"),
    }
}
