export interface AdminStats {
  total_uploads: number;
  pending_approvals: number;
  approved: number;
  rejected: number;
  total_cities: number;
  total_likes: number;
  total_comments: number;
}

export default function AdminStatsPanel({ stats }: { stats: AdminStats }) {
  const counts = [
    ["Uploads", stats.total_uploads],
    ["Awaiting review", stats.pending_approvals],
    ["Approved", stats.approved],
    ["Rejected", stats.rejected],
    ["Cities", stats.total_cities],
    ["Likes", stats.total_likes],
    ["Comments", stats.total_comments],
  ] as const;

  return (
    <section className="card p-6 space-y-6" aria-labelledby="admin-stats-title">
      <h2 id="admin-stats-title" className="font-display text-2xl font-semibold">
        Activity
      </h2>
      <dl className="grid grid-cols-2 gap-6 md:grid-cols-4">
        {counts.map(([label, value]) => (
          <div key={label}>
            <dt className="text-sm text-muted">{label}</dt>
            <dd className="mt-1 font-display text-3xl font-semibold">
              {value.toLocaleString()}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
