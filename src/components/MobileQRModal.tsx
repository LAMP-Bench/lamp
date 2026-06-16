import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import QRCode from "qrcode";
import { FiX, FiSmartphone, FiCopy, FiCheck } from "react-icons/fi";

/// Shows a QR code for opening the local site on a phone over Wi-Fi. The QR
/// encodes `http://<LAN-IP>:8080/` so a phone on the same network can scan
/// and reach Apache without having to type the IP. Hostnames like
/// `myhost.local` are skipped on purpose — phones don't resolve them, so
/// the URL targets the dev machine's IP directly.
export function MobileQRModal({
  open,
  onClose,
  pathSuffix = "/",
}: {
  open: boolean;
  onClose: () => void;
  pathSuffix?: string;
}) {
  const [ip, setIp] = useState<string | null>(null);
  const [qrSvg, setQrSvg] = useState<string>("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    invoke<string | null>("lan_ip").then(setIp).catch(() => setIp(null));
  }, [open]);

  const url = ip ? `http://${ip}:8080${pathSuffix}` : null;

  useEffect(() => {
    if (!url) {
      setQrSvg("");
      return;
    }
    QRCode.toString(url, { type: "svg", margin: 1, width: 256 })
      .then(setQrSvg)
      .catch(() => setQrSvg(""));
  }, [url]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 bg-black/40 flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-xl shadow-xl p-6 w-[360px] max-w-[90vw]"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 mb-3">
          <FiSmartphone className="text-sky-600 text-lg" />
          <h3 className="font-medium text-neutral-800 flex-1">
            Open on your phone
          </h3>
          <button
            onClick={onClose}
            className="text-neutral-400 hover:text-neutral-700"
          >
            <FiX />
          </button>
        </div>

        <p className="text-xs text-neutral-500 mb-3">
          Scan with your phone&apos;s camera. Phone and dev machine must be on
          the same Wi-Fi network.
        </p>

        <div className="bg-neutral-50 rounded-lg p-4 flex items-center justify-center min-h-[256px]">
          {qrSvg ? (
            <div
              className="size-[224px]"
              dangerouslySetInnerHTML={{ __html: qrSvg }}
            />
          ) : (
            <span className="text-xs text-neutral-400">
              {ip === null ? "Detecting LAN IP…" : "No LAN connection detected"}
            </span>
          )}
        </div>

        {url && (
          <div className="mt-3 flex items-center gap-2 bg-neutral-50 border border-neutral-200 rounded px-2 py-1.5">
            <code className="text-xs flex-1 text-neutral-700 truncate">
              {url}
            </code>
            <button
              onClick={() => {
                navigator.clipboard.writeText(url);
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              }}
              className="text-neutral-500 hover:text-neutral-800"
              title="Copy URL"
            >
              {copied ? <FiCheck className="text-emerald-600" /> : <FiCopy />}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
