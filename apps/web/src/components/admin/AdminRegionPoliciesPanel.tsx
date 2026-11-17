import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import { api, type RegionPolicyItem } from "../../lib/api";
import {
  useAdminAction,
  useAdminResource,
  type AdminSessionProps,
} from "./useAdminOperations";

export default function AdminRegionPoliciesPanel(session: AdminSessionProps) {
  const [countryCode, setCountryCode] = useState("");
  const [filter, setFilter] = useState({ countryCode: "" });
  const [items, setItems] = useState<RegionPolicyItem[]>([]);
  const [offset, setOffset] = useState(0);
  const [validationError, setValidationError] = useState("");
  const [notice, setNotice] = useState("");
  const formRef = useRef<HTMLFormElement>(null);
  const countryRef = useRef<HTMLInputElement>(null);
  const action = useAdminAction(session);
  const load = useCallback(
    (signal: AbortSignal) =>
      api.adminGetRegionPolicies({
        countryCode: filter.countryCode || undefined,
        limit: 500,
        offset: 0,
        signal,
      }),
    [filter],
  );
  const policies = useAdminResource(session, load);
  const loadHistory = useCallback(
    (signal: AbortSignal) =>
      api.adminGetAuditLogs({
        action: "UPSERT_REGION_POLICY",
        countryCode: filter.countryCode || undefined,
        limit: 10,
        offset,
        signal,
      }),
    [filter, offset],
  );
  const history = useAdminResource(session, loadHistory);
  useEffect(() => {
    setItems(policies.data?.items ?? []);
  }, [policies.data]);
  function applyFilter(event: FormEvent) {
    event.preventDefault();
    setValidationError("");
    setOffset(0);
    setFilter({ countryCode });
  }
  function updateDraft(code: string, changes: Partial<RegionPolicyItem>) {
    setItems((previous) =>
      previous.map((item) =>
        item.country_code === code ? { ...item, ...changes } : item,
      ),
    );
  }
  async function save(policy: RegionPolicyItem) {
    setNotice("");
    const result = await action.run(() =>
      api.adminUpsertRegionPolicy(policy.country_code, {
        uploads_enabled: policy.uploads_enabled,
        comments_enabled: policy.comments_enabled,
        discoverability_enabled: policy.discoverability_enabled,
        auto_moderation_level: policy.auto_moderation_level,
      }),
    );
    if (result.ok) {
      updateDraft(result.value.country_code, result.value);
      setNotice(`Policy updated for ${result.value.country_code}.`);
      history.reload();
    }
  }
  async function createDefaults() {
    if (action.busy || !formRef.current?.reportValidity()) return;
    if (!/^[A-Z]{2}$/.test(countryCode)) {
      setValidationError("Enter a two-letter country code.");
      countryRef.current?.focus();
      return;
    }
    if (
      !window.confirm(
        `Apply default settings for ${countryCode}? Existing settings for this country will be replaced.`,
      )
    )
      return;
    setValidationError("");
    setNotice("");
    const result = await action.run(() =>
      api.adminUpsertRegionPolicy(countryCode, {
        uploads_enabled: true,
        comments_enabled: true,
        discoverability_enabled: true,
        auto_moderation_level: "standard",
      }),
    );
    if (result.ok) {
      setOffset(0);
      setFilter({ countryCode });
      setNotice(`Default settings applied for ${countryCode}.`);
    }
  }
  const total = history.data?.total ?? 0;
  return (
    <section aria-labelledby="region-title" className="space-y-6">
      <form
        ref={formRef}
        onSubmit={applyFilter}
        aria-labelledby="region-title"
        className="card p-5 md:p-6 space-y-4"
      >
        <h2 id="region-title" className="text-lg font-semibold">
          Region Policy Controls
        </h2>
        <label className="block text-sm font-medium">
          Country code (optional filter)
          <input
            ref={countryRef}
            value={countryCode}
            disabled={action.busy}
            onChange={(event) => {
              setCountryCode(event.target.value.toUpperCase());
              setValidationError("");
            }}
            placeholder="Country code (US, IN)"
            maxLength={2}
            pattern="[A-Za-z]{2}"
            title="Use a two-letter country code, such as US or IN."
            aria-invalid={!!validationError}
            aria-describedby={
              validationError ? "region-country-error" : undefined
            }
            className="field mt-2 !text-base"
          />
        </label>
        {validationError && (
          <p
            id="region-country-error"
            role="alert"
            className="text-sm text-red-700"
          >
            {validationError}
          </p>
        )}
        <div className="flex flex-wrap gap-3">
          <button type="submit" disabled={action.busy} className="btn-dark">
            Load
          </button>
          <button
            type="button"
            disabled={action.busy}
            onClick={() => void createDefaults()}
            className="btn-secondary"
          >
            Apply default policy
          </button>
        </div>
        <p className="text-sm text-muted">
          Default settings enable uploads, comments, and discovery with standard
          moderation. Existing settings for the entered country will be
          replaced.
        </p>
      </form>
      {action.error && (
        <p role="alert" className="text-sm text-red-700 break-words">
          {action.error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      {policies.loading ? (
        <p role="status">Loading policies…</p>
      ) : policies.error ? (
        <p role="alert">
          {policies.error}{" "}
          <button className="underline" onClick={policies.reload}>
            Retry policies
          </button>
        </p>
      ) : items.length === 0 ? (
        <p className="text-muted">No region policies found.</p>
      ) : (
        [...items]
          .sort((a, b) => a.country_code.localeCompare(b.country_code))
          .map((policy) => (
            <form
              key={policy.country_code}
              aria-label={`Policy for ${policy.country_code}`}
              onSubmit={(event) => {
                event.preventDefault();
                void save(policy);
              }}
              aria-busy={action.busy}
              className="card min-w-0 p-4 space-y-4"
            >
              <h3 className="font-semibold">{policy.country_code}</h3>
              <fieldset
                disabled={action.busy}
                className="min-w-0 grid grid-cols-1 md:grid-cols-4 gap-3"
              >
                {(
                  [
                    ["uploads_enabled", "Uploads"],
                    ["comments_enabled", "Comments"],
                    ["discoverability_enabled", "Discoverable"],
                  ] as const
                ).map(([key, label]) => (
                  <label
                    key={key}
                    className="flex items-center justify-between gap-3 border border-line p-3 text-sm"
                  >
                    {label}
                    <input
                      type="checkbox"
                      checked={policy[key]}
                      onChange={(event) =>
                        updateDraft(policy.country_code, {
                          [key]: event.target.checked,
                        })
                      }
                    />
                  </label>
                ))}
                <label className="block min-w-0 text-sm">
                  Moderation
                  <select
                    value={policy.auto_moderation_level}
                    onChange={(event) =>
                      updateDraft(policy.country_code, {
                        auto_moderation_level: event.target
                          .value as RegionPolicyItem["auto_moderation_level"],
                      })
                    }
                    className="field mt-2 !text-base"
                  >
                    <option value="relaxed">Relaxed</option>
                    <option value="standard">Standard</option>
                    <option value="strict">Strict</option>
                  </select>
                </label>
              </fieldset>
              <button type="submit" disabled={action.busy} className="btn-dark">
                Save
              </button>
            </form>
          ))
      )}
      <section
        aria-labelledby="policy-history-title"
        className="card p-5 md:p-6 space-y-4"
      >
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h3 id="policy-history-title" className="font-semibold">
            Policy Change History
          </h3>
          <button
            type="button"
            className="btn-secondary"
            disabled={history.loading || action.busy}
            onClick={history.reload}
          >
            Refresh
          </button>
        </div>
        {history.loading ? (
          <p role="status">Loading policy history…</p>
        ) : history.error ? (
          <p role="alert">
            {history.error}{" "}
            <button className="underline" onClick={history.reload}>
              Retry history
            </button>
          </p>
        ) : (
          <>
            {history.data?.items.length === 0 ? (
              <p className="text-sm text-muted">
                No policy audit events found.
              </p>
            ) : (
              history.data?.items.map((item) => (
                <article
                  key={item.id}
                  className="border border-line p-3 space-y-1 break-words"
                >
                  <p className="text-sm">
                    {String(item.metadata?.country_code ?? "N/A")} /{" "}
                    {item.action}
                  </p>
                  <p className="text-sm text-muted">
                    {new Date(item.created_at).toLocaleString()} /{" "}
                    {item.admin_sub}
                  </p>
                  <p className="text-sm break-all">
                    {JSON.stringify(item.metadata)}
                  </p>
                </article>
              ))
            )}
            <nav
              aria-label="Policy history pages"
              className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-3"
            >
              <p className="text-sm text-muted">
                {total === 0 ? 0 : offset + 1}–{Math.min(offset + 10, total)} of{" "}
                {total}
              </p>
              <div className="flex gap-2">
                <button
                  className="btn-secondary"
                  disabled={offset === 0 || action.busy}
                  onClick={() => setOffset(Math.max(offset - 10, 0))}
                >
                  Prev
                </button>
                <button
                  className="btn-secondary"
                  disabled={offset + 10 >= total || action.busy}
                  onClick={() => setOffset(offset + 10)}
                >
                  Next
                </button>
              </div>
            </nav>
          </>
        )}
      </section>
    </section>
  );
}
