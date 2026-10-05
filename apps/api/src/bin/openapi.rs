use api::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() {
    let spec = serde_json::to_string_pretty(&ApiDoc::openapi()).expect("serialize openapi spec");
    println!("{spec}");
}
