use std::{env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../messages");
    let mut generated =
        String::from("pub fn message(key:&str,locale:&str)->&'static str {match (key,locale) {\n");
    for locale in ["en", "zh-CN"] {
        let path = root.join(format!("{locale}.json"));
        println!("cargo:rerun-if-changed={}", path.display());
        let values: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        for (key, value) in values.as_object().unwrap() {
            if key.starts_with("native_") {
                generated.push_str(&format!(
                    "({key:?},{locale:?})=>{:?},\n",
                    value.as_str().unwrap()
                ));
            }
        }
    }
    generated.push_str("(_,_)=>\"VigilShell\",}}\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("native_messages.rs"),
        generated,
    )
    .unwrap();
}
