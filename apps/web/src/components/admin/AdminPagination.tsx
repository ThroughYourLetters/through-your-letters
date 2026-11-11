export default function AdminPagination({
  offset,
  limit,
  total,
  disabled,
  onChange,
}: {
  offset: number;
  limit: number;
  total: number;
  disabled: boolean;
  onChange: (offset: number) => void;
}) {
  return (
    <nav
      aria-label="Result pages"
      className="flex flex-wrap items-center justify-between gap-3"
    >
      <p>
        {total === 0 ? 0 : offset + 1}–{Math.min(offset + limit, total)} of{" "}
        {total}
      </p>
      <div className="flex gap-2">
        <button
          className="btn-outline"
          disabled={disabled || offset === 0}
          onClick={() => onChange(Math.max(0, offset - limit))}
        >
          Previous page
        </button>
        <button
          className="btn-outline"
          disabled={disabled || offset + limit >= total}
          onClick={() => onChange(offset + limit)}
        >
          Next page
        </button>
      </div>
    </nav>
  );
}
