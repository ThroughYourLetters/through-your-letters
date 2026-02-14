import React, { useState, useEffect } from "react";
import { Trophy, FolderOpen, Target, Loader2, Plus, Heart } from "lucide-react";
import { LeaderboardEntry, CollectionSummary, ChallengeData } from "../types";
import { useToastStore } from "../store/useToastStore";
import { api } from "../lib/api";

type Tab = "leaderboard" | "collections" | "challenges";

const CommunityPage: React.FC<{
  onContributorClick?: (tag: string) => void;
}> = ({ onContributorClick }) => {
  const [tab, setTab] = useState<Tab>("leaderboard");
  const [leaderboard, setLeaderboard] = useState<LeaderboardEntry[]>([]);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [challenges, setChallenges] = useState<ChallengeData[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newCollection, setNewCollection] = useState({
    name: "",
    description: "",
    creator_tag: "",
  });
  const { addToast } = useToastStore();

  useEffect(() => {
    setLoading(true);
    if (tab === "leaderboard") {
      api
        .getLeaderboard()
        .then((data) => setLeaderboard(data))
        .catch(() => {
          setLeaderboard([]);
          addToast("Failed to load community data", "error");
        })
        .finally(() => setLoading(false));
      return;
    }

    if (tab === "collections") {
      api
        .getCollections()
        .then((data) => setCollections(data))
        .catch(() => {
          setCollections([]);
          addToast("Failed to load community data", "error");
        })
        .finally(() => setLoading(false));
      return;
    }

    api
      .getChallenges()
      .then((data) => setChallenges(data))
      .catch(() => {
        setChallenges([]);
        addToast("Failed to load community data", "error");
      })
      .finally(() => setLoading(false));
  }, [tab, addToast]);

  const handleCreateCollection = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newCollection.name.trim() || !newCollection.creator_tag.trim()) return;
    try {
      await api.createCollection(newCollection);
      addToast("Collection created", "success");
      setShowCreate(false);
      setNewCollection({ name: "", description: "", creator_tag: "" });
      const data = await api.getCollections();
      setCollections(data);
    } catch {
      addToast("Failed to create collection", "error");
    }
  };

  const tabs: { key: Tab; label: string; icon: React.ReactNode }[] = [
    { key: "leaderboard", label: "Leaderboard", icon: <Trophy size={16} /> },
    {
      key: "collections",
      label: "Collections",
      icon: <FolderOpen size={16} />,
    },
    { key: "challenges", label: "Challenges", icon: <Target size={16} /> },
  ];

  return (
    <div className="space-y-12 pb-24 animate-in">
      <div className="border-b-4 border-black pb-8">
        <h2 className="text-4xl md:text-6xl font-black uppercase tracking-tighter">
          Community
        </h2>
        <p className="text-[10px] font-black uppercase text-slate-400 tracking-widest mt-2">
          Leaderboards, collections, and challenges
        </p>
      </div>

      {/* Tabs */}
      <div className="flex border-2 border-black">
        {tabs.map((t) => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={`flex-1 flex items-center justify-center gap-2 py-4 text-[10px] font-black uppercase tracking-widest transition-colors ${
              tab === t.key
                ? "bg-black text-white"
                : "bg-white text-slate-400 hover:bg-slate-50"
            }`}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>

      {loading ? (
        <div className="flex justify-center py-20">
          <Loader2 size={32} className="animate-spin text-[#cc543a]" />
        </div>
      ) : (
        <>
          {/* Leaderboard */}
          {tab === "leaderboard" && (
            <div className="border-4 border-black bg-white">
              <div className="bg-black text-white px-6 py-4 text-[10px] font-black uppercase tracking-widest flex items-center gap-2">
                <Trophy size={16} className="text-[#d4a017]" />
                Top Contributors
              </div>
              {leaderboard.length === 0 ? (
                <p className="text-center py-12 text-slate-400 font-black text-sm uppercase">
                  No contributions yet
                </p>
              ) : (
                <div className="divide-y divide-black/10">
                  {leaderboard.map((entry, idx) => (
                    <div
                      key={entry.tag}
                      className="flex items-center px-6 py-4 hover:bg-slate-50 transition-colors"
                    >
                      <span
                        className={`w-10 text-2xl font-black ${
                          idx === 0
                            ? "text-[#d4a017]"
                            : idx === 1
                              ? "text-slate-400"
                              : idx === 2
                                ? "text-[#cd7f32]"
                                : "text-slate-300"
                        }`}
                      >
                        {idx + 1}
                      </span>
                      <button
                        onClick={() => onContributorClick?.(entry.tag)}
                        className="flex-1 text-left font-black uppercase text-sm hover:text-[#cc543a] transition-colors"
                      >
                        {entry.tag}
                      </button>
                      <div className="flex items-center gap-4">
                        <span className="text-[10px] font-black text-slate-500">
                          {entry.count} uploads
                        </span>
                        <span className="flex items-center gap-1 text-[10px] font-black text-[#cc543a]">
                          <Heart size={12} /> {entry.total_likes}
                        </span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Collections */}
          {tab === "collections" && (
            <div className="space-y-8">
              <div className="flex justify-end">
                <button
                  onClick={() => setShowCreate(!showCreate)}
                  className="flex items-center gap-2 bg-black text-white px-4 py-3 text-[10px] font-black uppercase hover:bg-[#cc543a] transition-colors"
                >
                  <Plus size={14} />
                  New Collection
                </button>
              </div>

              {showCreate && (
                <form
                  onSubmit={handleCreateCollection}
                  className="border-4 border-black p-6 bg-white space-y-4"
                >
                  <input
                    placeholder="Collection Name"
                    className="w-full border-2 border-black p-3 font-black text-sm outline-none focus:border-[#cc543a]"
                    value={newCollection.name}
                    onChange={(e) =>
                      setNewCollection({
                        ...newCollection,
                        name: e.target.value,
                      })
                    }
                    required
                  />
                  <input
                    placeholder="Your Name"
                    className="w-full border-2 border-black p-3 font-black text-sm outline-none focus:border-[#cc543a]"
                    value={newCollection.creator_tag}
                    onChange={(e) =>
                      setNewCollection({
                        ...newCollection,
                        creator_tag: e.target.value,
                      })
                    }
                    required
                  />
                  <textarea
                    placeholder="Description (optional)"
                    className="w-full border-2 border-black p-3 font-medium text-sm outline-none focus:border-[#cc543a]"
                    rows={3}
                    value={newCollection.description}
                    onChange={(e) =>
                      setNewCollection({
                        ...newCollection,
                        description: e.target.value,
                      })
                    }
                  />
                  <button
                    type="submit"
                    className="bg-[#cc543a] text-white px-6 py-3 font-black text-[10px] uppercase hover:bg-black transition-colors"
                  >
                    Create
                  </button>
                </form>
              )}

              {collections.length === 0 ? (
                <p className="text-center py-12 text-slate-400 font-black text-sm uppercase">
                  No collections yet. Create the first one.
                </p>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                  {collections.map((c) => (
                    <div
                      key={c.id}
                      className="border-2 border-black bg-white p-6 brutalist-shadow-sm hover:-translate-y-1 transition-all space-y-3"
                    >
                      <h3 className="font-black text-lg uppercase">{c.name}</h3>
                      {c.description && (
                        <p className="text-sm text-slate-600 font-medium">
                          {c.description}
                        </p>
                      )}
                      <div className="flex justify-between items-center text-[9px] font-black uppercase text-slate-400">
                        <span>By {c.creator_tag}</span>
                        <span>{c.item_count} items</span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Challenges */}
          {tab === "challenges" && (
            <div className="space-y-6">
              {challenges.length === 0 ? (
                <p className="text-center py-12 text-slate-400 font-black text-sm uppercase">
                  No active challenges. Check back soon.
                </p>
              ) : (
                challenges.map((ch) => {
                  const progress = Math.min(
                    100,
                    Math.round((ch.current_count / ch.target_count) * 100),
                  );
                  return (
                    <div
                      key={ch.id}
                      className="border-4 border-black bg-white p-6 space-y-4"
                    >
                      <div className="flex justify-between items-start">
                        <div>
                          <h3 className="font-black text-xl uppercase">
                            {ch.title}
                          </h3>
                          {ch.description && (
                            <p className="text-sm text-slate-600 font-medium mt-1">
                              {ch.description}
                            </p>
                          )}
                        </div>
                        <div className="bg-[#cc543a] text-white px-3 py-1 text-[10px] font-black uppercase">
                          {ch.status}
                        </div>
                      </div>

                      {ch.target_script && (
                        <span className="inline-block bg-slate-100 border border-black/10 px-2 py-1 text-[9px] font-black uppercase">
                          Script: {ch.target_script}
                        </span>
                      )}
                      {ch.target_area && (
                        <span className="inline-block bg-slate-100 border border-black/10 px-2 py-1 text-[9px] font-black uppercase ml-2">
                          Area: {ch.target_area}
                        </span>
                      )}

                      <div className="space-y-2">
                        <div className="flex justify-between text-[10px] font-black uppercase">
                          <span>
                            {ch.current_count} / {ch.target_count}
                          </span>
                          <span>{progress}%</span>
                        </div>
                        <div className="w-full h-4 bg-slate-100 border-2 border-black">
                          <div
                            className="h-full bg-[#cc543a] transition-all duration-500"
                            style={{ width: `${progress}%` }}
                          />
                        </div>
                      </div>

                      {ch.ends_at && (
                        <p className="text-[9px] font-bold text-slate-400">
                          Ends: {new Date(ch.ends_at).toLocaleDateString()}
                        </p>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
};

export default CommunityPage;
