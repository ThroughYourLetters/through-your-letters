use super::{
    helpers::{TestApp, read_json, send, spawn_app, spawn_app_with_config},
    test_pagination::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::Connection;
use sqlx::PgPool;
use std::time::{Duration, Instant};
use uuid::Uuid;

const DENSITY: &str = "/api/v1/geo/density?west=77&south=12&east=78&north=13.5&zoom=12";
const CACHED_PATHS: [&str; 3] = ["/api/v1/letterings", DENSITY, "/api/v1/geo/summary"];

async fn response(app: &TestApp, path: &str) -> axum::response::Response {
    send(
        &app.app,
        Request::builder().uri(path).body(Body::empty()).unwrap(),
    )
    .await
}
async fn get(app: &TestApp, path: &str) -> Value {
    let result = response(app, path).await;
    assert_eq!(result.status(), StatusCode::OK, "{path}");
    read_json(result).await
}
async fn admin(
    app: &TestApp,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> axum::response::Response {
    send(
        &app.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
            .unwrap(),
    )
    .await
}
fn mapped(value: &Value) -> i64 {
    value["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|cell| cell["count"].as_i64().unwrap())
        .sum()
}

#[sqlx::test]
async fn committed_moderation_and_policy_changes_refresh_other_replicas(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    sqlx::query("UPDATE letterings SET location=ST_SetSRID(ST_MakePoint(77.6,12.9),4326)::geography,location_source='postal_code' WHERE id<>$1")
        .bind(f.ids[0]).execute(&pool).await.unwrap();
    let other = spawn_app(&pool).await;
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 8);
    assert_eq!(mapped(&get(&f.app, DENSITY).await), 7);
    assert_eq!(get(&f.app, CACHED_PATHS[2]).await["unmapped_count"], 1);

    for (id, total, expected_mapped) in [(f.ids[0], 7, 7), (f.ids[1], 6, 6)] {
        let result = admin(
            &other,
            &f.admin,
            "POST",
            &format!("/api/v1/admin/letterings/{id}/reject"),
            Some(json!({"reason":"Review complete"})),
        )
        .await;
        assert_eq!(result.status(), StatusCode::NO_CONTENT);
        let gallery = get(&f.app, CACHED_PATHS[0]).await;
        assert_eq!(gallery["total"], total);
        assert!(
            gallery["letterings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["id"] != id.to_string())
        );
        assert_eq!(mapped(&get(&f.app, DENSITY).await), expected_mapped);
        assert_eq!(get(&f.app, CACHED_PATHS[2]).await["unmapped_count"], 0);
    }
    let country: String = sqlx::query_scalar(
        "SELECT c.country_code FROM cities c JOIN letterings l ON l.city_id=c.id WHERE l.id=$1",
    )
    .bind(f.ids[2])
    .fetch_one(&pool)
    .await
    .unwrap();
    let result = admin(
        &other,
        &f.admin,
        "PUT",
        &format!("/api/v1/admin/region-policies/{country}"),
        Some(json!({
            "uploads_enabled":true,"comments_enabled":true,"discoverability_enabled":false
        })),
    )
    .await;
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 0);
    assert_eq!(mapped(&get(&f.app, DENSITY).await), 0);
    assert_eq!(get(&f.app, CACHED_PATHS[2]).await["unmapped_count"], 0);
}

#[sqlx::test]
async fn cache_versions_follow_commits_and_rollbacks(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 8);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(f.ids[0])
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 8);
    tx.rollback().await.unwrap();
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 8);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(f.ids[0])
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 7);

    let mut earlier = pool.begin().await.unwrap();
    let mut later = pool.begin().await.unwrap();
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(f.ids[1])
        .execute(&mut *earlier)
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
            .bind(f.ids[2])
            .execute(&mut *later),
    )
    .await
    .unwrap()
    .unwrap();
    later.commit().await.unwrap();
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 6);
    earlier.commit().await.unwrap();
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 5);
}

#[sqlx::test]
async fn missing_freshness_state_fails_closed_and_a_new_epoch_cannot_reuse_old_data(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    for path in CACHED_PATHS {
        get(&f.app, path).await;
    }
    let epoch: Uuid = sqlx::query_scalar("SELECT epoch FROM public_cache_revision WHERE singleton")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM public_cache_revision")
        .execute(&pool)
        .await
        .unwrap();
    for path in CACHED_PATHS {
        let result = response(&f.app, path).await;
        assert_eq!(result.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(result.headers()["cache-control"], "no-store");
        let body: Value = read_json(result).await;
        assert_eq!(body, json!({"error":"External service unavailable"}));
    }
    let result = response(&f.app, "/health").await;
    assert_eq!(result.status(), StatusCode::SERVICE_UNAVAILABLE);
    let health: Value = read_json(result).await;
    assert_eq!(health["status"], "unhealthy");
    assert_eq!(health["database"], "up");
    assert_eq!(health["cache_consistency"], "down");
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(f.ids[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO public_cache_revision(singleton) VALUES(true)")
        .execute(&pool)
        .await
        .unwrap();
    let new_epoch: Uuid =
        sqlx::query_scalar("SELECT epoch FROM public_cache_revision WHERE singleton")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(new_epoch, epoch);
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 7);
    assert_eq!(get(&f.app, "/health").await["cache_consistency"], "up");
}

#[sqlx::test]
async fn busy_database_cannot_leave_cached_requests_or_readiness_waiting(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let replica = spawn_app_with_config(&pool, |config| {
        config.database_max_connections = 2;
        config.limits.cache.freshness_timeout_ms = 100;
    })
    .await;
    get(&replica, CACHED_PATHS[0]).await;
    let mut held = Vec::new();
    for _ in 0..replica.db.options().get_max_connections() {
        held.push(replica.db.acquire().await.unwrap());
    }
    let started = Instant::now();
    assert_eq!(
        response(&replica, CACHED_PATHS[0]).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        response(&replica, "/health").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    drop(held);
    assert_eq!(get(&replica, "/health").await["status"], "healthy");
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 8);
}

#[sqlx::test]
async fn retiring_a_writer_cannot_revive_an_older_cached_response(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 8);
    let epoch: Uuid = sqlx::query_scalar("SELECT epoch FROM public_cache_revision WHERE singleton")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut writer = pool.acquire().await.unwrap().detach();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut writer)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET status='PENDING' WHERE id=$1")
        .bind(f.ids[0])
        .execute(&mut writer)
        .await
        .unwrap();
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 7);
    writer.close().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1)",
        )
        .bind(pid)
        .fetch_one(&pool)
        .await
        .unwrap()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let removed = api::workers::public_cache_cleanup::cleanup_once(
        &pool,
        std::num::NonZeroU32::new(1000).unwrap(),
    )
    .await
    .unwrap();
    assert!(removed >= 1);
    let new_epoch: Uuid =
        sqlx::query_scalar("SELECT epoch FROM public_cache_revision WHERE singleton")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(epoch, new_epoch);
    assert_eq!(get(&f.app, CACHED_PATHS[0]).await["total"], 7);
}

#[sqlx::test]
async fn cleanup_bounds_its_batch_and_preserves_live_writers(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let mut active = pool.acquire().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *active)
        .await
        .unwrap();
    sqlx::query("UPDATE letterings SET description='Still active' WHERE id=$1")
        .bind(f.ids[0])
        .execute(&mut *active)
        .await
        .unwrap();
    for pid in (i32::MAX - 2)..=i32::MAX {
        sqlx::query("INSERT INTO public_cache_writers(backend_pid,revision) VALUES($1,1)")
            .bind(pid)
            .execute(&pool)
            .await
            .unwrap();
    }
    let removed = api::workers::public_cache_cleanup::cleanup_once(
        &pool,
        std::num::NonZeroU32::new(1).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(removed, 1);
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM public_cache_writers WHERE backend_pid=$1)"
        )
        .bind(pid)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM public_cache_writers WHERE backend_pid >= $1"
        )
        .bind(i32::MAX - 2)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
}
