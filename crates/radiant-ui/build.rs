fn main() {
    // Translations (README section 7): the gettext files lang/<code>/LC_MESSAGES/radiant-ui.po are bundled
    // into the binary, so `@tr(...)` texts in main.slint can switch language at runtime without extra files.
    // The domain `radiant-ui` is the package name of this crate.
    println!("cargo:rerun-if-changed=lang");
    // By default Slint looks a text up under the name of its component (msgctxt). The .po files here have no
    // context (the key is just the English text, so the same file also serves the Rust side), hence `None`.
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
}
