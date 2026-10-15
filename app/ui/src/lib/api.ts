import { toast } from "sonner";
import type { ApiError } from "@/bindings";

/** The shape tauri-specta gives every command that returns `Result<T, ApiError>`. */
type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: ApiError };

/** A failed command, carrying the structured error from Rust. */
export class FridaError extends Error {
  readonly apiError: ApiError;

  constructor(apiError: ApiError) {
    super(apiError.message);
    this.apiError = apiError;
  }
}

/** Awaits a command and returns its data, or throws a {@link FridaError}. */
export async function unwrap<T>(result: Promise<CommandResult<T>>): Promise<T> {
  const r = await result;
  if (r.status === "error") throw new FridaError(r.error);
  return r.data;
}

/** The structured error behind `e`, or a generic one for unexpected exceptions. */
export function toApiError(e: unknown): ApiError {
  if (e instanceof FridaError) return e.apiError;
  return { code: "io", message: e instanceof Error ? e.message : String(e), hint: null };
}

/** Shows an error toast with the message and, if present, the hint. */
export function showError(e: unknown) {
  const error = toApiError(e);
  toast.error(error.message, { description: error.hint ?? undefined });
}
