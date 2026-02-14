import React, { useCallback, useEffect, useMemo, useState } from "react";
import { Globe2, Loader2, RefreshCw, Save } from "lucide-react";
import { AdminAuditLogItem, RegionPolicyItem, api } from "../../lib/api";
import { useToastStore } from "../../store/useToastStore";

type PolicyDraft = RegionPolicyItem;

const AdminRegionPoliciesPanel: React.FC = () => {
  const { addToast } = useToastStore();
  const [countryCode, setCountryCode] = useState("");
  const [items, setItems] = useState<PolicyDraft[]>([]);
  const [loading, setLoading] = useState(false);
  const [savingCode, setSavingCode] = useState<string | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyItems, setHistoryItems] = useState<AdminAuditLogItem[]>([]);
  const [historyOffset, setHistoryOffset] = useState(0);
  const [historyTotal, setHistoryTotal] = useState(0);
  const historyLimit = 10;

  const loadPolicies = useCallback(async () => {
    setLoading(true);
    try {
      const response = await api.adminGetRegionPolicies({
        countryCode: countryCode.trim() || undefined,
        limit: 500,
        offset: 0,
      });
      setItems(response.items);
    } catch (e) {
      addToast(
        (e as Error).message || "Failed to load region policies",
        "error",
      );
    } finally {
      setLoading(false);
    }
  }, [addToast, countryCode]);

  useEffect(() => {
    void loadPolicies();
  }, [loadPolicies]);

  const loadHistory = useCallback(async () => {
    setHistoryLoading(true);
    try {
      const response = await api.adminGetAuditLogs({
        action: "UPSERT_REGION_POLICY",
        countryCode: countryCode.trim() || undefined,
        limit: historyLimit,
        offset: historyOffset,
      });
      setHistoryItems(response.items);
      setHistoryTotal(response.total);
    } catch (e) {
      addToast(
        (e as Error).message || "Failed to load policy history",
        "error",
      );
    } finally {
      setHistoryLoading(false);
    }
  }, [addToast, countryCode, historyOffset]);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  useEffect(() => {
    setHistoryOffset(0);
  }, [countryCode]);

  const hasRows = items.length > 0;
  const sorted = useMemo(
    () =>
      [...items].sort((a, b) => a.country_code.localeCompare(b.country_code)),
    [items],
  );

  const updateDraft = (
    code: string,
    changes: Partial<
      Pick<
        PolicyDraft,
        | "uploads_enabled"
        | "comments_enabled"
        | "discoverability_enabled"
        | "auto_moderation_level"
      >
    >,
  ) => {
    setItems((prev) =>
      prev.map((item) =>
        item.country_code === code ? { ...item, ...changes } : item,
      ),
    );
  };

  const savePolicy = async (policy: PolicyDraft) => {
    setSavingCode(policy.country_code);
    try {
      const updated = await api.adminUpsertRegionPolicy(policy.country_code, {
        uploads_enabled: policy.uploads_enabled,
        comments_enabled: policy.comments_enabled,
        discoverability_enabled: policy.discoverability_enabled,
        auto_moderation_level: policy.auto_moderation_level,
      });
      setItems((prev) =>
        prev.map((item) =>
          item.country_code === updated.country_code ? updated : item,
        ),
      );
      addToast(`Policy updated for ${policy.country_code}`, "success");
    } catch (e) {
      addToast((e as Error).message || "Policy update failed", "error");
    } finally {
      setSavingCode(null);
    }
  };

  const createPolicyForCountry = async () => {
    const code = countryCode.trim().toUpperCase();
    if (!/^[A-Z]{2}$/.test(code)) {
      addToast("Enter a valid 2-letter country code", "warning");
      return;
    }

    setSavingCode(code);
    try {
      const created = await api.adminUpsertRegionPolicy(code, {
        uploads_enabled: true,
        comments_enabled: true,
        discoverability_enabled: true,
        auto_moderation_level: "standard",
      });
      setItems((prev) => {
        const without = prev.filter(
          (p) => p.country_code !== created.country_code,
        );
        return [created, ...without];
      });
      addToast(`Policy created for ${code}`, "success");
    } catch (e) {
      addToast((e as Error).message || "Failed to create policy", "error");
    } finally {
      setSavingCode(null);
    }
  };

  return (
    <div className="space-y-8">
      <div className="bg-white border-4 border-black p-5 md:p-6 space-y-4">
        <div className="flex items-center gap-2">
          <Globe2 size={16} className="text-[#cc543a]" />
          <h3 className="text-sm font-black uppercase tracking-widest">
            Region Policy Controls
          </h3>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          <input
            value={countryCode}
            onChange={(e) => setCountryCode(e.target.value.toUpperCase())}
            placeholder="Country code (US, IN)"
            maxLength={2}
            className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase tracking-widest outline-none"
          />
          <button
            onClick={() => void loadPolicies()}
            className="bg-black text-white px-4 py-2 text-[10px] font-black uppercase flex items-center justify-center gap-2 hover:bg-[#cc543a] transition-colors"
          >
            {loading ? (
              <Loader2 size={14} className="animate-spin" />
            ) : (
              <RefreshCw size={14} />
            )}
            Load
          </button>
          <button
            onClick={createPolicyForCountry}
            className="border-2 border-black px-4 py-2 text-[10px] font-black uppercase hover:bg-slate-100 transition-colors"
          >
            Create / Upsert
          </button>
        </div>
      </div>

      {!hasRows ? (
        <div className="text-center py-16 border-4 border-dashed border-black/10 font-black uppercase text-slate-400">
          {loading ? "Loading policies..." : "No region policies found"}
        </div>
      ) : (
        <div className="space-y-4">
          {sorted.map((policy) => (
            <div
              key={policy.country_code}
              className="border-2 border-black bg-white p-4 space-y-4"
            >
              <div className="flex items-center justify-between gap-3">
                <h4 className="text-sm font-black uppercase tracking-widest">
                  {policy.country_code}
                </h4>
                <button
                  onClick={() => savePolicy(policy)}
                  disabled={savingCode === policy.country_code}
                  className="bg-black text-white px-3 py-2 text-[10px] font-black uppercase flex items-center gap-2 hover:bg-[#cc543a] transition-colors disabled:opacity-50"
                >
                  {savingCode === policy.country_code ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : (
                    <Save size={14} />
                  )}
                  Save
                </button>
              </div>

              <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
                <label className="flex items-center justify-between border-2 border-black px-3 py-2 text-[10px] font-black uppercase">
                  Uploads
                  <input
                    type="checkbox"
                    checked={policy.uploads_enabled}
                    onChange={(e) =>
                      updateDraft(policy.country_code, {
                        uploads_enabled: e.target.checked,
                      })
                    }
                  />
                </label>

                <label className="flex items-center justify-between border-2 border-black px-3 py-2 text-[10px] font-black uppercase">
                  Comments
                  <input
                    type="checkbox"
                    checked={policy.comments_enabled}
                    onChange={(e) =>
                      updateDraft(policy.country_code, {
                        comments_enabled: e.target.checked,
                      })
                    }
                  />
                </label>

                <label className="flex items-center justify-between border-2 border-black px-3 py-2 text-[10px] font-black uppercase">
                  Discoverable
                  <input
                    type="checkbox"
                    checked={policy.discoverability_enabled}
                    onChange={(e) =>
                      updateDraft(policy.country_code, {
                        discoverability_enabled: e.target.checked,
                      })
                    }
                  />
                </label>

                <label className="border-2 border-black px-3 py-2 text-[10px] font-black uppercase flex items-center justify-between gap-2">
                  Moderation
                  <select
                    value={policy.auto_moderation_level}
                    onChange={(e) =>
                      updateDraft(policy.country_code, {
                        auto_moderation_level: e.target
                          .value as PolicyDraft["auto_moderation_level"],
                      })
                    }
                    className="border border-black px-2 py-1 text-[10px] font-black uppercase bg-white"
                  >
                    <option value="relaxed">Relaxed</option>
                    <option value="standard">Standard</option>
                    <option value="strict">Strict</option>
                  </select>
                </label>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="border-4 border-black bg-white p-5 md:p-6 space-y-4">
        <div className="flex items-center justify-between gap-3">
          <h4 className="text-sm font-black uppercase tracking-widest">
            Policy Change History
          </h4>
          <button
            onClick={() => void loadHistory()}
            className="border-2 border-black px-3 py-1 text-[10px] font-black uppercase hover:bg-slate-100"
          >
            Refresh
          </button>
        </div>

        {historyLoading ? (
          <div className="flex justify-center py-8">
            <Loader2 size={24} className="animate-spin text-[#cc543a]" />
          </div>
        ) : historyItems.length === 0 ? (
          <p className="text-sm text-slate-500">
            No policy audit events found.
          </p>
        ) : (
          <div className="space-y-2">
            {historyItems.map((item) => (
              <div
                key={item.id}
                className="border-2 border-black p-3 bg-slate-50"
              >
                <p className="text-[10px] font-black uppercase">
                  {(item.metadata?.country_code as string) || "N/A"} /{" "}
                  {item.action}
                </p>
                <p className="text-[10px] font-bold uppercase text-slate-500">
                  {new Date(item.created_at).toLocaleString()} /{" "}
                  {item.admin_sub}
                </p>
                <p className="text-xs text-slate-700 break-all">
                  {JSON.stringify(item.metadata)}
                </p>
              </div>
            ))}
          </div>
        )}

        <div className="flex items-center justify-between border-t-2 border-black/10 pt-3">
          <p className="text-[10px] font-black uppercase text-slate-500">
            {historyOffset + 1}-
            {Math.min(historyOffset + historyLimit, historyTotal)} of{" "}
            {historyTotal}
          </p>
          <div className="flex gap-2">
            <button
              onClick={() =>
                setHistoryOffset(Math.max(historyOffset - historyLimit, 0))
              }
              disabled={historyOffset === 0}
              className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
            >
              Prev
            </button>
            <button
              onClick={() => setHistoryOffset(historyOffset + historyLimit)}
              disabled={historyOffset + historyLimit >= historyTotal}
              className="border-2 border-black px-3 py-1 text-[9px] font-black uppercase disabled:opacity-40"
            >
              Next
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default AdminRegionPoliciesPanel;
