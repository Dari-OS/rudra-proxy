use axum::http::header;
use axum::response::{Html, IntoResponse, Response};

const OPENAPI_SPEC: &str = include_str!("../../openapi.json");

/// Serves the interactive Scalar API documentation UI.
pub async fn scalar_docs_handler() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html>
  <head>
    <title>Rudra Proxy — API Reference</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <style>
      body {
        margin: 0;
        padding: 0;
      }
    </style>
  </head>
  <body>
    <script
      id="api-reference"
      data-url="/openapi.json"
      data-configuration='{"theme":"purple","layout":"modern"}'></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#,
    )
}

/// Serves the OpenAPI 3.1 schema definition for Rudra Proxy.
pub async fn openapi_spec_handler() -> Response {
    ([(header::CONTENT_TYPE, "application/json")], OPENAPI_SPEC).into_response()
}
