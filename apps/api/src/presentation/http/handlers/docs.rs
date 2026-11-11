use axum::Json;
use serde_json::{Map, Value, json};

pub async fn api_docs() -> Json<Value> {
    let paths: Map<String, Value> = [
        ("/api/v1/admin/metrics", json!({"get": {"summary": "Admin: completed request counts, status classes, and latency for this instance"}})),
        ("/health", json!({"get": {"summary": "Health check"}})),
        ("/api/v1/letterings", json!({"get": {"summary": "List letterings"}})),
        ("/api/v1/letterings/search", json!({"get": {"summary": "Search letterings (supports lang query for locale-aware search)"}})),
        (
            "/api/v1/letterings/upload",
            json!({
                "post": {
                    "summary": "Upload lettering",
                    "parameters": [
                        {
                            "name": "Idempotency-Key",
                            "in": "header",
                            "required": false,
                            "schema": {
                                "type": "string",
                                "format": "uuid"
                            },
                            "description": "Reuse a nonzero UUID for retries of the same normalized image and metadata. Keys belong to the current account, or to the anonymous upload scope. Changed content or a deleted upload returns 409."
                        }
                    ]
                }
            }),
        ),
        (
            "/api/v1/letterings/{id}",
            json!({
                "get": {
                    "summary": "Get lettering by id"
                },
                "delete": {
                    "summary": "Delete lettering by id"
                }
            }),
        ),
        (
            "/api/v1/letterings/{id}/comments",
            json!({
                "get": {
                    "summary": "List comments for lettering"
                },
                "post": {
                    "summary": "Add comment for lettering (authenticated user)"
                }
            }),
        ),
        ("/api/v1/letterings/{id}/like", json!({"post": {"summary": "Toggle like"}})),
        ("/api/v1/letterings/{id}/similar", json!({"get": {"summary": "Get similar letterings"}})),
        ("/api/v1/letterings/{id}/download", json!({"get": {"summary": "Redirect to original image"}})),
        (
            "/api/v1/letterings/{id}/revisits",
            json!({
                "get": {
                    "summary": "Get revisit links for lettering"
                },
                "post": {
                    "summary": "Create revisit link for lettering"
                }
            }),
        ),
        ("/api/v1/geo/density", json!({"get": {"summary": "Get bounded density cells for map bounds and zoom"}})),
        ("/api/v1/geo/summary", json!({"get": {"summary": "Count approved uploads without usable map locations"}})),
        ("/api/v1/geo/photos", json!({"get": {"summary": "Page through uploads within map bounds"}})),
        ("/api/v1/geo/markers", json!({"get": {"summary": "Get map markers"}})),
        ("/api/v1/geo/nearby", json!({"get": {"summary": "Get nearby markers"}})),
        ("/api/v1/geo/coverage", json!({"get": {"summary": "Get pin-code coverage data"}})),
        ("/api/v1/cities", json!({"get": {"summary": "List cities (supports search/discovery)"}})),
        ("/api/v1/cities/{id}", json!({"get": {"summary": "Get city detail"}})),
        ("/api/v1/cities/{id}/stats", json!({"get": {"summary": "Get city neighborhood stats"}})),
        ("/api/v1/admin/cities/discover", json!({"post": {"summary": "Admin: discover cities using Nominatim + Wikipedia enrichment"}})),
        ("/api/v1/admin/cities/bootstrap-capitals", json!({"post": {"summary": "Admin: bootstrap global capitals using REST Countries + Wikipedia enrichment"}})),
        ("/api/v1/docs", json!({"get": {"summary": "OpenAPI spec"}})),
        ("/api/v1/auth/register", json!({"post": {"summary": "Register user account"}})),
        ("/api/v1/auth/login", json!({"post": {"summary": "Login user account"}})),
        ("/api/v1/auth/me", json!({"get": {"summary": "Get current user profile"}})),
        ("/api/v1/me/letterings", json!({"get": {"summary": "List current user's uploads"}})),
        ("/api/v1/me/notifications", json!({"get": {"summary": "List current user's notifications"}})),
        ("/api/v1/me/collections", json!({"get": {"summary": "List current user's public and private collections"}})),
        ("/api/v1/admin/comments", json!({"get": {"summary": "Admin: list comments for moderation (status/search/review filters, score sorting)"}})),
        ("/api/v1/admin/comments/{id}/hide", json!({"post": {"summary": "Admin: hide comment and resolve review flag"}})),
        ("/api/v1/admin/comments/{id}/restore", json!({"post": {"summary": "Admin: restore comment"}})),
        ("/api/v1/admin/comments/{id}", json!({"delete": {"summary": "Admin: delete comment"}})),
        ("/api/v1/admin/region-policies", json!({"get": {"summary": "Admin: list region policies"}})),
        ("/api/v1/admin/region-policies/{country_code}", json!({"put": {"summary": "Admin: upsert region policy for a country code"}})),
        ("/ws/feed", json!({"get": {"summary": "WebSocket live feed"}})),
    ].into_iter().map(|(path, methods)| (path.to_owned(), methods)).collect();
    Json(json!({
        "openapi": "3.0.0",
        "info": {"title": "Through Your Letters API", "version": "1.0.0"},
        "paths": paths
    }))
}
