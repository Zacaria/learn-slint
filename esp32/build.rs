fn main() {
    embuild::espidf::sysenv::output();
    slint_build::compile_with_config(
        "appwindow.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
    )
    .unwrap();
}
