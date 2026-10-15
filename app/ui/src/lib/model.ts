import type { Request } from "@/bindings";

/**
 * The generated bindings mark every `#[serde(default)]` field as optional (`json?: string`),
 * because Rust *accepts* the field missing when reading YAML. Values that come *from* Rust
 * always have every field, though. `Complete<T>` is the same type with all fields required,
 * derived from the generated one, so nothing is duplicated by hand.
 */
export type Complete<T> = T extends (infer U)[]
  ? Complete<U>[]
  : T extends object
    ? { [K in keyof T]-?: Complete<T[K]> }
    : T;

/** A request as returned by Rust: every field present. */
export type RequestModel = Complete<Request>;

/** Marks a value received from a Rust command as complete (see {@link Complete}). */
export const fromRust = <T>(value: T) => value as Complete<T>;
