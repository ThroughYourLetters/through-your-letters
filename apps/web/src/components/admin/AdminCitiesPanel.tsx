import { pageLimits } from "@/lib/runtimeConfig";
import { useCallback, useRef, useState, type FormEvent } from "react";
import { Globe } from "lucide-react";
import { api } from "../../lib/api";
import {
  useAdminAction,
  useAdminResource,
  type AdminSessionProps,
} from "./useAdminOperations";

export default function AdminCitiesPanel(session: AdminSessionProps) {
  const [draft, setDraft] = useState({
    query: "",
    countryCode: "",
    limit: "100",
  });
  const [filters, setFilters] = useState(draft);
  const [validationError, setValidationError] = useState("");
  const [notice, setNotice] = useState("");
  const formRef = useRef<HTMLFormElement>(null);
  const queryRef = useRef<HTMLInputElement>(null);
  const action = useAdminAction(session);
  const load = useCallback(
    (signal: AbortSignal) =>
      api.getCities({
        q: filters.query.trim() || undefined,
        countryCode: filters.countryCode || undefined,
        limit: Math.min(Number(filters.limit), pageLimits.cities.max_limit),
        offset: 0,
        signal,
      }),
    [filters],
  );
  const cities = useAdminResource(session, load);
  function applyFilters(event: FormEvent) {
    event.preventDefault();
    setValidationError("");
    setFilters({ ...draft });
  }
  async function discover(capitals = false) {
    if (action.busy || !formRef.current?.reportValidity()) return;
    setValidationError("");
    setNotice("");
    if (!capitals && draft.query.trim().length < 2) {
      setValidationError("Enter at least 2 characters to discover cities.");
      queryRef.current?.focus();
      return;
    }
    if (
      capitals &&
      !window.confirm(
        "Add capital cities? Existing city records will also be updated.",
      )
    )
      return;
    const result = await action.run(() =>
      capitals
        ? api.adminBootstrapCapitals({ limit: Number(draft.limit) })
        : api.adminDiscoverCities({
            query: draft.query.trim(),
            country_code: draft.countryCode || undefined,
            limit: Math.min(
              Number(draft.limit),
              pageLimits.city_discovery.max_limit,
            ),
          }),
    );
    if (result.ok) {
      const { processed, upserted, failed } = result.value;
      setNotice(
        `Checked ${processed} / Added or updated ${upserted} / Failed ${failed}`,
      );
      setFilters({ ...draft });
    }
  }
  return (
    <section aria-labelledby="cities-title" className="space-y-6">
      <form
        ref={formRef}
        onSubmit={applyFilters}
        aria-labelledby="cities-title"
        className="card p-5 md:p-6 space-y-4"
      >
        <h2
          id="cities-title"
          className="flex items-center gap-2 text-lg font-semibold"
        >
          <Globe size={18} />
          Global City Operations
        </h2>
        <fieldset
          disabled={action.busy}
          className="grid min-w-0 grid-cols-1 md:grid-cols-3 gap-3"
        >
          <label className="block text-sm font-medium">
            Search city
            <input
              ref={queryRef}
              value={draft.query}
              onChange={(event) =>
                setDraft({ ...draft, query: event.target.value })
              }
              placeholder="Search city"
              className="field mt-2 !text-base"
              aria-describedby={
                validationError ? "city-query-error" : undefined
              }
              aria-invalid={!!validationError}
            />
          </label>
          <label className="block text-sm font-medium">
            Country code (optional)
            <input
              value={draft.countryCode}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  countryCode: event.target.value.toUpperCase(),
                })
              }
              placeholder="Country code (US, IN)"
              maxLength={2}
              pattern="[A-Za-z]{2}"
              title="Use a two-letter country code, such as US or IN."
              className="field mt-2 !text-base"
            />
          </label>
          <label className="block text-sm font-medium">
            Result limit
            <input
              type="number"
              min={1}
              max={pageLimits.capitals.max_limit}
              step={1}
              required
              value={draft.limit}
              onChange={(event) =>
                setDraft({ ...draft, limit: event.target.value })
              }
              className="field mt-2 !text-base"
              aria-describedby="city-discovery-limit"
            />
          </label>
        </fieldset>
        <p id="city-discovery-limit" className="text-sm text-muted">
          City discovery uses at most {pageLimits.city_discovery.max_limit}{" "}
          results per search.
        </p>
        {validationError && (
          <p
            id="city-query-error"
            role="alert"
            className="text-sm text-red-700"
          >
            {validationError}
          </p>
        )}
        {action.error && (
          <p role="alert" className="text-sm text-red-700 break-words">
            {action.error}
          </p>
        )}
        {notice && (
          <p role="status" className="text-sm">
            {notice}
          </p>
        )}
        <div className="flex flex-wrap gap-3">
          <button type="submit" disabled={action.busy} className="btn-dark">
            Load
          </button>
          <button
            type="button"
            disabled={action.busy}
            onClick={() => void discover()}
            className="btn-secondary"
          >
            Find cities
          </button>
          <button
            type="button"
            disabled={action.busy}
            onClick={() => void discover(true)}
            className="btn-secondary"
          >
            Add capital cities
          </button>
        </div>
        {action.busy && <p role="status">Updating cities…</p>}
      </form>
      {cities.loading ? (
        <p role="status">Loading cities…</p>
      ) : cities.error ? (
        <p role="alert">
          {cities.error}{" "}
          <button className="underline" onClick={cities.reload}>
            Retry cities
          </button>
        </p>
      ) : cities.data?.length === 0 ? (
        <p className="text-muted">No cities found for current filter.</p>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {cities.data?.map((city) => (
            <article
              key={city.id}
              className="card min-w-0 p-4 space-y-2 break-words"
            >
              <div className="flex flex-wrap justify-between gap-2">
                <h3 className="min-w-0 text-sm font-semibold">{city.name}</h3>
                <span className="text-sm">{city.country_code}</span>
              </div>
              <p className="text-sm text-muted">
                {city.center_lat?.toFixed(4) ?? "-"},{" "}
                {city.center_lng?.toFixed(4) ?? "-"} / zoom{" "}
                {city.default_zoom ?? "-"}
              </p>
              <p className="text-sm">
                {city.description || "No encyclopedia summary yet"}
              </p>
              <p
                className={`text-sm font-medium ${city.is_active ? "text-green-700" : "text-muted"}`}
              >
                {city.is_active ? "Active" : "Inactive"}
              </p>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
