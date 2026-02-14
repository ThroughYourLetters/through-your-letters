use axum::Json;

pub async fn api_docs() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "openapi": "3.0.0",
        "info": {
            "title": "Through Your Letters API",
            "version": "1.0.0"
        },
        "paths": {
            "/health": { "get": { "summary": "Health check" } },
            "/api/v1/letterings": { "get": { "summary": "List letterings" } },
            "/api/v1/letterings/search": { "get": { "summary": "Search letterings (supports lang query for locale-aware search)" } },
            "/api/v1/letterings/upload": { "post": { "summary": "Upload lettering" } },
            "/api/v1/letterings/{id}": {
                "get": { "summary": "Get lettering by id" },
                "delete": { "summary": "Delete lettering by id" }
            },
            "/api/v1/letterings/{id}/comments": {
                "get": { "summary": "List comments for lettering" },
                "post": { "summary": "Add comment for lettering (authenticated user)" }
            },
            "/api/v1/letterings/{id}/like": { "post": { "summary": "Toggle like" } },
            "/api/v1/letterings/{id}/similar": { "get": { "summary": "Get similar letterings" } },
            "/api/v1/letterings/{id}/download": { "get": { "summary": "Redirect to original image" } },
            "/api/v1/letterings/{id}/revisits": {
                "get": { "summary": "Get revisit links for lettering" },
                "post": { "summary": "Create revisit link for lettering" }
            },
            "/api/v1/geo/markers": { "get": { "summary": "Get map markers" } },
            "/api/v1/geo/nearby": { "get": { "summary": "Get nearby markers" } },
            "/api/v1/geo/coverage": { "get": { "summary": "Get pin-code coverage data" } },
            "/api/v1/cities": { "get": { "summary": "List cities (supports search/discovery)" } },
            "/api/v1/cities/{id}": { "get": { "summary": "Get city detail" } },
            "/api/v1/cities/{id}/stats": { "get": { "summary": "Get city neighborhood stats" } },
            "/api/v1/admin/cities/discover": { "post": { "summary": "Admin: discover cities using Nominatim + Wikipedia enrichment" } },
            "/api/v1/admin/cities/bootstrap-capitals": { "post": { "summary": "Admin: bootstrap global capitals using REST Countries + Wikipedia enrichment" } },
            "/api/v1/docs": { "get": { "summary": "OpenAPI spec" } },
            "/api/v1/auth/register": { "post": { "summary": "Register user account" } },
            "/api/v1/auth/login": { "post": { "summary": "Login user account" } },
            "/api/v1/auth/me": { "get": { "summary": "Get current user profile" } },
            "/api/v1/me/letterings": { "get": { "summary": "List current user's uploads" } },
            "/api/v1/me/notifications": { "get": { "summary": "List current user's notifications" } },
            "/api/v1/admin/comments": { "get": { "summary": "Admin: list comments for moderation (status/search/review filters, score sorting)" } },
            "/api/v1/admin/comments/{id}/hide": { "post": { "summary": "Admin: hide comment and resolve review flag" } },
            "/api/v1/admin/comments/{id}/restore": { "post": { "summary": "Admin: restore comment" } },
            "/api/v1/admin/comments/{id}": { "delete": { "summary": "Admin: delete comment" } },
            "/api/v1/admin/region-policies": { "get": { "summary": "Admin: list region policies" } },
            "/api/v1/admin/region-policies/{country_code}": { "put": { "summary": "Admin: upsert region policy for a country code" } },
            "/ws/feed": { "get": { "summary": "WebSocket live feed" } }
        }
    }))
}
