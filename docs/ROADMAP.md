# Through Your Letters Roadmap

Status date: February 12, 2026

This document tracks implementation status against product and platform goals.

## Product Vision

- Build the most trusted, community-powered archive of public lettering worldwide.
- Make contribution easy, discovery rich, and moderation accountable.
- Preserve cultural context while keeping safety, quality, and ownership explicit.

## Current Delivery Status

### Core Platform: Delivered

- Authenticated users with owner-restricted deletion.
- Upload, discover, map, contributor profiles, comments, likes, report flow.
- Admin moderation for letterings and comments.
- Region policy controls (uploads/comments/discoverability/moderation level).
- City discovery/bootstrap operations and region governance controls.
- Request traceability (`x-request-id`) and admin audit logging.
- User workspace for upload lifecycle, metadata edits, and timeline history.

### Reliability and Quality Baseline: Delivered

- Integration tests for upload, gallery, lifecycle updates, and smoke user journeys.
- End-to-end API smoke journey for auth, upload, discovery, comment, admin moderation.
- CI checks for migration drift and SQL contract validation.
- Backend repository stub removal (`LetteringRepository::update` now implemented).

### Contributor Lifecycle Completion: Delivered

- Upload status timeline and status history persisted in DB.
- Moderation feedback visible with reason and action timestamp.
- Metadata edit flow with validation and metadata change history.
- User notifications for moderation and comment events.

### Admin Excellence: Delivered

- Pagination for moderation queues, comments queue, audit logs, region policy history.
- Bulk actions for letterings and comments with per-item failure reporting.
- Saved admin filter presets in UI.
- Policy change history view linked to audit events.

## Definition of Feature-Done (V1)

V1 is considered feature-done when these conditions are true:

- Critical user journeys are implemented and test-covered:
  auth, upload, discover, comment, moderation, ownership updates/deletes.
- Moderation and region governance are explicit and auditable.
- Contributor lifecycle feedback is visible and editable by owners.
- CI enforces schema/migration consistency and query contract checks.
- Operational docs are present for local, deployment, and OCI setup paths.

Current status: V1 feature-done baseline is achieved.

## Post-V1 Backlog (Enhancements, Not Blockers)

### Global Relevance

- Expand full UI localization coverage beyond current targeted strings.
- Improve cross-lingual ranking with language-specific relevance tuning.
- Add transliteration-aware query behavior where practical.

### Discoverability Depth

- Add richer map filters (script/style/date/confidence).
- Add contributor reputation/trust scoring.
- Improve semantic related-lettering ranking.

### Research and Institutional Features

- Citation exports and researcher metadata packs.
- Privacy-safe public dataset export workflows.
- API key management and rate-tier controls.

### Operations at Scale

- Regional data residency controls and routing strategies.
- Appeals workflow and reviewer QA sampling.
- SLO-targeted alerting and incident drill automation.

## Scope Guardrails

- No hidden moderation or policy behavior.
- No silent moderation actions without audit trail.
- No schema changes without migration and rollback notes.
- No placeholder stubs in production paths or automated tests.
