import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { invalidatePublicLetterings } from "../../lib/publicQueries";
import { locationLabel } from "../../types";
import type {
  AppliedRepair,
  LocationRecord,
  RepairEvidence,
  RepairPreview,
} from "../../types/locationRepairs";

function position(record: { lat: number | null; lng: number | null }) {
  return record.lat === null || record.lng === null
    ? "No coordinates"
    : `${record.lat}, ${record.lng}`;
}

function History({ id }: { id: string }) {
  const [after, setAfter] = useState<string>();
  const history = useQuery({
    queryKey: ["admin", "location-history", id, after],
    queryFn: ({ signal }) => api.getLocationHistory(id, after, signal),
  });
  return (
    <section aria-label="Correction history" className="space-y-3">
      <h3 className="font-semibold">Correction history</h3>
      {history.isLoading && <p role="status">Loading history...</p>}
      {history.isError && (
        <p role="alert">
          Could not load history.{" "}
          <button onClick={() => void history.refetch()}>Retry history</button>
        </p>
      )}
      {history.data?.items.length === 0 && <p>No corrections recorded.</p>}
      {history.data?.items.map((repair) => (
        <article
          key={repair.id}
          className="rounded-xl border border-line p-3 space-y-1 break-words"
        >
          <p>
            {new Date(repair.created_at).toLocaleString()} ·{" "}
            {repair.reviewed_by}
          </p>
          <p>
            {position(repair.before_location)} →{" "}
            {position(repair.after_location)}
          </p>
          <a
            href={repair.evidence.source_url}
            target="_blank"
            rel="noopener noreferrer"
            className="underline"
          >
            {repair.evidence.source_name}
          </a>
          <p>{repair.evidence.reason}</p>
        </article>
      ))}
      {!!history.data?.next_cursor && (
        <button
          className="btn-secondary"
          onClick={() => setAfter(history.data!.next_cursor!)}
        >
          Older corrections
        </button>
      )}
    </section>
  );
}

function RepairForm({
  record,
  onClose,
  onApplied,
}: {
  record: LocationRecord;
  onClose: () => void;
  onApplied: (repair: AppliedRepair) => void;
}) {
  const [preview, setPreview] = useState<RepairPreview | null>(null);
  const [reviewed, setReviewed] = useState(false);
  const [evidence, setEvidence] = useState<
    Record<keyof RepairEvidence, string>
  >({
    lat: "",
    lng: "",
    city_name: "",
    country_code: "",
    pin_code: "",
    source_name: "",
    source_url: "",
    reason: "",
  });
  const previewMutation = useMutation({
    mutationFn: () =>
      api.previewLocationRepair(record.id, {
        ...evidence,
        lat: Number(evidence.lat),
        lng: Number(evidence.lng),
      }),
    onSuccess: setPreview,
  });
  const applyMutation = useMutation({
    mutationFn: (value: RepairPreview) => api.applyLocationRepair(value),
    onSuccess: onApplied,
  });
  const busy = previewMutation.isPending || applyMutation.isPending;
  return (
    <div className="space-y-5">
      <button className="btn-secondary" disabled={busy} onClick={onClose}>
        Back to report
      </button>
      <h3 className="text-lg font-semibold">
        {record.city_name} · {record.pin_code}
      </h3>
      <img
        src={record.image_url}
        alt="Upload being reviewed"
        className="max-h-64 max-w-full rounded-xl object-contain"
      />
      <p>
        Current: {position(record)}. {locationLabel(record.location_source)}
      </p>
      <form
        className="space-y-4"
        onSubmit={(event) => {
          event.preventDefault();
          setReviewed(false);
          setPreview(null);
          applyMutation.reset();
          previewMutation.mutate();
        }}
      >
        <fieldset disabled={busy} className="grid gap-3 sm:grid-cols-2">
          {(
            [
              ["city_name", "Source city"],
              ["country_code", "Source country code"],
              ["pin_code", "Source postal code"],
              ["lat", "Latitude"],
              ["lng", "Longitude"],
              ["source_name", "Source name"],
              ["source_url", "Source URL"],
              ["reason", "Reason for correction"],
            ] as const
          ).map(([key, label]) => (
            <label key={key} className="flex flex-col gap-1 min-w-0">
              {label}
              <input
                required
                className="rounded-lg border border-line bg-surface p-2 min-w-0"
                type={
                  key === "source_url"
                    ? "url"
                    : key === "lat" || key === "lng"
                      ? "number"
                      : "text"
                }
                step={key === "lat" || key === "lng" ? "any" : undefined}
                min={key === "lat" ? -90 : key === "lng" ? -180 : undefined}
                max={key === "lat" ? 90 : key === "lng" ? 180 : undefined}
                value={evidence[key]}
                onChange={(event) => {
                  setEvidence({ ...evidence, [key]: event.target.value });
                  setPreview(null);
                  setReviewed(false);
                  previewMutation.reset();
                  applyMutation.reset();
                }}
              />
            </label>
          ))}
        </fieldset>
        <p>
          Use a postal authority or a checked map source. Postal coordinates
          remain approximate.
        </p>
        <button className="btn-secondary" disabled={busy} type="submit">
          {previewMutation.isPending ? "Checking..." : "Preview correction"}
        </button>
      </form>
      {previewMutation.error && (
        <p role="alert">{previewMutation.error.message}</p>
      )}
      {preview && (
        <section
          aria-label="Correction preview"
          className="rounded-xl border border-line p-4 space-y-3"
        >
          <h3 className="font-semibold">Preview correction</h3>
          <p>
            {position(preview.current)} → {position(preview.evidence)}
          </p>
          <a
            href={preview.evidence.source_url}
            target="_blank"
            rel="noopener noreferrer"
            className="underline"
          >
            Open source: {preview.evidence.source_name}
          </a>
          <label className="flex items-start gap-2">
            <input
              type="checkbox"
              checked={reviewed}
              disabled={busy}
              onChange={(event) => setReviewed(event.target.checked)}
            />{" "}
            I checked the coordinates, city, and postal code against the source.
          </label>
          <button
            className="btn-primary"
            disabled={!reviewed || busy}
            onClick={() => applyMutation.mutate(preview)}
          >
            {applyMutation.isPending ? "Saving..." : "Approve correction"}
          </button>
          {applyMutation.error && (
            <p role="alert">
              {applyMutation.error.message} Preview again if the upload changed.
            </p>
          )}
        </section>
      )}
      <History id={record.id} />
    </div>
  );
}

export default function AdminLocationsPanel() {
  const client = useQueryClient();
  const [cursors, setCursors] = useState<Array<string | undefined>>([
    undefined,
  ]);
  const [selected, setSelected] = useState<LocationRecord | null>(null);
  const [applied, setApplied] = useState<AppliedRepair | null>(null);
  const [includeResolved, setIncludeResolved] = useState(false);
  const after = cursors.at(-1);
  const report = useQuery({
    queryKey: ["admin", "location-repairs", after, includeResolved],
    queryFn: ({ signal }) =>
      api.getLocationRepairs(after, signal, includeResolved),
  });
  async function onApplied(repair: AppliedRepair) {
    setSelected(null);
    setApplied(repair);
    await Promise.all([
      client.invalidateQueries({ queryKey: ["admin", "location-repairs"] }),
      client.invalidateQueries({
        queryKey: ["admin", "location-history", repair.lettering_id],
      }),
      invalidatePublicLetterings(client),
    ]);
  }
  return (
    <section className="space-y-5" aria-label="Location repairs">
      <h2 className="text-xl font-semibold">Location repairs</h2>
      <p>
        Review unverified and unresolved uploads. Previewing does not change the
        upload.
      </p>
      {!selected && (
        <label className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={includeResolved}
            onChange={(event) => {
              setIncludeResolved(event.target.checked);
              setCursors([undefined]);
            }}
          />
          Show resolved uploads
        </label>
      )}
      {applied && (
        <div role="status" className="rounded-xl border border-line p-3">
          Correction saved for {applied.after_location.city_name}{" "}
          {applied.after_location.pin_code}.{" "}
          <button
            className="underline"
            onClick={() => {
              setSelected(applied.after_location);
              setApplied(null);
            }}
          >
            View correction history
          </button>
        </div>
      )}
      {selected ? (
        <RepairForm
          key={selected.id}
          record={selected}
          onClose={() => setSelected(null)}
          onApplied={(repair) => void onApplied(repair)}
        />
      ) : (
        <>
          {report.isLoading && <p role="status">Loading location report...</p>}
          {report.isError && (
            <p role="alert">
              Could not load location report.{" "}
              <button onClick={() => void report.refetch()}>
                Retry report
              </button>
            </p>
          )}
          {report.data && (
            <>
              {report.data.items.length === 0 && (
                <p>No uploads need review on this page.</p>
              )}
              <ul className="space-y-2">
                {report.data.items.map((record) => (
                  <li
                    key={record.id}
                    className="rounded-xl border border-line p-3 flex flex-wrap justify-between gap-3"
                  >
                    <div>
                      <p>
                        {record.city_name} · {record.pin_code}
                      </p>
                      <p>
                        {locationLabel(record.location_source)} ·{" "}
                        {position(record)}
                      </p>
                      {record.location_error && (
                        <p>
                          Lookup failed:{" "}
                          {record.location_error.replaceAll("_", " ")}
                        </p>
                      )}
                    </div>
                    <button
                      className="btn-secondary"
                      onClick={() => {
                        setApplied(null);
                        setSelected(record);
                      }}
                    >
                      Review location
                    </button>
                  </li>
                ))}
              </ul>
              <div className="flex flex-wrap gap-3">
                <button
                  className="btn-secondary"
                  disabled={cursors.length === 1}
                  onClick={() => setCursors(cursors.slice(0, -1))}
                >
                  Previous page
                </button>
                <button
                  className="btn-secondary"
                  disabled={!report.data.next_cursor}
                  onClick={() =>
                    setCursors([...cursors, report.data!.next_cursor!])
                  }
                >
                  Next page
                </button>
                <button
                  className="btn-secondary"
                  onClick={() => void report.refetch()}
                >
                  Refresh report
                </button>
              </div>
            </>
          )}
        </>
      )}
    </section>
  );
}
