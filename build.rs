fn main() {
    println!("cargo:rerun-if-changed=buzzprint.rc");
    println!("cargo:rerun-if-changed=resources/buzzprint.ico");
    println!("cargo:rerun-if-changed=resources/buzzprint.manifest");
    let numbers = format!(
        "BUZZPRINT_VERSION_NUMBERS={},{},{},0",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    );
    let text = format!("BUZZPRINT_VERSION_TEXT=\"{}\"", env!("CARGO_PKG_VERSION"));
    embed_resource::compile("buzzprint.rc", [numbers, text])
        .manifest_required()
        .unwrap();
}
