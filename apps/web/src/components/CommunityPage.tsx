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
        .then((data) => setCollections(data.items))
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
    if (!newCollection.name.trim()) return;
    try {
      await api.createCollection(newCollection);
      addToast("Collection created", "success");
      setShowCreate(false);
      setNewCollection({ name: "", description: "" });
      const data = await api.getCollections();
      setCollections(data.items);
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
      <div className="border-b border-line pb-8">
        <h2 className="font-display font-normal text-4xl md:text-6xl tracking-tight">
          Community
        </h2>
        <p className="text-xs font-semibold uppercase text-stone-400 tracking-widest mt-2">
          Leaderboards, collections, and challenges
        </p>
      </div>

      {/* Tabs */}
      <div className="flex w-fit max-w-full overflow-x-auto rounded-full border border-line bg-surface p-1 gap-1">
        {tabs.map((t) => (
          <button
            key={t.key}
            onClick={() => setTab(t.key)}
            className={`flex-1 flex items-center justify-center gap-2 rounded-full px-4 py-2 text-sm font-medium transition-colors whitespace-nowrap ${tab === t.key ? "bg-ink text-paper" : "text-muted hover:text-ink"}`}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>

      {loading ? (
        <div className="flex justify-center py-20">
          <Loader2 size={32} className="animate-spin text-terracotta" />
        </div>
      ) : (
        <>
          {/* Leaderboard */}
          {tab === "leaderboard" && (
            <div className="card overflow-hidden">
              <div className="border-b border-line px-6 py-4 eyebrow text-ink flex items-center gap-2">
                <Trophy size={16} className="text-ochre" />
                Top Contributors
              </div>
              {leaderboard.length === 0 ? (
                <p className="text-center py-12 text-stone-400 font-semibold text-sm uppercase">
                  No contributions yet
                </p>
              ) : (
                <div className="divide-y divide-line">
                  {leaderboard.map((entry, idx) => (
                    <div
                      key={entry.tag}
                      className="flex items-center px-6 py-4 hover:bg-stone-50 transition-colors"
                    >
                      <span
                        className={`w-10 font-display text-2xl ${ idx === 0 ? "text-ochre" : idx === 1 ? "text-stone-400" : idx === 2 ? "text-[#cd7f32]" : "text-stone-300" }`}
                      >
                        {idx + 1}
                      </span>
                      <button
                        onClick={() => onContributorClick?.(entry.tag)}
                        className="flex-1 text-left font-medium hover:text-terracotta transition-colors"
                      >
                        {entry.tag}
                      </button>
                      <div className="flex items-center gap-4">
                        <span className="text-xs font-semibold text-stone-500">
                          {entry.count} uploads
                        </span>
                        <span className="flex items-center gap-1 text-xs font-semibold text-terracotta">
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
                  className="btn-dark"
                >
                  <Plus size={14} />
                  New Collection
                </button>
              </div>

              {showCreate && (
                <form
                  onSubmit={handleCreateCollection}
                  className="border border-line p-6 bg-surface space-y-4"
                >
                  <input
                    placeholder="Collection Name"
                    className="field w-full"
                    value={newCollection.name}
                    onChange={(e) =>
                      setNewCollection({
                        ...newCollection,
                        name: e.target.value,
                      })
                    }
                    required
                  />
                  <textarea
                    placeholder="Description (optional)"
                    className="field w-full"
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
                    className="btn-primary"
                  >
                    Create
                  </button>
                </form>
              )}

              {collections.length === 0 ? (
                <p className="text-center py-12 text-stone-400 font-semibold text-sm uppercase">
                  No collections yet. Create the first one.
                </p>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                  {collections.map((c) => (
                    <div
                      key={c.id}
                      className="card p-6 hover:shadow-card hover:-translate-y-0.5 transition-all space-y-3"
                    >
                      <h3 className="font-display text-2xl">{c.name}</h3>
                      {c.description && (
                        <p className="text-sm text-stone-600 font-medium">
                          {c.description}
                        </p>
                      )}
                      <div className="flex justify-between items-center text-xs font-semibold uppercase text-stone-400">
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
                <p className="text-center py-12 text-stone-400 font-semibold text-sm uppercase">
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
                      className="card p-6 space-y-4"
                    >
                      <div className="flex justify-between items-start">
                        <div>
                          <h3 className="font-display font-normal tracking-tight text-xl">
                            {ch.title}
                          </h3>
                          {ch.description && (
                            <p className="text-sm text-stone-600 font-medium mt-1">
                              {ch.description}
                            </p>
                          )}
                        </div>
                        <div className="chip border-terracotta/30 bg-terracotta-soft text-terracotta-dark">
                          {ch.status}
                        </div>
                      </div>

                      {ch.target_script && (
                        <span className="chip">
                          Script: {ch.target_script}
                        </span>
                      )}
                      {ch.target_area && (
                        <span className="chip ml-2">
                          Area: {ch.target_area}
                        </span>
                      )}

                      <div className="space-y-2">
                        <div className="flex justify-between text-xs font-semibold uppercase">
                          <span>
                            {ch.current_count} / {ch.target_count}
                          </span>
                          <span>{progress}%</span>
                        </div>
                        <div className="w-full h-2 rounded-full bg-stone-100 overflow-hidden">
                          <div
                            className="h-full bg-terracotta transition-all duration-500"
                            style={{ width: `${progress}%` }}
                          />
                        </div>
                      </div>

                      {ch.ends_at && (
                        <p className="text-xs font-bold text-stone-400">
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
