export default function AdminFilterPresets({
  items,
  onSave,
  onApply,
  onRemove,
}: {
  items: { id: string; name: string }[];
  onSave: () => void;
  onApply: (id: string) => void;
  onRemove: (id: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      <button className="btn-outline" onClick={onSave}>
        Save filter preset
      </button>
      {items.slice(0, 4).map((item) => (
        <div key={item.id} className="flex">
          <button className="btn-outline" onClick={() => onApply(item.id)}>
            Apply {item.name}
          </button>
          <button
            className="btn-outline"
            aria-label={`Remove preset ${item.name}`}
            onClick={() => onRemove(item.id)}
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
