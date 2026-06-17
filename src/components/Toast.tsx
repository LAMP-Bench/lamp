import {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { FiCheckCircle, FiAlertCircle, FiInfo, FiX } from "react-icons/fi";

/// Lightweight non-blocking toast system. Replaces `alert()` calls scattered
/// across the UI. Toasts auto-dismiss after `duration` ms unless the tone is
/// `error`, where the user has to acknowledge.
type ToastKind = "info" | "success" | "error";

type Toast = {
  id: number;
  kind: ToastKind;
  message: string;
};

type Ctx = {
  push: (kind: ToastKind, message: string, duration?: number) => void;
};

const ToastCtx = createContext<Ctx | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const push = useCallback(
    (kind: ToastKind, message: string, duration = 4000) => {
      const id = Date.now() + Math.random();
      setToasts((prev) => [...prev, { id, kind, message }]);
      if (kind !== "error") {
        setTimeout(() => dismiss(id), duration);
      }
    },
    [dismiss],
  );

  const ctx = useMemo(() => ({ push }), [push]);

  return (
    <ToastCtx.Provider value={ctx}>
      {children}
      <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm pointer-events-none">
        {toasts.map((t) => (
          <ToastItem key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

function ToastItem({
  toast,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: () => void;
}) {
  const colors =
    toast.kind === "error"
      ? "bg-red-50 border-red-200 text-red-800"
      : toast.kind === "success"
      ? "bg-emerald-50 border-emerald-200 text-emerald-800"
      : "bg-sky-50 border-sky-200 text-sky-800";
  const Icon =
    toast.kind === "error"
      ? FiAlertCircle
      : toast.kind === "success"
      ? FiCheckCircle
      : FiInfo;
  return (
    <div
      className={`pointer-events-auto rounded-lg border shadow-sm px-3 py-2 text-sm flex items-start gap-2 ${colors}`}
    >
      <Icon className="mt-0.5 shrink-0" />
      <span className="flex-1 break-words">{toast.message}</span>
      <button
        onClick={onDismiss}
        className="p-0.5 rounded hover:bg-black/10 shrink-0"
        title="Dismiss"
      >
        <FiX />
      </button>
    </div>
  );
}

export function useToast() {
  const ctx = useContext(ToastCtx);
  if (!ctx) throw new Error("useToast outside ToastProvider");
  return ctx.push;
}

/// Replacement for `window.confirm`. Renders a centered modal with a
/// configurable primary action tone. Returns a promise that resolves to
/// the user's choice.
type ConfirmOpts = {
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "danger" | "default";
};

type ConfirmCtx = {
  ask: (opts: ConfirmOpts) => Promise<boolean>;
};

const ConfirmCtxObj = createContext<ConfirmCtx | null>(null);

export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<
    | null
    | (ConfirmOpts & { resolve: (v: boolean) => void })
  >(null);

  const ask = useCallback((opts: ConfirmOpts) => {
    return new Promise<boolean>((resolve) => {
      setPending({ ...opts, resolve });
    });
  }, []);

  function answer(v: boolean) {
    if (pending) {
      pending.resolve(v);
      setPending(null);
    }
  }

  useEffect(() => {
    if (!pending) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") answer(false);
      if (e.key === "Enter") answer(true);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending]);

  return (
    <ConfirmCtxObj.Provider value={{ ask }}>
      {children}
      {pending && (
        <div className="fixed inset-0 z-50 bg-black/30 flex items-center justify-center p-6">
          <div className="bg-white rounded-lg shadow-xl max-w-md w-full p-5">
            <p className="text-sm text-neutral-800 whitespace-pre-wrap break-words">
              {pending.message}
            </p>
            <div className="mt-4 flex items-center gap-2 justify-end">
              <button
                onClick={() => answer(false)}
                className="px-3 py-1.5 rounded border border-neutral-300 text-sm hover:bg-neutral-50"
              >
                {pending.cancelLabel ?? "Cancel"}
              </button>
              <button
                onClick={() => answer(true)}
                autoFocus
                className={`px-3 py-1.5 rounded text-sm font-medium text-white ${
                  pending.tone === "danger"
                    ? "bg-red-600 hover:bg-red-700"
                    : "bg-sky-600 hover:bg-sky-700"
                }`}
              >
                {pending.confirmLabel ?? "Confirm"}
              </button>
            </div>
          </div>
        </div>
      )}
    </ConfirmCtxObj.Provider>
  );
}

export function useConfirm() {
  const ctx = useContext(ConfirmCtxObj);
  if (!ctx) throw new Error("useConfirm outside ConfirmProvider");
  return ctx.ask;
}
