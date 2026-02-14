export type DomainError =
  | { type: "NotFound" }
  | { type: "ValidationError"; message: string }
  | { type: "InfrastructureError"; message: string }
  | { type: "RateLimitExceeded" }
  | { type: "Unauthorized" };
