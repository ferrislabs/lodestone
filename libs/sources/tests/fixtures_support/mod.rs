#![allow(dead_code)]

use std::{collections::BTreeSet, fs, path::PathBuf};

use serde_json::Value;

pub struct HttpFixture {
    pub file: String,
    pub product: String,
    pub path: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/iam-ferriskey/http")
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}

pub fn load(file: &str) -> HttpFixture {
    let raw = fs::read_to_string(fixture_root().join(file)).unwrap();
    let value: Value = serde_json::from_str(&raw).unwrap();
    let response = &value["response"];
    let headers = response["headers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().unwrap().to_owned(),
                pair[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    HttpFixture {
        file: file.to_owned(),
        product: text(&value, "product"),
        path: text(&value["request"], "path"),
        status: u16::try_from(response["status"].as_u64().unwrap()).unwrap(),
        headers,
        body: text(response, "body"),
    }
}

pub fn indexed_files() -> Vec<String> {
    let raw = fs::read_to_string(fixture_root().join("index.json")).unwrap();
    let index: Value = serde_json::from_str(&raw).unwrap();
    index["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| text(entry, "file"))
        .collect()
}

pub fn all() -> Vec<HttpFixture> {
    indexed_files().iter().map(|file| load(file)).collect()
}

pub fn files_on_disk() -> BTreeSet<String> {
    let root = fixture_root();
    let mut files = BTreeSet::new();
    for product in fs::read_dir(&root).unwrap() {
        let product = product.unwrap().path();
        if !product.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&product).unwrap() {
            let path = entry.unwrap().path();
            files.insert(
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    files
}
