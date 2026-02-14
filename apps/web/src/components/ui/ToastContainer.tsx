import React from "react";
import { useToastStore } from "../../store/useToastStore";
import { X, CheckCircle, AlertCircle, Info } from "lucide-react";

const ToastContainer: React.FC = () => {
  const { toasts, removeToast } = useToastStore();

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-40 md:bottom-4 right-4 z-[120] flex flex-col gap-2 pointer-events-none">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`
            pointer-events-auto flex items-center gap-3 min-w-[300px] max-w-sm p-4
            border-2 border-black brutalist-shadow-sm animate-in slide-in-from-bottom-5 fade-in
            ${toast.type === "success" ? "bg-white text-black" : ""}
            ${toast.type === "error" ? "bg-red-50 text-red-900 border-red-900" : ""}
            ${toast.type === "info" ? "bg-blue-50 text-blue-900 border-blue-900" : ""}
          `}
        >
          {toast.type === "success" && (
            <CheckCircle size={20} className="text-green-600" />
          )}
          {toast.type === "error" && (
            <AlertCircle size={20} className="text-red-600" />
          )}
          {toast.type === "info" && (
            <Info size={20} className="text-blue-600" />
          )}

          <p className="flex-1 text-sm font-bold">{toast.message}</p>

          <button
            onClick={() => removeToast(toast.id)}
            className="text-slate-400 hover:text-black transition-colors"
          >
            <X size={16} />
          </button>
        </div>
      ))}
    </div>
  );
};

export default ToastContainer;
