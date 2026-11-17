use super::{
    helpers::{read_json, send},
    test_pagination::Fixture,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

fn request(method: &str, path: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
async fn snapshot(pool: &PgPool, id: Uuid) -> (Value, i64, i64, i64) {
    let row: Value = sqlx::query_scalar("SELECT to_jsonb(l) FROM letterings l WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM admin_audit_logs),(SELECT COUNT(*) FROM notifications),(SELECT COUNT(*) FROM lettering_status_history)").fetch_one(pool).await.unwrap();
    (row, counts.0, counts.1, counts.2)
}

#[sqlx::test]
async fn failed_audits_or_notifications_leave_decisions_and_history_unchanged(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    for table in ["admin_audit_logs", "notifications"] {
        sqlx::raw_sql(&format!("CREATE FUNCTION fail_moderation_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture failure'; END $$; CREATE TRIGGER fail_moderation_write BEFORE INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION fail_moderation_write()"))
            .execute(&pool).await.unwrap();
        for action in ["approve", "reject", "clear-reports"] {
            let before = snapshot(&pool, f.ids[0]).await;
            let response = send(
                &f.app.app,
                request(
                    "POST",
                    &format!("/api/v1/admin/letterings/{}/{action}", f.ids[0]),
                    &f.admin,
                    json!({"reason":"Review"}),
                ),
            )
            .await;
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{table}: {action}"
            );
            assert_eq!(
                snapshot(&pool, f.ids[0]).await,
                before,
                "A failed write must roll back the entire decision"
            );
        }
        sqlx::raw_sql(&format!(
            "DROP TRIGGER fail_moderation_write ON {table}; DROP FUNCTION fail_moderation_write()"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
}

#[sqlx::test]
async fn bulk_failures_report_every_item_and_leave_failed_decisions_unchanged(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let missing = Uuid::now_v7();
    for (table, column, operation, good) in [
        ("admin_audit_logs", "lettering_id", "INSERT", f.ids[1]),
        ("letterings", "id", "UPDATE", f.ids[2]),
    ] {
        sqlx::raw_sql(&format!("CREATE FUNCTION fail_one_decision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.{column}='{}'::uuid THEN RAISE EXCEPTION 'fixture failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_one_decision BEFORE {operation} ON {table} FOR EACH ROW EXECUTE FUNCTION fail_one_decision()",f.ids[0]))
            .execute(&pool).await.unwrap();
        let before = snapshot(&pool, f.ids[0]).await.0;
        let response = send(
            &f.app.app,
            request(
                "POST",
                "/api/v1/admin/letterings/bulk",
                &f.admin,
                json!({"action":"reject","ids":[good,f.ids[0],missing],"reason":"Review"}),
            ),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK, "{table}");
        let result: Value = read_json(response).await;
        assert_eq!(result["requested"], 3);
        assert_eq!(result["processed"], 1);
        assert_eq!(result["failed"], 2);
        let failures = result["failed_items"].as_array().unwrap();
        assert!(failures.iter().any(|v| v["id"] == f.ids[0].to_string()));
        assert!(failures.iter().any(|v| v["id"] == missing.to_string()));
        assert_eq!(snapshot(&pool, f.ids[0]).await.0, before);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM letterings WHERE id=$1")
                .bind(good)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "REJECTED"
        );
        let audits:i64=sqlx::query_scalar("SELECT COUNT(*) FROM admin_audit_logs WHERE action='BULK_REJECT_LETTERING' AND lettering_id=$1").bind(good).fetch_one(&pool).await.unwrap();
        assert_eq!(audits, 1);
        sqlx::raw_sql(&format!(
            "DROP TRIGGER fail_one_decision ON {table}; DROP FUNCTION fail_one_decision()"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
}

#[sqlx::test]
async fn account_deletion_waits_for_a_moderation_transaction_without_deadlocking(pool: PgPool) {
    let f = Fixture::new(&pool).await;
    let hash = bcrypt::hash("SecurePassword123!", bcrypt::DEFAULT_COST).unwrap();
    sqlx::query(
        "UPDATE users SET password_hash=$2 WHERE id=(SELECT user_id FROM letterings WHERE id=$1)",
    )
    .bind(f.ids[0])
    .bind(hash)
    .execute(&pool)
    .await
    .unwrap();
    let gate = i64::from_be_bytes(Uuid::now_v7().as_bytes()[8..].try_into().unwrap());
    sqlx::raw_sql(&format!("CREATE FUNCTION pause_moderation_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock({gate}); RETURN NEW; END $$; CREATE TRIGGER pause_moderation_audit BEFORE INSERT ON admin_audit_logs FOR EACH ROW EXECUTE FUNCTION pause_moderation_audit()"))
        .execute(&pool).await.unwrap();
    let mut blocker = pool.begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(gate)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let router = f.app.app.clone();
    let approval = request(
        "POST",
        &format!("/api/v1/admin/letterings/{}/approve", f.ids[0]),
        &f.admin,
        Value::Null,
    );
    let approval = tokio::spawn(async move { send(&router, approval).await });
    let moderation_pid=tokio::time::timeout(Duration::from_secs(3),async {
        loop {
            let waiting:Option<i32>=sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid))").bind(blocker_pid).fetch_optional(&pool).await.unwrap();
            if let Some(pid)=waiting {break pid;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("Moderation did not reach its audit write");
    let router = f.app.app.clone();
    let deletion = request(
        "DELETE",
        "/api/v1/me/account",
        &f.token,
        json!({"password":"SecurePassword123!"}),
    );
    let deletion = tokio::spawn(async move { send(&router, deletion).await });
    tokio::time::timeout(Duration::from_secs(3),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND $1=ANY(pg_blocking_pids(pid)))").bind(moderation_pid).fetch_one(&pool).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("Deletion did not wait for the active decision");
    blocker.rollback().await.unwrap();
    let (approval, deletion) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(approval, deletion)
    })
    .await
    .unwrap();
    assert_eq!(approval.unwrap().status(), StatusCode::NO_CONTENT);
    assert_eq!(deletion.unwrap().status(), StatusCode::NO_CONTENT);
    let row: (Option<Uuid>, String, String) =
        sqlx::query_as("SELECT user_id,contributor_tag,status FROM letterings WHERE id=$1")
            .bind(f.ids[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (None, "Anonymous".into(), "APPROVED".into()));
}
