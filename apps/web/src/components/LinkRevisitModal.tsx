import React, { useState } from "react";
import { X, Link as LinkIcon, Search, Loader2 } from "lucide-react";
import { api } from "../lib/api";
import { Lettering } from "../types";

const LinkRevisitModal: React.FC<{ originalId: string; onClose: () => void }> = ({ originalId, onClose }) => {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Lettering[]>([]);
  const [loading, setLoading] = useState(false);
  const [notes, setNotes] = useState("");

  const handleSearch = async () => {
    if (query.length < 2) return;
    setLoading(true);
    try {
      const data = await api.search(query);
      setResults(data.filter(i => i.id !== originalId));
    } finally {
      setLoading(false);
    }
  };

  const handleLink = async (targetId: string) => {
    try {
      await api.linkRevisit(originalId, { revisit_lettering_id: targetId, notes: notes.trim() || undefined });
      onClose();
    } catch {
      // Fixed: Removed unused 'e' variable
      alert("Linking failed");
    }
  };

  return (
    <div className="fixed inset-0 z-[110] bg-black/60 flex items-center justify-center p-4">
      <div className="card w-full max-w-lg shadow-card animate-in slide-in-from-bottom-4">
        <div className="bg-ink text-white p-4 flex justify-between items-center">
          <h3 className="font-semibold uppercase text-sm flex items-center gap-2"><LinkIcon size={18} /> Link Timeline Revisit</h3>
          <button onClick={onClose}><X size={20} /></button>
        </div>
        <div className="p-6 space-y-6">
          <div className="flex gap-2">
            <input value={query} onChange={e => setQuery(e.target.value)} placeholder="Search by text or PIN..." className="field flex-1" />
            <button onClick={handleSearch} className="btn-dark"><Search size={20} /></button>
          </div>
          <textarea value={notes} onChange={e => setNotes(e.target.value)} placeholder="What changed? (e.g., Repainted, faded, demolished)" className="field w-full min-h-[80px]" />
          
          <div className="space-y-3 max-h-48 overflow-y-auto">
            {loading ? <Loader2 className="animate-spin mx-auto text-terracotta" /> : 
              results.map(r => (
                <button key={r.id} onClick={() => handleLink(r.id)} className="w-full flex items-center gap-4 border border-line p-2 hover:bg-stone-50 transition-colors">
                  <img src={r.thumbnail_urls.small} className="w-12 h-12 object-cover border border-line" alt="" />
                  <div className="text-left">
                    <p className="text-xs font-semibold uppercase truncate w-40">{r.detected_text || "Discovery"}</p>
                    <p className="text-xs font-bold text-stone-400">PIN {r.pin_code} / {new Date(r.created_at).toLocaleDateString()}</p>
                  </div>
                  <span className="ml-auto text-xs font-semibold uppercase text-terracotta">Select</span>
                </button>
              ))
            }
          </div>
        </div>
      </div>
    </div>
  );
};

export default LinkRevisitModal;