import type { TFunction } from "i18next";
import type { ServiceError } from "./types";

/// Render a start/stop failure for a human.
///
/// `backend` errors already carry a specific message from Rust ("httpd binary
/// not found at …", "port 8080 is already used by nginx") so we prefix them
/// with the service name and pass them straight through. `exited` has no
/// message of its own — the process just vanished — so we spell out the two
/// causes that account for nearly all of them.
export function serviceErrorText(
  t: TFunction,
  label: string,
  error: ServiceError,
): string {
  return error.kind === "backend"
    ? `${label}: ${error.message}`
    : t("services.exitedImmediately", { service: label });
}
