use crate::services::attachment_service::AttachmentService;
use std::sync::Arc;
use tauri::http::{Response, StatusCode};

pub fn handle_attachment_protocol(
    request: &tauri::http::Request<Vec<u8>>,
    attachment_service: Arc<AttachmentService>,
) -> Response<Vec<u8>> {
    let uri = request.uri().to_string();

    // URI format: suijian-attachment://<uuid> or suijian-attachment://localhost/<uuid>
    let raw_uuid = uri
        .trim_start_matches("suijian-attachment://")
        .trim_start_matches("localhost/")
        .trim_matches('/');

    let uuid_str = raw_uuid.split(['?', '#']).next().unwrap_or("").trim();

    if uuid_str.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(b"Missing attachment UUID".to_vec())
            .unwrap();
    }

    match attachment_service.resolve_attachment_file(uuid_str) {
        Ok(path) => match std::fs::read(&path) {
            Ok(bytes) => {
                let mime = match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    _ => "application/octet-stream",
                };

                Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime)
                    .header("Cache-Control", "public, max-age=31536000, immutable")
                    .body(bytes)
                    .unwrap()
            }
            Err(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(b"File read error".to_vec())
                .unwrap(),
        },
        Err(err) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(err.into_bytes())
            .unwrap(),
    }
}
