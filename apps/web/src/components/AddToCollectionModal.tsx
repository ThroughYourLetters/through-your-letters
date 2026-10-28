import React, { useEffect, useState } from "react";
import { X, Plus, Loader2, FolderPlus } from "lucide-react";
import { api } from "../lib/api";
import { CollectionSummary } from "../types";

const AddToCollectionModal: React.FC<{ letteringId: string; onClose: () => void }> = ({ letteringId, onClose }) => {
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [processingId, setProcessingId] = useState<string | null>(null);

  useEffect(() => {
    api.getCollections().then((data) => setCollections(data.items)).finally(() => setLoading(false));
  }, []);

  const handleAdd = async (colId: string) => {
    setProcessingId(colId);
    try {
      await api.addToCollection(colId, letteringId);
      onClose();
    } finally {
      setProcessingId(null);
    }
  };

  return (
    <div className="fixed inset-0 z-[110] bg-black/60 flex items-center justify-center p-4">
      <div className="card w-full max-w-md shadow-card animate-in zoom-in-95">
        <div className="bg-ink text-white p-4 flex justify-between items-center">
          <h3 className="font-semibold uppercase text-sm flex items-center gap-2">
            <FolderPlus size={18} /> Curate Specimen
          </h3>
          <button onClick={onClose} className="hover:text-terracotta"><X size={20} /></button>
        </div>
        <div className="p-6 space-y-4 max-h-[60vh] overflow-y-auto">
          {loading ? <Loader2 className="animate-spin mx-auto text-terracotta" /> :
            collections.length === 0 ? <p className="text-center font-semibold uppercase text-xs text-stone-400 py-8">No collections found</p> :
            collections.map(col => (
              <button key={col.id} onClick={() => handleAdd(col.id)} disabled={!!processingId}
                className="w-full border border-line p-4 text-left font-semibold uppercase text-xs hover:bg-stone-50 flex justify-between items-center group">
                {col.name}
                {processingId === col.id ? <Loader2 size={14} className="animate-spin" /> : <Plus size={14} className="group-hover:text-terracotta" />}
              </button>
            ))
          }
        </div>
      </div>
    </div>
  );
};

export default AddToCollectionModal;
